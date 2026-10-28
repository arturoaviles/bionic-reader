package bionicreader

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"math"
	"os"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"
)

func newTestReader(t *testing.T) *Reader {
	t.Helper()
	r, err := NewReader(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := r.Close(); err != nil {
			t.Error(err)
		}
	})
	return r
}

func TestSharedCorpus(t *testing.T) {
	r := newTestReader(t)
	data, err := os.ReadFile("tests/fixtures/library.json")
	if err != nil {
		t.Fatal(err)
	}
	var corpus struct {
		Render []struct {
			Name, Text string
			Format     Format
			Options    json.RawMessage
			Output     string
			Error      string
		}
		Segments []struct {
			Name, Text string
			Options    Options
			Output     []TextSegment
		}
	}
	if err := json.Unmarshal(data, &corpus); err != nil {
		t.Fatal(err)
	}
	for _, test := range corpus.Render {
		t.Run(test.Name, func(t *testing.T) {
			var options Options
			decoder := json.NewDecoder(bytes.NewReader(test.Options))
			decoder.DisallowUnknownFields()
			err := decoder.Decode(&options)
			var output string
			if err == nil {
				output, err = r.Render(context.Background(), test.Text, test.Format, options)
			}
			if test.Error != "" {
				if err == nil {
					t.Fatal("expected invalid input to fail")
				}
				return
			}
			if err != nil || output != test.Output {
				t.Fatalf("output=%q err=%v; want %q", output, err, test.Output)
			}
		})
	}
	for _, test := range corpus.Segments {
		t.Run(test.Name, func(t *testing.T) {
			output, err := r.Segments(context.Background(), test.Text, test.Options)
			if err != nil || !reflect.DeepEqual(output, test.Output) {
				t.Fatalf("output=%+v err=%v; want %+v", output, err, test.Output)
			}
		})
	}
}

func TestVersionAndExplicitConfiguration(t *testing.T) {
	digest := sha256.Sum256(engine)
	if hex.EncodeToString(digest[:]) != engineSHA256 {
		t.Fatal("embedded engine differs from its generated identity")
	}
	r := newTestReader(t)
	options, err := r.ParseConfig(context.Background(), "profile = 'strong'\nfraction = 0.25\n")
	if err != nil {
		t.Fatal(err)
	}
	output, err := r.Render(context.Background(), "reading", HTML, options)
	if err != nil || output != "<strong>re</strong>ading" {
		t.Fatalf("output=%q err=%v", output, err)
	}
	options, err = r.ParseConfig(context.Background(), "style = 'never'")
	if err != nil {
		t.Fatal(err)
	}
	output, err = r.Render(context.Background(), "Hello", HTML, options)
	if err != nil || output != "Hello" {
		t.Fatalf("output=%q err=%v", output, err)
	}
}

func TestInvalidEngineInitialization(t *testing.T) {
	original := engine
	engine = []byte("invalid WebAssembly")
	defer func() { engine = original }()
	r, err := NewReader(context.Background())
	var failure *Error
	if r != nil || !errors.As(err, &failure) || failure.Code != "initialization_failed" {
		t.Fatalf("expected initialization failure, got reader=%v err=%v", r, err)
	}
}

func TestExplicitZeroFalseAndInvalidInputs(t *testing.T) {
	r := newTestReader(t)
	for _, options := range []Options{{Fraction: Pointer(0.0)}, {Enabled: Pointer(false)}} {
		output, err := r.Render(context.Background(), "Hello", HTML, options)
		if err != nil || output != "Hello" {
			t.Fatalf("output=%q err=%v", output, err)
		}
	}
	for _, options := range []Options{
		{Fraction: Pointer(math.NaN())}, {Fraction: Pointer(1.5)},
		{Every: Pointer(uint32(0))}, {Profile: "unknown"},
		{LengthRules: []LengthRule{{MaxLength: 8, Fraction: 0.5}, {MaxLength: 4, Fraction: 0.2}}},
	} {
		_, err := r.Render(context.Background(), "Hello", HTML, options)
		var failure *Error
		if !errors.As(err, &failure) || failure.Code != "invalid_input" {
			t.Fatalf("expected invalid_input, got %v", err)
		}
	}
	for _, call := range []func() error{
		func() error { _, e := r.Render(context.Background(), "\xff", HTML, Options{}); return e },
		func() error { _, e := r.Segments(context.Background(), "\xff", Options{}); return e },
		func() error { _, e := r.ParseConfig(context.Background(), "\xff"); return e },
		func() error { _, e := r.ParseConfig(context.Background(), "unknown = true"); return e },
		func() error { _, e := r.Render(context.Background(), "Hello", "", Options{}); return e },
	} {
		var failure *Error
		if err := call(); !errors.As(err, &failure) || failure.Code != "invalid_input" {
			t.Fatalf("expected invalid_input, got %v", err)
		}
	}
}

func TestReaderConcurrentCallsAndClose(t *testing.T) {
	r := newTestReader(t)
	var tasks sync.WaitGroup
	for range 8 {
		tasks.Go(func() {
			output, err := r.Render(context.Background(), "Hello world", HTML, Options{Every: Pointer(uint32(2))})
			if err != nil || output != "<strong>Hel</strong>lo world" {
				t.Errorf("output=%q err=%v", output, err)
			}
		})
	}
	tasks.Wait()
	if err := r.Close(); err != nil {
		t.Fatal(err)
	}
	if err := r.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := r.Render(context.Background(), "Hello", HTML, Options{}); !errors.Is(err, ErrClosed) {
		t.Fatalf("expected closed reader, got %v", err)
	}
	var zero Reader
	if _, err := zero.Segments(context.Background(), "Hello", Options{}); !errors.Is(err, ErrClosed) {
		t.Fatalf("expected uninitialized reader to fail, got %v", err)
	}
}

func TestCancellationDoesNotCloseReader(t *testing.T) {
	initial, cancel := context.WithCancel(context.Background())
	r, err := NewReader(initial)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = r.Close() })
	cancel()
	if _, err := r.Render(initial, "Hello", HTML, Options{}); !errors.Is(err, context.Canceled) {
		t.Fatalf("expected canceled context, got %v", err)
	}
	if _, err := NewReader(initial); !errors.Is(err, context.Canceled) {
		t.Fatalf("expected canceled initialization, got %v", err)
	}
	deadline, stop := context.WithTimeout(context.Background(), time.Millisecond)
	defer stop()
	_, err = r.Render(deadline, strings.Repeat("Hello world ", 300000), HTML, Options{})
	if !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("expected execution deadline, got %v", err)
	}
	output, err := r.Render(context.Background(), "Hello", HTML, Options{})
	if err != nil || output != "<strong>Hel</strong>lo" {
		t.Fatalf("reader failed after canceled instance: output=%q err=%v", output, err)
	}
}

func TestMarkdownLimit(t *testing.T) {
	r := newTestReader(t)
	_, err := r.Render(context.Background(), strings.Repeat("a", 8*1024*1024+1), HTML, Options{InputFormat: "markdown"})
	var failure *Error
	if !errors.As(err, &failure) || failure.Code != "invalid_input" {
		t.Fatalf("expected Markdown limit, got %v", err)
	}
}
