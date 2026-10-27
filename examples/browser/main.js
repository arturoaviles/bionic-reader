import { createReader } from '@arturoaviles/bionic-reader';

const status = document.querySelector('#status');
const button = document.querySelector('#render');
const output = document.querySelector('#output');

try {
  const reader = await createReader();
  // Available in the developer console for trying the installed package's public API.
  window.bionicReader = reader;
  function render() {
    try {
      const text = document.querySelector('#text').value;
      const options = JSON.parse(document.querySelector('#options').value);
      const format = document.querySelector('#format').value;
      output.textContent = reader.render(text, { ...options, format });
      output.removeAttribute('data-error');
      const segments = reader.segments(text, options);
      document.querySelector('#segments').textContent = JSON.stringify(segments);
      const fragment = document.createDocumentFragment();
      for (const part of segments) {
        const node = document.createElement('span');
        node.textContent = part.text;
        if (part.emphasized) node.className = 'emphasized';
        fragment.append(node);
      }
      document.querySelector('#preview').replaceChildren(fragment);
    } catch (error) {
      output.textContent = error.message;
      output.dataset.error = error.code ?? 'invalid_input';
    }
  }
  button.addEventListener('click', render);
  button.disabled = false;
  status.textContent = 'Ready. Text is processed locally.';
  render();
} catch (error) {
  status.textContent = error.message;
}
