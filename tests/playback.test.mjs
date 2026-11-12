import test from 'node:test';
import assert from 'node:assert/strict';
import { Playback } from '../ui/playback/engine.ts';
import { clockFor, position, fadeSeconds, shuffle } from '../ui/playback/timeline.ts';

test('tuning static is cleaned up when playback is ready and honors mute', async () => {
  const { player, audio } = setup();
  player.options.tuningSource = async () => 'a';
  player.mute();
  try {
    await player.tune(station('tuning', [track('a'), track('b')]), {
      tuningMinMs: 20,
      tuningMaxMs: 20,
    });
    assert.equal(player.noise, null);
    assert.equal(player.state.playing, true);
    assert.ok(audio.some((a) => a.plays === 1 && a.paused && a.volume === 0));
  } finally {
    player.dispose();
  }
});
test('static loops past the minimum delay through preparation and stream startup', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  let finishPrepare, finishPlay;
  const { player } = setup({ prepare: () => new Promise((resolve) => (finishPrepare = resolve)) });
  player.options.tuningSource = async () => 'c';
  const originalPlay = player.current.play.bind(player.current);
  player.current.play = () =>
    new Promise(
      (resolve) =>
        (finishPlay = async () => {
          await originalPlay();
          resolve();
        }),
    );
  try {
    const tuning = player.tune(station('slow', [track('a')]), { tuningMinMs: 20, tuningMaxMs: 20 });
    await flush();
    const noise = player.noise;
    assert.ok(noise);
    assert.equal(noise.loop, true);
    t.mock.timers.tick(100);
    await flush();
    assert.equal(player.state.loading, true);
    assert.equal(noise.paused, false, 'Static stopped after minimum delay');
    player.setVolume(0.35);
    assert.equal(noise.volume, 0.35);
    finishPrepare([{ ...track('a'), duration: 20 }]);
    await flush();
    assert.ok(finishPlay, 'Stream did not reach startup');
    assert.equal(noise.paused, false, 'Static stopped before stream play resolved');
    await finishPlay();
    await tuning;
    assert.equal(player.state.playing, true);
    assert.equal(player.noise, null);
    assert.equal(noise.paused, true);
    assert.equal(noise.src, '');
  } finally {
    player.dispose();
  }
});
test('loading-only and zero-delay tuning still loop static until ready', async () => {
  for (const settings of [{ tuningTransition: false }, { tuningMinMs: 0, tuningMaxMs: 0 }]) {
    let finishPrepare;
    const { player } = setup({
      prepare: () => new Promise((resolve) => (finishPrepare = resolve)),
    });
    player.options.tuningSource = async () => 'c';
    try {
      const tuning = player.tune(station('loading', [track('a')]), settings);
      await flush();
      const noise = player.noise;
      assert.ok(noise, 'No loading static when delay is disabled');
      assert.equal(noise.loop, true);
      assert.equal(noise.paused, false);
      finishPrepare([{ ...track('a'), duration: 20 }]);
      await tuning;
      assert.equal(player.noise, null);
      assert.equal(noise.paused, true);
    } finally {
      player.dispose();
    }
  }
});
test('a stale tune finishing cannot stop the new station static', async () => {
  const pending = new Map();
  const { player } = setup({
    prepare: (key) => new Promise((resolve) => pending.set(key, resolve)),
  });
  player.options.tuningSource = async () => 'c';
  try {
    const old = player.tune(station('old', [track('a')]), { tuningTransition: false });
    await flush();
    const oldNoise = player.noise;
    const current = player.tune(station('current', [track('b')]), { tuningTransition: false });
    await flush();
    const newNoise = player.noise;
    assert.equal(oldNoise.paused, true);
    pending.get('old')([{ ...track('a'), duration: 20 }]);
    await old;
    assert.equal(player.noise, newNoise);
    assert.equal(newNoise.paused, false);
    pending.get('current')([{ ...track('b'), duration: 20 }]);
    await current;
    assert.equal(newNoise.paused, true);
  } finally {
    player.dispose();
  }
});
test('retuning cancels the delay and late tuning audio cannot restart', async () => {
  const { player } = setup();
  let resolve;
  player.options.tuningSource = () => new Promise((r) => (resolve = r));
  try {
    const tuning = player.tune(station('cancel', [track('a')]), {
      tuningMinMs: 1500,
      tuningMaxMs: 1500,
    });
    await flush();
    player.stop();
    resolve('a');
    await tuning;
    await flush();
    assert.equal(player.noise, null);
    assert.equal(player.state.key, '');
    assert.equal(player.state.playing, false);
  } finally {
    player.dispose();
  }
});

