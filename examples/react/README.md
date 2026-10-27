# React server rendering and hydration

Install the npm package, build the client bundle, and start the example:

This example requires the forthcoming library release; the currently published
npm package provides the CLI.

```sh
npm install @arturoaviles/bionic-reader
npm run build
npm start
```

Open http://127.0.0.1:4174. The server initializes its reader before accepting
requests. The browser initializes its reader before calling `hydrateRoot`.
Both render the same text and settings through `BionicReaderProvider` and
`BionicText`; component rendering never loads the engine. The status changes to
"Hydrated" after hydration completes.

The component accepts plain text, preserves whitespace, and presents markup
literally. Use `emphasisClassName` or `emphasisStyle` to customize the prefixes.
Core JavaScript consumers do not need React. The adapter supports React 18.3 and
19; this example pins React 19 and Vite 8, requiring Node 22.12+.
