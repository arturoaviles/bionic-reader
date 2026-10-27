import React from 'react';
import { BionicReaderProvider, BionicText } from '@arturoaviles/bionic-reader/react';

export const text = '<script> Hello\nworld café ภาษาไทย';

function HydrationStatus() {
  const [hydrated, setHydrated] = React.useState(false);
  React.useEffect(() => setHydrated(true), []);
  return React.createElement('p', { id: 'hydration', 'data-ready': hydrated },
    hydrated ? 'Hydrated' : 'Server rendered');
}

function ReadingDemo() {
  const [options, setOptions] = React.useState({});
  const [content, setContent] = React.useState(text);
  return React.createElement(React.Fragment, null,
    React.createElement('h1', null, 'Bionic Reader with React'),
    React.createElement(BionicText, { id: 'reading', text: content, options }),
    React.createElement('div', null,
      React.createElement('button', { id: 'emphasis-off', onClick: () => setOptions({ enabled: false }) }, 'Disable emphasis'),
      React.createElement('button', { id: 'emphasis-change', onClick: () => setOptions({ profile: 'strong', fraction: 1, every: 2 }) }, 'Emphasize whole selected words'),
      React.createElement('button', { id: 'control-sample', onClick: () => setContent('\u001b[31mHello\u001b[0m\tworld\ncafé ภาษาไทย') }, 'Show control-filtered sample')),
    React.createElement(HydrationStatus));
}

export function createApp(reader) {
  return React.createElement(BionicReaderProvider, { reader }, React.createElement(ReadingDemo));
}
