// Native packages exclude the comparison engine and its duplicate sound asset.
export const createWebPlayback:typeof import('./web').createWebPlayback=()=>{
  throw new Error('This package requires the native audio backend');
};