test('persisted durations bypass probes and new measurements are sent to the cache', async () => {
  const { player, audio } = setup({ prepare: async () => [{ ...track('a'), duration: 20 }] });
  const remembered = [];
  player.options.rememberDuration = async (...args) => remembered.push(args);
  try {
    await player.tune(station('cached', [track('a')]));
    assert.equal(audio.length, 2);
    assert.equal(remembered.length, 0);
    player.options.prepare = async () => [track('b')];
    await player.tune(station('fresh', [track('b')]));
    assert.equal(remembered[0][0], 'fresh');
    assert.equal(remembered[0][2], 20);
  } finally {
    player.dispose();
  }
});
test('duration writes finish before tune completes, and failures remain visible without blocking playback', async () => {
  const { player } = setup({ prepare: async () => [track('a')] });
  let finishWrite;
  player.options.rememberDuration = () => new Promise((resolve) => (finishWrite = resolve));
  try {
    const tuning = player.tune(station('persist', [track('a')]));
    await flush();
    assert.ok(finishWrite);
    assert.equal(player.state.loading, true);
    finishWrite();
    await tuning;
    assert.equal(player.state.playing, true);
    player.invalidate();
    player.options.rememberDuration = async () => {
      throw new Error('Storage unavailable');
    };
    await player.tune(station('failed-cache', [track('a')]));
    assert.equal(player.state.playing, true);
    assert.match(player.state.error, /Durations could not be saved/);
  } finally {
    player.dispose();
  }
});
test('each group retains its own station clock', async () => {
  const { player, setNow } = setup();
  const s = station('same', [track('a'), track('b')]);
  try {
    await player.tune(s, {}, 'first');
    setNow(5000);
    await player.tune(s, {}, 'second');
    setNow(10000);
    await player.tune(s, {}, 'first');
    assert.equal(player.state.seconds, 10);
    await player.tune(s, {}, 'second');
    assert.equal(player.state.seconds, 5);
  } finally {
    player.dispose();
  }
});

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
  srcAssignments = 0;
  _src = '';
  constructor(lengths) {
    super();
    this.lengths = lengths;
  }
  get src() {
    return this._src;
  }
  set src(value) {
    this.srcAssignments++;
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
  sampleRate = 48000;
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
    assert.equal(player.state.notice.problems[0].path, 'missing');
    assert.match(player.state.notice.problems[0].reason, /unsupported/);
    assert.equal(player.state.notice.severity, 'warning');
    await player.tune(station('bad', [track('missing')]));
    assert.equal(player.state.playing, false);
    assert.match(player.state.error, /No playable tracks/);
    assert.equal(player.state.notice.severity, 'error');
    assert.equal(player.state.notice.problems[0].path, 'missing');
  } finally {
    player.dispose();
  }
});
test('native missing-file reports and codec failures retain paths, reasons and stable notice identity', async () => {
  const { player } = setup({
    prepare: async () => ({
      tracks: [track('missing-codec'), track('a')],
      problems: [
        { path: 'missing-file.mp3', name: 'Missing file', reason: 'File not found.', kind: 'file' },
      ],
    }),
  });
  try {
    const stationConfig = station('reported', [
      track('missing-file.mp3'),
      track('missing-codec'),
      track('a'),
    ]);
    await player.tune(stationConfig);
    const notice = player.state.notice;
    assert.equal(player.state.playing, true);
    assert.equal(notice.problems.length, 2);
    assert.equal(
      notice.problems.find((p) => p.path === 'missing-file.mp3').reason,
      'File not found.',
    );
    assert.match(notice.problems.find((p) => p.path === 'missing-codec').reason, /unsupported/);
    player.tick();
    player.setVolume(0.4);
    await player.pause();
    await player.play();
    assert.equal(player.state.notice, notice, 'Progress and controls regenerated the same notice');
    await player.tune(stationConfig);
    assert.ok(player.state.notice.id > notice.id, 'A fresh tune did not create a new report');
  } finally {
    player.dispose();
  }
});
test('runtime playback failures add the failed recording to the report and preserve other playback', async () => {
  const { player } = setup();
  try {
    await player.tune(station('runtime', [track('a'), track('b')], 'longset', 100));
    const path = player.state.path;
    player.current.dispatchEvent(new Event('error'));
    await flush();
    assert.equal(player.state.playing, true);
    assert.ok(player.state.notice.problems.some((problem) => problem.path === path));
    assert.match(player.state.notice.problems[0].reason, /read|decoded/);
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

test('very large station clocks do not exceed the JavaScript argument limit', () => {
  const count = 200000;
  const clock = clockFor(Array(count).fill(track('a')), Array(count).fill(180), 5, 0, () => 0);
  assert.equal(clock.fade, 5);
  assert.equal(clock.total, count * 175);
});

test('zero-fade streaming uses the preloaded track without reassigning its source', async () => {
  const {player} = setup();
  try {
    await player.tune(station('stream', [track('a'), track('b')]));
    const next = player.spare;
    const path = next.src;
    const assignments = next.srcAssignments;
    player.current.dispatchEvent(new Event('ended'));
    await flush();
    assert.equal(player.state.path, path);
    assert.equal(next.paused, false);
    assert.equal(next.srcAssignments, assignments, 'Preloaded media was loaded again');
    assert.equal(player.state.playing, true);
  } finally {player.dispose();}
});

test('mute remembers the volume, while changing volume unmutes at the new level',async()=>{
  const {player}=setup();
  try {
    await player.tune(station('volume',[track('a'),track('b')]));
    player.mute();
    assert.equal(player.state.volume,.8);
    assert.equal(player.current.volume,0);
    player.mute();
    assert.equal(player.current.volume,.8);
    player.mute();
    player.setVolume(.35);
    assert.equal(player.state.muted,false);
    assert.equal(player.state.volume,.35);
    assert.equal(player.current.volume,.35);
    player.mute();
    player.mute();
    assert.equal(player.current.volume,.35);
    player.mute();
    player.setVolume(0);
    assert.equal(player.state.muted,false);
    assert.equal(player.current.volume,0);
  } finally {player.dispose();}
});

test('over-budget broadcast estimates stream without fetching or decoding buffers', async () => {
  const context = new ContextFake();
  let fetched = 0, decoded = 0;
  const {player} = setup({context, lengths:{a:500,b:500}});
  player.options.fetchBytes = async () => {fetched++;return new ArrayBuffer(8);};
  context.decodeAudioData = async () => {decoded++;return {duration:500,length:24000000,numberOfChannels:2};};
  try {
    await player.tune(station('large', [track('a'),track('b')], 'longset'));
    assert.equal(player.state.engine, 'stream');
    assert.equal(player.state.playing, true);
    assert.equal(fetched, 0);
    assert.equal(decoded, 0);
    assert.equal(context.state, 'closed');
  } finally {player.dispose();}
});

test('an unready preload falls back to loading the next stream', async () => {
  const {player} = setup();
  try {
    await player.tune(station('unready', [track('a'),track('b')]));
    const nextPath=player.spare.src;
    player.spare.readyState=0;
    player.current.dispatchEvent(new Event('ended'));
    await flush();
    assert.equal(player.state.path,nextPath);
    assert.equal(player.state.playing,true);
    assert.equal(player.current.paused,false);
  } finally {player.dispose();}
});

test('buffer prefetch waits for capacity instead of decoding past the total budget', async () => {
  const context=new ContextFake();
  let fetched=0,decoded=0;
  const {player}=setup({context,lengths:{a:160,b:160,c:160},prepare:async()=>[track('a'),track('b'),track('c')]});
  player.options.fetchBytes=async()=>{fetched++;return new ArrayBuffer(8);};
  context.decodeAudioData=async()=>{decoded++;return {duration:160,length:7680000,numberOfChannels:2};};
  try {
    await player.tune(station('capacity',[track('a'),track('b'),track('c')],'longset'));
    await flush();
    assert.equal(player.state.engine,'buffered');
    assert.equal(fetched,2);
    assert.equal(decoded,2);
    context.sources[0].onended();
    await flush();
    assert.equal(decoded,3,'Prefetch did not resume after releasing the previous buffer');
  } finally {player.dispose();}
});
