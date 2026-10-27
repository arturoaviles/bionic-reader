import type { CSSProperties, HTMLAttributes, ReactNode, ReactElement } from 'react';
import type { Options, Reader } from './index.js';

export interface BionicReaderProviderProps { reader: Reader; children?: ReactNode }
export interface BionicTextProps extends Omit<HTMLAttributes<HTMLSpanElement>, 'children' | 'dangerouslySetInnerHTML'> {
  text: string;
  options?: Omit<Options, 'input_format' | 'controls' | 'emphasis' | 'skip_code'>;
  emphasisClassName?: string;
  emphasisStyle?: CSSProperties;
}
export function BionicReaderProvider(props: BionicReaderProviderProps): ReactElement;
export function BionicText(props: BionicTextProps): ReactElement;
