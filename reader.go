// Package bionicreader transforms text and Markdown with bionic reading emphasis.
// It embeds the shared Rust engine and runs locally without CGO, Rust, a CLI,
// or a network service. Reuse a Reader and close it when finished.
package bionicreader

import (
	"bytes"
	"context"
	_ "embed"
	"encoding/json"
	"errors"
	"fmt"
	"sync"
	"unicode/utf8"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
	"github.com/tetratelabs/wazero/sys"
)

//go:embed internal/engine/engine.wasm
var engine []byte

// Format is the explicitly selected document output representation.
type Format string

const (
	ANSI     Format = "ansi"
	Plain    Format = "plain"
	Markdown Format = "markdown"
	HTML     Format = "html"
)

// LengthRule sets an emphasis fraction through an inclusive word-length limit.
type LengthRule struct {
	MaxLength uint32  `json:"max_length"`
	Fraction  float64 `json:"fraction"`
}

// Options overrides balanced defaults. Nil pointers leave defaults unchanged;
// Pointer(false) and Pointer(0.0) explicitly disable emphasis or prefix selection.
// Word counts must be positive and rules must have increasing thresholds.
type Options struct {
	Enabled       *bool        `json:"enabled,omitempty"`
	Profile       string       `json:"profile,omitempty"`
	Fraction      *float64     `json:"fraction,omitempty"`
	Every         *uint32      `json:"every,omitempty"`
	MinWordLength *uint32      `json:"min_word_length,omitempty"`
	LengthRules   []LengthRule `json:"length_rules,omitempty"`
	Emphasis      string       `json:"emphasis,omitempty"`
	InputFormat   string       `json:"input_format,omitempty"`
	Controls      string       `json:"controls,omitempty"`
	SkipCode      *bool        `json:"skip_code,omitempty"`
}

// Pointer returns a pointer to a setting, including a zero or false value.
func Pointer[T any](value T) *T { return &value }

// TextSegment is literal visible text with its bionic reading emphasis state.
type TextSegment struct {
	Text       string `json:"text"`
	Emphasized bool   `json:"emphasized"`
}

// Error is a transformation or engine failure with a stable machine-readable code.
// Codes are invalid_input, initialization_failed, engine_failed, and closed_reader.
// Context cancellation and deadline errors are returned directly.
type Error struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func (e *Error) Error() string { return e.Message }

// ErrClosed is returned when using a closed or uninitialized Reader.
var ErrClosed = &Error{Code: "closed_reader", Message: "bionic reader is closed or uninitialized"}

// Reader owns a reusable compiled engine. Calls are safe concurrently and use
// isolated instances without filesystem or network access. It must not be copied.
// Close waits for active calls; cancel their contexts to interrupt them.
type Reader struct {
	mu       sync.RWMutex
	runtime  wazero.Runtime
	compiled wazero.CompiledModule
	closed   bool
}

// NewReader loads and validates the embedded engine. No files are discovered or
// downloaded. The context bounds initialization, not the lifetime of the reader.
func NewReader(ctx context.Context) (*Reader, error) {
	if err := ctx.Err(); err != nil {
		return nil, err
	}
	runtime := wazero.NewRuntimeWithConfig(ctx, wazero.NewRuntimeConfig().WithCloseOnContextDone(true))
	reader := &Reader{runtime: runtime}
	fail := func(err error) (*Reader, error) {
		_ = runtime.Close(context.Background())
		if ctx.Err() != nil {
			return nil, ctx.Err()
		}
		return nil, &Error{Code: "initialization_failed", Message: err.Error()}
	}
	if _, err := wasi_snapshot_preview1.Instantiate(ctx, runtime); err != nil {
		return fail(err)
	}
	compiled, err := runtime.CompileModule(ctx, engine)
	if err != nil {
		return fail(err)
	}
	reader.compiled = compiled
	var version string
	if err := reader.call(ctx, map[string]any{"op": "version"}, &version); err != nil {
		return fail(err)
	}
	if version != Version {
		return fail(fmt.Errorf("engine version %q differs from package %q", version, Version))
	}
	return reader, nil
}

// Close releases engine resources and is idempotent. It waits for active calls.
func (r *Reader) Close() error {
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.closed || r.runtime == nil {
		return nil
	}
	r.closed = true
	return r.runtime.Close(context.Background())
}

// Render transforms a whole Unicode string in the explicitly selected format.
// Markdown input is limited to 8 MiB of UTF-8. Other inputs are bounded by
// available memory and the WebAssembly linear-memory limit. No configuration
// files, terminal state, NO_COLOR, or TERM are inspected.
func (r *Reader) Render(ctx context.Context, text string, format Format, options Options) (string, error) {
	var value string
	err := r.callText(ctx, text, map[string]any{
		"op": "render", "text": text, "format": format, "options": options,
	}, &value)
	return value, err
}

// Segments returns ordered literal text segments without terminal controls or
// byte offsets. Markdown input retains source syntax and protects code by default.
func (r *Reader) Segments(ctx context.Context, text string, options Options) ([]TextSegment, error) {
	var value []TextSegment
	err := r.callText(ctx, text, map[string]any{
		"op": "segments", "text": text, "options": options,
	}, &value)
	return value, err
}

// ParseConfig parses explicitly supplied TOML contents, performing no file I/O.
// A style of never disables emphasis; auto is deterministically enabled. The
// caller still chooses Render's format explicitly. Session-only fields are ignored.
func (r *Reader) ParseConfig(ctx context.Context, contents string) (Options, error) {
	var value Options
	err := r.callText(ctx, contents, map[string]any{
		"op": "parse_config", "contents": contents,
	}, &value)
	return value, err
}

func (r *Reader) callText(ctx context.Context, text string, request any, output any) error {
	if !utf8.ValidString(text) {
		return &Error{Code: "invalid_input", Message: "text must be valid UTF-8"}
	}
	return r.call(ctx, request, output)
}

func (r *Reader) call(ctx context.Context, request any, output any) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	data, err := json.Marshal(request)
	if err != nil {
		return &Error{Code: "invalid_input", Message: err.Error()}
	}
	r.mu.RLock()
	defer r.mu.RUnlock()
	if r.closed || r.runtime == nil || r.compiled == nil {
		return ErrClosed
	}
	var stdout bytes.Buffer
	module, err := r.runtime.InstantiateModule(ctx, r.compiled, wazero.NewModuleConfig().
		WithName("").WithStdin(bytes.NewReader(data)).WithStdout(&stdout))
	if module != nil {
		defer module.Close(context.Background())
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	var exit *sys.ExitError
	if err != nil && !(errors.As(err, &exit) && exit.ExitCode() == 0) {
		return &Error{Code: "engine_failed", Message: err.Error()}
	}
	var response struct {
		Value json.RawMessage `json:"value"`
		Error *Error          `json:"error"`
	}
	if err := json.Unmarshal(stdout.Bytes(), &response); err != nil {
		return &Error{Code: "engine_failed", Message: "invalid engine response: " + err.Error()}
	}
	if response.Error != nil {
		return response.Error
	}
	if err := json.Unmarshal(response.Value, output); err != nil {
		return &Error{Code: "engine_failed", Message: "invalid engine value: " + err.Error()}
	}
	return nil
}
