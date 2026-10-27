import React from 'react';
import { createReader } from '@arturoaviles/bionic-reader';
import { BionicReaderProvider, BionicText } from '@arturoaviles/bionic-reader/react';

// Each runtime initializes its own reader while loading the module, before rendering.
const reader = await createReader();
const text = '<script> Hello\nworld café ภาษาไทย';

export default function Home() {
  const [hydrated, setHydrated] = React.useState(false);
  React.useEffect(() => setHydrated(true), []);
  return React.createElement(BionicReaderProvider, { reader },
    React.createElement('main', { style: { maxWidth: '42rem', margin: '3rem auto', font: '1.2rem/1.7 system-ui' } },
      React.createElement('h1', null, 'Bionic Reader with Next.js'),
      React.createElement(BionicText, { id: 'reading', text }),
      React.createElement('p', { id: 'hydration', 'data-ready': hydrated }, hydrated ? 'Hydrated' : 'Server rendered')));
}

export async function getServerSideProps() {
  return { props: {} };
}
