import { hydrateRoot } from 'react-dom/client';
import { createReader } from '@arturoaviles/bionic-reader';
import { createApp } from './app.js';

const reader = await createReader();
hydrateRoot(document.querySelector('#root'), createApp(reader), {
  onRecoverableError: (error) => console.error(error),
});
