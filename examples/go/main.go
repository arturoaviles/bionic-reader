package main

import (
	"context"
	"fmt"
	"log"

	bionicreader "github.com/arturoaviles/bionic-reader"
)

func main() {
	if err := run(); err != nil {
		log.Fatal(err)
	}
}

func run() error {
	ctx := context.Background()
	reader, err := bionicreader.NewReader(ctx)
	if err != nil {
		return err
	}
	defer reader.Close()
	html, err := reader.Render(ctx, "Hello world", bionicreader.HTML, bionicreader.Options{Profile: "balanced"})
	if err != nil {
		return err
	}
	fmt.Println(html)
	parts, err := reader.Segments(ctx, "Hello world", bionicreader.Options{})
	if err != nil {
		return err
	}
	fmt.Printf("%+v\n", parts)
	return nil
}
