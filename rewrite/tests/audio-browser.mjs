import { Playback } from '../ui/playback/engine.ts';
const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};
function wav(seconds) {
  const samples = Math.round(seconds * 48000);
  const bytes = new ArrayBuffer(44 + samples * 2);
  const view = new DataView(bytes);
  const text = (at, value) => {
    for (let i = 0; i < value.length; i++) view.setUint8(at + i, value.charCodeAt(i));
  };
  text(0, 'RIFF');
  view.setUint32(4, bytes.byteLength - 8, true);
  text(8, 'WAVEfmt ');
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, 48000, true);
  view.setUint32(28, 96000, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  text(36, 'data');
  view.setUint32(40, samples * 2, true);
  return new Blob([bytes], { type: 'audio/wav' });
}
window.audioSmoke = async () => {
  const urls = [URL.createObjectURL(wav(1.2)), URL.createObjectURL(wav(1.2))];
  const tracks = urls.map((path, i) => ({ path, name: `Track ${i}`, size: 115244, modified: 1 }));
  const player = new Playback({
    prepare: async () => tracks,
    url: (path) => path,
    onState() {},
    random: () => 0,
  });
  player.setVolume(0);
  try {
    await player.tune({ key: 'broadcast', station: 'Broadcast', tracks, mode: 'longset' });
    assert(
      player.state.engine === 'buffered',
      'Broadcast did not use real AudioContext scheduling',
    );
    const entries = player.buffered.entries;
    assert(
      Math.abs(entries[1].when - entries[0].end) < 0.000001,
      'Scheduled buffers are not adjacent',
    );
    await wait(200);
    await player.pause();
    const paused = player.buffered.context.currentTime;
    await wait(150);
    assert(
      Math.abs(player.buffered.context.currentTime - paused) < 0.02,
      'Paused context kept advancing',
    );
    await player.play();
    await wait(1350);
    assert(player.state.title === 'Track 1', 'Gapless broadcast did not advance');
    await player.tune({ key: 'music', station: 'Music', tracks, mode: 'tracks', crossfade: 500 });
    assert(player.state.engine === 'stream', 'Music did not use streaming elements');
    const first = player.current;
    await wait(1450);
    assert(player.current !== first, 'Real audio crossfade did not swap streams');
    await player.pause();
    assert(player.current.paused && player.spare.paused, 'Pause did not stop both streams');
    return 'Real audio smoke passed: decode, scheduled gapless handoff, pause/resume, streaming crossfade, cleanup.';
  } finally {
    player.dispose();
    urls.forEach((url) => URL.revokeObjectURL(url));
  }
};
