import { createReader, type Options, type Reader } from '@arturoaviles/bionic-reader';

const reader: Reader = await createReader();
const options: Options = { profile: 'balanced', every: 2 };
console.log(reader.render('Hello world', { ...options, format: 'html' }));
console.log(reader.segments('Hello world', options));
