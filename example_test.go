package bionicreader_test

import (
	"context"
	"fmt"

	bionicreader "github.com/arturoaviles/bionic-reader"
)

func ExampleReader_Render() {
	ctx := context.Background()
	reader, err := bionicreader.NewReader(ctx)
	if err != nil {
		panic(err)
	}
	defer reader.Close()
	html, err := reader.Render(ctx, "Hello world", bionicreader.HTML, bionicreader.Options{})
	if err != nil {
		panic(err)
	}
	fmt.Println(html)
	// Output: <strong>Hel</strong>lo <strong>wor</strong>ld
}
