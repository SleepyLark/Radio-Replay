const path = require('node:path');
const { utilityProcess } = require('electron');

class MetadataService {
  constructor() {
    this.pending = new Map();
    this.nextId = 0;
  }

  start() {
    if (this.child) return this.child;
    const child = utilityProcess.fork(path.join(__dirname, 'metadata-worker.cjs'));
    this.child = child;
    child.on('message', ({ id, value, error }) => {
      const request = this.pending.get(id);
      if (!request) return;
      clearTimeout(request.timer);
      this.pending.delete(id);
      if (error) request.reject(new Error(error));
      else request.resolve(value);
    });
    child.on('exit', () => {
      if (this.child !== child) return;
      this.child = null;
      for (const request of this.pending.values()) {
        clearTimeout(request.timer);
        request.reject(new Error('Metadata process exited'));
      }
      this.pending.clear();
    });
    return child;
  }

  request(kind, file) {
    const child = this.start();
    const id = ++this.nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error('Metadata timed out'));
        child.kill();
      }, 5000);
      this.pending.set(id, { resolve, reject, timer });
      child.postMessage({ id, kind, file });
    });
  }

  close() {
    this.child?.kill();
  }
}

module.exports = { MetadataService };
