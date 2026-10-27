# Browser example

Install the built npm tarball into this directory, then start Vite:

```sh
npm install /absolute/path/to/bionic-reader-base-vVERSION.tgz
npm run dev
```

Use `npm run build` and `npm run preview` to check the production bundle. This
example uses Vite 8 and requires Node 22.12+; the library itself supports Node 22+.
The package ships its WebAssembly asset. Engine loading happens explicitly before
rendering, with no text-processing service or runtime CDN. Loading failures appear
in the status message. Once initialized, rendering is synchronous and whole-document
limits apply, including the 8 MiB Markdown input limit.

Try `window.bionicReader` in the developer console to call the installed package's
public API directly. The visual preview uses literal text nodes and the returned
text segments; the formatted output is displayed as text.
