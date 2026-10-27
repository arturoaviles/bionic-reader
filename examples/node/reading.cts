import bionic = require('@arturoaviles/bionic-reader');

bionic.createReader().then((reader: bionic.Reader) => {
  console.log(reader.render('Hello world', { format: 'html' }));
  console.log(reader.segments('Hello world'));
});
