import { BionicReaderProvider, BionicText } from '@arturoaviles/bionic-reader/react';
import type { Reader } from '@arturoaviles/bionic-reader';

export function Reading({ reader }: { reader: Reader }) {
  return <BionicReaderProvider reader={reader}>
    <BionicText text="Hello world" options={{ profile: 'balanced' }} emphasisStyle={{ fontWeight: 600 }} />
  </BionicReaderProvider>;
}
