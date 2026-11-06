import test from 'node:test';
import assert from 'node:assert/strict';
import { Playback } from '../ui/playback/engine.ts';
import { clockFor, position, fadeSeconds, shuffle } from '../ui/playback/timeline.ts';

const track = (path, size = 100) => ({ path, name: path, size, modified: 1 });
const station = (key, tracks, mode = 'tracks', crossfade = 0) => ({
  key,
  station: key,
  tracks,
  mode,
  crossfade,
});
const flush = async () => {
  for (let i = 0; i < 20; i++) await Promise.resolve();
};

test('an old buffered pause completing after retune cannot pause the new station', async () => {
  const context = new ContextFake();
  const { player } = setup({ context, lengths: { a: 10, b: 10 } });
  try {
    await player.tune(station('old', [track('a'), track('b')], 'longset'));
    let finishPause;
    context.suspend = () =>
      new Promise((resolve) => {
        finishPause = resolve;
      });
    const paused = player.pause();
    await player.tune(station('new', [track('a')], 'tracks'));
    finishPause();
    await paused;
    assert.equal(player.state.key, 'new');
    assert.equal(player.state.playing, true);
    assert.equal(player.current.paused, false);
  } finally {
    player.dispose();
  }
});
class AudioFake extends EventTarget {
  duration = NaN;
  currentTime = 0;
  readyState = 0;
  paused = true;
  volume = 1;
  loop = false;
  ended = false;
  plays = 0;
  _src = '';
  constructor(lengths) {
    super();
    this.lengths = lengths;
  }
  get src() {
    return this._src;
  }
  set src(value) {
    this._src = value;
    this.duration = this.lengths[value] ?? NaN;
    this.readyState = Number.isFinite(this.duration) ? 1 : 0;
    if (!this.readyState) queueMicrotask(() => this.dispatchEvent(new Event('error')));
  }
  async play() {
    this.plays++;
    this.paused = false;
  }
  pause() {
    this.paused = true;
  }
  removeAttribute() {
    this._src = '';
    this.duration = NaN;
    this.readyState = 0;
  }
  load() {}
}
class ContextFake {
  currentTime = 0;
  sources = [];
  state = 'suspended';
  createGain() {
    return { gain: { value: 1 }, connect() {} };
  }
  createBufferSource() {
    const source = {
      connect() {},
      start(when, offset) {
        this.when = when;
        this.offset = offset;
      },
      stop() {},
      onended: null,
    };
    this.sources.push(source);
    return source;
  }
  async decodeAudioData() {
    return { duration: 10, length: 480000, numberOfChannels: 1 };
  }
  async resume() {
    this.state = 'running';
  }
  async suspend() {
    this.state = 'suspended';
  }
  async close() {
    this.state = 'closed';
  }
}
function setup({ prepare, lengths = { a: 20, b: 20, c: 20 }, random = () => 0, context } = {}) {
  const audio = [];
  let now = 0;
  const player = new Playback({
    prepare: prepare ?? (async () => [track('a'), track('b')]),
    url: (p) => p,
    onState() {},
    now: () => now,
    random,
    audio: () => {
      const a = new AudioFake(lengths);
      audio.push(a);
      return a;
    },
    context: () => context ?? new ContextFake(),
    fetchBytes: async () => new ArrayBuffer(8),
  });
  return { player, audio, setNow: (n) => (now = n) };
}

