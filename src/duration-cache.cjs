const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const { createHash } = require('node:crypto');

const keyFor = (file) =>
  createHash('sha256')
    .update(
      process.platform === 'win32' ? path.normalize(file).toLowerCase() : path.normalize(file),
    )
    .digest('hex');
const sameFile = (a, b) =>
  a && b && a.size === b.size && a.mtimeMs === b.mtimeMs && a.ctimeMs === b.ctimeMs;
const validDuration = (value) => Number.isFinite(value) && value >= 0.5 && value <= 48 * 3600;

// Derived, disposable data. Audio and station definitions never live in this database.
class DurationCache {
  constructor(file) {
    this.file = file;
    this.pending = new Map();
    this.disabled = false;
    this.referenceVersion = 0;
    this.generation = 0;
  }

  open() {
    if (this.db || this.disabled) return this.db;
    try {
      const { DatabaseSync } = require('node:sqlite');
      fs.mkdirSync(path.dirname(this.file), { recursive: true });
      this.db = new DatabaseSync(this.file);
      this.db.exec(`PRAGMA page_size=4096;
        PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA journal_mode=DELETE;
        PRAGMA temp_store=FILE;
        CREATE TABLE IF NOT EXISTS durations_v1 (
          key TEXT PRIMARY KEY, size REAL, mtimeMs REAL, ctimeMs REAL,
          duration REAL, used REAL
        ); CREATE INDEX IF NOT EXISTS duration_used ON durations_v1(used);`);
      this.get = this.db.prepare('SELECT * FROM durations_v1 WHERE key=?');
      this.put = this.db.prepare('INSERT OR REPLACE INTO durations_v1 VALUES (?, ?, ?, ?, ?, ?)');
      this.remove = this.db.prepare('DELETE FROM durations_v1 WHERE key=?');
      this.db.prepare('DELETE FROM durations_v1 WHERE used < ?').run(Date.now() - 90 * 86400000);
      return this.db;
    } catch (error) {
      // Corrupt/unwritable caches must not prevent playback. Remove only our derived file.
      try {
        this.db?.close();
      } catch {}
      this.db = null;
      if (
        ['SQLITE_CORRUPT', 'SQLITE_NOTADB'].includes(error.code) ||
        (error.code === 'ERR_SQLITE_ERROR' &&
          /file is not a database|database disk image is malformed/i.test(error.message))
      ) {
        try {
          fs.unlinkSync(this.file);
        } catch {}
      }
      this.disabled = true;
      return null;
    }
  }

  async signature(file) {
    try {
      const stat = await fsp.stat(file);
      return stat.isFile()
        ? { size: stat.size, mtimeMs: stat.mtimeMs, ctimeMs: stat.ctimeMs }
        : null;
    } catch {
      return null;
    }
  }

  async lookup(file) {
    const cacheGeneration = this.generation;
    const signature = await this.signature(file);
    if (!signature) {
      const key = keyFor(file);
      this.pending.delete(key);
      try {
        if (this.open()) this.remove.run(key);
      } catch {}
      return { available: false, duration: 0 };
    }
    let duration = 0;
    const key = keyFor(file);
    try {
      if (this.open()) {
        const entry = this.pending.get(key) || this.get.get(key);
        if (sameFile(entry, signature) && validDuration(entry.duration)) {
          duration = entry.duration;
          // A daily access stamp is sufficient for stale-entry cleanup; warm tuning is read-only.
          if (Date.now() - entry.used > 86400000) this.queue(key, { ...signature, duration });
        }
      }
    } catch {
      /* A cache miss can always fall back to an audio metadata probe. */
    }
    return { available: true, ...signature, duration, cacheGeneration };
  }

  async remember(file, signature, duration) {
    const current = await this.signature(file);
    if (
      signature?.cacheGeneration !== this.generation ||
      !validDuration(duration) ||
      !sameFile(signature, current)
    )
      return false;
    if (this.open()) this.queue(keyFor(file), { ...signature, duration });
    return true;
  }

  queue(key, entry) {
    this.pending.set(key, { ...entry, used: Date.now() });
    if (this.pending.size >= 256) this.flush();
    else if (!this.timer) this.timer = setTimeout(() => this.flush(), 2000);
  }

  flush() {
    clearTimeout(this.timer);
    this.timer = null;
    if (!this.db || !this.pending.size) return;
    try {
      this.db.exec('BEGIN');
      for (const [key, entry] of this.pending)
        this.put.run(key, entry.size, entry.mtimeMs, entry.ctimeMs, entry.duration, entry.used);
      this.db.exec('COMMIT');
    } catch {
      try {
        this.db.exec('ROLLBACK');
        // Keep existing entries on write failure; missed updates can be retried on tune.
      } catch {}
    }
    this.pending.clear();
  }

  async retain(files) {
    if (!this.db) return;
    const version = ++this.referenceVersion;
    try {
      this.db.exec(
        'CREATE TEMP TABLE IF NOT EXISTS referenced_duration_keys (key TEXT PRIMARY KEY); DELETE FROM referenced_duration_keys',
      );
      const insert = this.db.prepare('INSERT OR IGNORE INTO referenced_duration_keys VALUES (?)');
      let count = 0;
      for (const file of files) {
        if (version !== this.referenceVersion || !this.db) return;
        insert.run(keyFor(file));
        if (++count % 256 === 0) await new Promise((resolve) => setImmediate(resolve));
      }
      if (version !== this.referenceVersion || !this.db) return;
      this.flush();
      this.db.exec(
        'DELETE FROM durations_v1 WHERE key NOT IN (SELECT key FROM referenced_duration_keys); DROP TABLE referenced_duration_keys',
      );
    } catch {
      /* Cleanup can be repeated after the next library edit. */
    }
  }

  clear() {
    ++this.generation;
    ++this.referenceVersion;
    clearTimeout(this.timer);
    this.timer = null;
    this.pending.clear();
    this.disabled = false;
    if (!this.open()) throw new Error('Duration cache could not be cleared');
    this.db.exec('DELETE FROM durations_v1; DROP TABLE IF EXISTS referenced_duration_keys; VACUUM');
  }

  close() {
    ++this.referenceVersion;
    this.flush();
    try {
      this.db?.close();
    } catch {}
    this.db = null;
  }
}

module.exports = { DurationCache };
