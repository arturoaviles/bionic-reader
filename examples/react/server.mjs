import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { renderToString } from 'react-dom/server';
import { createReader } from '@arturoaviles/bionic-reader';
import { createApp } from './app.js';

const directory = new URL('./dist/', import.meta.url);
const template = await readFile(new URL('index.html', directory), 'utf8');
const reader = await createReader();
const mime = { '.js': 'text/javascript', '.wasm': 'application/wasm', '.css': 'text/css' };
const server = createServer((request, response) => {
  async function serve() {
    const path = new URL(request.url, 'http://localhost').pathname;
    if (path === '/') {
      response.setHeader('Content-Type', 'text/html; charset=utf-8');
      response.end(template.replace('<!--ssr-outlet-->', () => renderToString(createApp(reader))));
      return;
    }
    const asset = new URL(`.${path}`, directory);
    if (!path.startsWith('/assets/') || !asset.href.startsWith(directory.href)) {
      response.writeHead(404).end();
      return;
    }
    const extension = path.slice(path.lastIndexOf('.'));
    response.setHeader('Content-Type', mime[extension] ?? 'application/octet-stream');
    response.end(await readFile(asset));
  }
  serve().catch((error) => {
    console.error(error);
    if (!response.headersSent) response.writeHead(500);
    response.end('Example request failed');
  });
});
server.listen(Number(process.env.BIONIC_EXAMPLE_PORT ?? 4174), '127.0.0.1');
process.once('SIGTERM', () => server.close());
