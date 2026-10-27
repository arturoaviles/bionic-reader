'use client';
'use strict';

const React = require('react');
const ReaderContext = React.createContext(null);

function failure(message, code) {
  return Object.assign(new Error(message), { name: 'BionicError', code });
}

function BionicReaderProvider({ reader, children }) {
  if (!reader || typeof reader.render !== 'function' || typeof reader.segments !== 'function') {
    throw failure('BionicReaderProvider requires an initialized reader; await createReader() first', 'initialization_failed');
  }
  return React.createElement(ReaderContext.Provider, { value: reader }, children);
}

function BionicText({ text, options, emphasisClassName, emphasisStyle, style, ...attributes }) {
  const reader = React.useContext(ReaderContext);
  if (!reader) throw failure('BionicText requires BionicReaderProvider with an initialized reader', 'initialization_failed');
  if ('dangerouslySetInnerHTML' in attributes || 'children' in attributes) {
    throw failure('BionicText accepts plain text through the text prop', 'invalid_input');
  }
  const parts = reader.segments(text, { ...options, input_format: 'text', controls: 'safe' });
  const prefixStyle = { ...(emphasisClassName ? {} : { fontWeight: 700 }), ...emphasisStyle };
  return React.createElement('span', {
    ...attributes,
    style: { whiteSpace: 'pre-wrap', ...style },
  }, parts.map((part, index) => part.emphasized
    ? React.createElement('span', { key: index, className: emphasisClassName, style: prefixStyle }, part.text)
    : part.text));
}

module.exports = { BionicReaderProvider, BionicText };