test('Electron timeline fixtures: random start, wrap and background wall-clock advancement', () => {
  const clock = clockFor([track('a'), track('b')], [20, 40], 5, 1000, () => 0.5);
  assert.deepEqual(clock.steps, [15, 35]);
  assert.equal(clock.offset, 25);
  assert.deepEqual(position(clock, 1000), { index: 1, seconds: 10 });
  assert.deepEqual(position(clock, 36000), { index: 0, seconds: 10 });
  assert.deepEqual(position(clock, 86000), { index: 0, seconds: 10 });
});
test('legacy fade units, broadcast defaults, half-track clamp, immutable shuffle', () => {
  assert.equal(fadeSeconds({ mode: 'tracks' }, { crossfade: 5 }), 5);
  assert.equal(fadeSeconds({ mode: 'tracks', crossfade: 5000 }, {}), 5);
  assert.equal(fadeSeconds({ mode: 'longset' }, { crossfade: 5000 }), 0);
  assert.equal(clockFor([track('a'), track('b')], [2, 20], 5, 0, () => 0).fade, 1);
  const input = [1, 2, 3];
  assert.deepEqual(
    shuffle(input, () => 0),
    [2, 3, 1],
  );
  assert.deepEqual(input, [1, 2, 3]);
});
test('retuning retains session order and advances the silent station clock', async () => {
  const { player, setNow } = setup();
  try {
    const s = station('one', [track('a'), track('b')]);
    await player.tune(s);
    const title = player.state.title;
    await player.tune(station('two', [track('a')]));
    setNow(10000);
    await player.tune(s);
    assert.equal(player.state.title, title);
    assert.equal(player.state.seconds, 10);
    await player.pause();
    assert.equal(player.state.playing, false);
    await player.play();
    assert.equal(player.state.playing, true);
  } finally {
    player.dispose();
  }
});
test('rapid tuning invalidates pending work and cannot revive the old station', async () => {
  let resolve;
  const first = new Promise((r) => (resolve = r));
  const { player } = setup({ prepare: async (key) => (key === 'old' ? first : [track('b')]) });
  try {
    const old = player.tune(station('old', [track('a')]));
    await player.tune(station('new', [track('b')]));
    resolve([track('a')]);
    await old;
    assert.equal(player.state.key, 'new');
    assert.equal(player.state.title, 'b');
    assert.equal(player.state.playing, true);
  } finally {
    player.dispose();
  }
});
test('pause during tuning prevents automatic playback on completion', async () => {
  let resolve;
  const pending = new Promise((r) => (resolve = r));
  const { player, audio } = setup({ prepare: () => pending });
  try {
    const tune = player.tune(station('one', [track('a')]));
    await player.pause();
    resolve([track('a')]);
    await tune;
    assert.equal(player.state.playing, false);
    assert.equal(audio[0].plays, 0);
    await player.play();
    assert.equal(player.state.playing, true);
  } finally {
    player.dispose();
  }
});
test('missing/unsupported tracks are skipped; all-unplayable stations stop visibly', async () => {
  const { player } = setup({
    prepare: async (key) => (key === 'bad' ? [track('missing')] : [track('missing'), track('a')]),
  });
  try {
    await player.tune(station('mixed', [track('missing'), track('a')]));
    assert.equal(player.state.title, 'a');
    assert.match(player.state.error, /skipped/);
    await player.tune(station('bad', [track('missing')]));
    assert.equal(player.state.playing, false);
    assert.match(player.state.error, /No playable tracks/);
  } finally {
    player.dispose();
  }
});
test('equal-power crossfade freezes on pause, honors mute, and swaps audio roles', async () => {
  const { player } = setup();
  try {
    await player.tune(station('fade', [track('a'), track('b')], 'tracks', 5000));
    player.current.currentTime = 15;
    player.tick();
    await flush();
    assert.ok(player.fading);
    const incoming = player.spare;
    incoming.currentTime = 2.5;
    player.tick();
    assert.ok(Math.abs(player.current.volume - 0.8 * Math.SQRT1_2) < 0.001);
    await player.pause();
    player.tick();
    assert.equal(incoming.paused, true);
    player.mute();
    player.tick();
    assert.equal(incoming.volume, 0);
    await player.play();
    incoming.currentTime = 5;
    player.tick();
    assert.equal(player.current, incoming);
    assert.equal(player.fading, null);
    assert.equal(player.state.playing, true);
  } finally {
    player.dispose();
  }
});
test('broadcasts schedule exact adjacent boundaries, preserve pause and release contexts', async () => {
  const context = new ContextFake();
  const { player } = setup({ context, lengths: { a: 10, b: 10 } });
  try {
    await player.tune(station('radio', [track('a'), track('b')], 'longset'));
    await flush();
    assert.equal(player.state.engine, 'buffered');
    assert.ok(context.sources.length >= 2);
    assert.equal(context.sources[1].when, context.sources[0].when + 10);
    await player.pause();
    assert.equal(context.state, 'suspended');
    await player.play();
    assert.equal(context.state, 'running');
    context.sources[0].onended();
    assert.equal(player.state.title, 'b');
    player.stop();
    assert.equal(context.state, 'closed');
  } finally {
    player.dispose();
  }
});
test('long broadcasts stream, and decode failures fall back to streaming', async () => {
  const context = new ContextFake();
  context.decodeAudioData = async () => {
    throw new Error('codec');
  };
  const { player } = setup({ context, lengths: { a: 700, b: 700 } });
  try {
    await player.tune(station('long', [track('a'), track('b')], 'longset'));
    assert.equal(player.state.engine, 'stream');
    player.durations.clear();
    player.options.audio = () => new AudioFake({ a: 10, b: 10 });
    await player.tune(station('fallback', [track('a'), track('b')], 'longset'));
    assert.equal(player.state.engine, 'stream');
    assert.equal(player.state.playing, true);
    assert.equal(context.state, 'closed');
  } finally {
    player.dispose();
  }
});
test('oversized decoded buffers fall back without retaining a context', async () => {
  const context = new ContextFake();
  context.decodeAudioData = async () => ({ duration: 10, length: 20_000_000, numberOfChannels: 2 });
  const { player } = setup({ context, lengths: { a: 10, b: 10 } });
  try {
    await player.tune(station('budget', [track('a'), track('b')], 'longset'));
    assert.equal(player.state.engine, 'stream');
    assert.equal(context.state, 'closed');
  } finally {
    player.dispose();
  }
});
