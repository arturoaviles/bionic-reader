# Next.js server rendering

This Pages Router example pins Next.js 16.4.3 and React 19.3.1 and uses Webpack.
Install the npm package, build, then start:

This example uses the application APIs documented in the
[library guide](../../docs/LIBRARY.md).

```sh
npm install @arturoaviles/bionic-reader
npm run build
npm start
```

Open http://127.0.0.1:4175. Server and browser initialize their readers at module
load before React renders. Both use identical plain text and settings; only
serializable page props cross the Next.js boundary. No reader functions are
serialized from server to client. The browser loads the bundled WebAssembly asset
and hydrates the server-rendered text. The component's rendering lifecycle never
starts engine initialization.

The server externalizes the package so its Node loader can read the shipped engine
asset. Browser imports select the browser entry point. Initialization errors fail
the module load instead of silently changing the reading style. This example
validates the Pages Router/Webpack combination; it does not establish every
Next.js router/bundler combination.
