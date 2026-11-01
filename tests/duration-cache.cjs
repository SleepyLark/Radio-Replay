const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { DurationCache } = require('../src/duration-cache.cjs');

async function fixture() {
  const output = path.join(__dirname, '..', 'out');
  await fs.mkdir(output, { recursive: true });
  const directory = await fs.mkdtemp(path.join(output, 'cache-test-'));
  const audio = path.join(directory, 'audio.mp3');
  await fs.writeFile(audio, 'audio fixture');
  return { directory, audio, file: path.join(directory, 'duration-cache.sqlite') };
}

test('durations survive restarts and changes/deletion invalidate them', async () => {
  const { audio, file } = await fixture();
  let cache = new DurationCache(file);
  assert.equal((await cache.lookup(audio)).duration, 0);
  assert.equal(await cache.remember(audio, await cache.lookup(audio), 123), true);
  cache.close();
  cache = new DurationCache(file);
  try {
    assert.equal((await cache.lookup(audio)).duration, 123);
    const old = await cache.lookup(audio);
    await fs.appendFile(audio, 'changed');
    assert.equal((await cache.lookup(audio)).duration, 0);
    assert.equal(await cache.remember(audio, old, 456), false);
    await cache.remember(audio, await cache.lookup(audio), 456);
    assert.equal((await cache.lookup(audio)).duration, 456);
    await fs.unlink(audio);
    assert.deepEqual(await cache.lookup(audio), { available: false, duration: 0 });
  } finally {
    cache.close();
  }
});

test('timestamp changes alone invalidate a duration', async () => {
  const { audio, file } = await fixture();
  const cache = new DurationCache(file);
  try {
    await cache.remember(audio, await cache.lookup(audio), 10);
    await fs.utimes(audio, new Date(), new Date(Date.now() + 5000));
    assert.equal((await cache.lookup(audio)).duration, 0);
  } finally {
    cache.close();
  }
});

test('cleanup removes unreferenced audio without evicting referenced entries', async () => {
  const { directory, file } = await fixture();
  const cache = new DurationCache(file);
  const files = [];
  try {
    for (let i = 0; i < 10; i++) {
      const audio = path.join(directory, i + '.mp3');
      files.push(audio);
      await fs.writeFile(audio, 'fixture');
      await cache.remember(audio, await cache.lookup(audio), i + 1);
    }
    cache.flush();
    let hits = 0;
    for (const audio of files) if ((await cache.lookup(audio)).duration) hits++;
    assert.equal(hits, 10);
    await cache.retain([files[9]]);
    assert.equal((await cache.lookup(files[9])).duration, 10);
    for (const audio of files.slice(0, 9)) assert.equal((await cache.lookup(audio)).duration, 0);
  } finally {
    cache.close();
  }
});

test('libraries beyond the former size and entry caps survive writes and reopening', async () => {
  const { audio, file } = await fixture();
  let cache = new DurationCache(file);
  cache.open();
  // Seed a large library efficiently; then exercise the regular write/flush/reopen path.
  cache.db.exec(`WITH RECURSIVE entries(n) AS (
    SELECT 1 UNION ALL SELECT n+1 FROM entries WHERE n<110000
  ) INSERT INTO durations_v1 SELECT printf('%064x', n), 8000000, 1700000000000.123,
    1700000000000.456, 240.123, ${Date.now()} FROM entries`);
  await cache.remember(audio, await cache.lookup(audio), 55);
  cache.close();
  assert.ok((await fs.stat(file)).size > 16 * 1048576);
  cache = new DurationCache(file);
  try {
    assert.equal((await cache.lookup(audio)).duration, 55);
    assert.equal(
      cache.db.prepare('SELECT COUNT(*) AS count FROM durations_v1').get().count,
      110001,
    );
  } finally {
    cache.close();
  }
});

test('corrupt caches fall back safely and rebuild on the next open', async () => {
  const { audio, file } = await fixture();
  await fs.writeFile(file, 'invalid sqlite');
  let cache = new DurationCache(file);
  const lookup = await cache.lookup(audio);
  assert.equal(lookup.available, true);
  assert.equal(lookup.duration, 0);
  cache.close();
  cache = new DurationCache(file);
  try {
    await cache.remember(audio, await cache.lookup(audio), 77);
    cache.flush();
    assert.equal((await cache.lookup(audio)).duration, 77);
  } finally {
    cache.close();
  }
});

test('clearing removes pending and saved entries, reclaims space, and rejects stale probes', async () => {
  const { audio, file } = await fixture();
  let cache = new DurationCache(file);
  const stale = await cache.lookup(audio);
  await cache.remember(audio, stale, 123);
  cache.flush();
  await cache.remember(audio, stale, 456);
  const inFlight = cache.remember(audio, stale, 999);
  const maintenance = cache.retain(Array.from({ length: 300 }, () => audio));
  cache.clear();
  assert.equal(await inFlight, false);
  await maintenance;
  assert.equal((await cache.lookup(audio)).duration, 0);
  assert.equal(await cache.remember(audio, stale, 789), false);
  await cache.remember(audio, await cache.lookup(audio), 42);
  cache.close();
  cache = new DurationCache(file);
  try {
    assert.equal((await cache.lookup(audio)).duration, 42);
  } finally {
    cache.close();
  }
});
