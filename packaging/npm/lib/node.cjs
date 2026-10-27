'use strict';

exports.createReader = async function createReader() {
  try {
    const module = await import('./node.js');
    return await module.createReader();
  } catch (error) {
    if (error?.name === 'BionicError') throw error;
    throw Object.assign(new Error(String(error?.message ?? error)), {
      name: 'BionicError', code: 'initialization_failed',
    });
  }
};
