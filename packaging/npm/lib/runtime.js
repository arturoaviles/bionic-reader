import initialize, { Engine, parse_config, version } from './wasm/engine.js';
import { VERSION } from './version.js';

let ready;
let engine;

export function failure(message, code = 'invalid_input') {
  return Object.assign(new Error(String(message)), { name: 'BionicError', code });
}

function unicode(text) {
  if (typeof text !== 'string') throw failure('text must be a Unicode string');
  for (let index = 0; index < text.length; index += 1) {
    const unit = text.charCodeAt(index);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = text.charCodeAt(index + 1);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw failure('text contains malformed Unicode');
      index += 1;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) {
      throw failure('text contains malformed Unicode');
    }
  }
  return text;
}

function optionsJSON(options) {
  if (!options || typeof options !== 'object' || Array.isArray(options)) {
    throw failure('options must be an object');
  }
  return JSON.stringify(options, (_key, value) => {
    if (typeof value === 'function' || typeof value === 'symbol' || typeof value === 'bigint') {
      throw failure('options must contain serializable values');
    }
    if (typeof value === 'number' && !Number.isFinite(value)) {
      throw failure('numeric options must be finite');
    }
    return value;
  });
}

function call(operation) {
  try {
    return operation();
  } catch (error) {
    if (error?.name === 'BionicError') throw error;
    throw failure(error?.message ?? error);
  }
}

export async function createInitializedReader(load) {
  if (!ready) {
    ready = Promise.resolve().then(load).then((bytes) => initialize({ module_or_path: bytes }))
      .then(() => {
        if (version() !== VERSION) throw new Error('The bundled engine version does not match the package');
        engine = new Engine();
      })
      .catch((error) => {
        ready = undefined;
        throw failure(error?.message ?? error, 'initialization_failed');
      });
  }
  await ready;
  // The engine is immutable between calls; only the compiled runtime/data is shared.
  return Object.freeze({
    render(text, options) {
      return call(() => {
        const { format, ...settings } = options ?? {};
        if (typeof format !== 'string') throw failure('render requires an explicit output format');
        return engine.render(unicode(text), format, optionsJSON(settings));
      });
    },
    segments(text, options = {}) {
      return call(() => JSON.parse(engine.segments(unicode(text), optionsJSON(options))));
    },
    parseConfig(contents) {
      return call(() => JSON.parse(parse_config(unicode(contents))));
    },
  });
}
