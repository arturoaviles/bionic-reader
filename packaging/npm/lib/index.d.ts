export type OutputFormat = 'ansi' | 'plain' | 'markdown' | 'html';
export interface Options {
  enabled?: boolean;
  profile?: 'subtle' | 'balanced' | 'strong' | null;
  fraction?: number | null;
  every?: number | null;
  min_word_length?: number | null;
  length_rules?: Array<{ max_length: number; fraction: number }> | null;
  emphasis?: 'bold' | 'underline' | 'reverse' | null;
  input_format?: 'text' | 'markdown' | 'terminal' | null;
  controls?: 'safe' | 'preserve' | null;
  skip_code?: boolean | null;
}
export interface TextSegment { text: string; emphasized: boolean }
export interface BionicError extends Error { code: 'invalid_input' | 'initialization_failed' }
export interface Reader {
  render(text: string, options: Options & { format: OutputFormat }): string;
  segments(text: string, options?: Options): TextSegment[];
  parseConfig(contents: string): Options;
}
export function createReader(): Promise<Reader>;
