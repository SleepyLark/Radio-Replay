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
  const noiseUrl = URL.createObjectURL(wav(0.08));
  try {
    let finishPrepare;
    player.options.prepare = () => new Promise((resolve) => (finishPrepare = resolve));
    player.options.tuningSource = async () => noiseUrl;
    const tuning = player.tune(
      { key: 'slow-tuning', station: 'Slow tuning', tracks: [tracks[0]], mode: 'tracks' },
      { tuningMinMs: 20, tuningMaxMs: 20 },
    );
    for (
      let i = 0;
      i < 100 && (!player.noise || player.noise.readyState < 2 || player.noise.paused);
      i++
    )
      await wait(20);
    const noise = player.noise;
    assert(noise && !noise.paused, 'Real tuning static did not start');
    await wait(350);
    assert(
      noise.duration < 0.2 && noise.loop && !noise.ended && !noise.paused && player.noise === noise,
      'Static did not loop past its recording and minimum transition',
    );
    assert(player.state.loading, 'Slow fixture was not still tuning');
    finishPrepare([{ ...tracks[0], duration: 1.2 }]);
    await tuning;
    assert(
      player.state.playing && player.noise === null && noise.paused,
      'Static was not released when station started',
    );
    player.options.prepare = async () => tracks;
    player.options.tuningSource = undefined;
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
    await player.tune({key:'no-fade',station:'No fade',tracks,mode:'tracks',crossfade:0});
    const preloaded=player.spare;
    for(let i=0;i<100&&preloaded.readyState<2;i++)await wait(10);
    assert(preloaded.readyState>=2,'Next stream did not preload');
    let reloaded=false;
    preloaded.addEventListener('emptied',()=>{reloaded=true;});
    for(let i=0;i<200&&player.current!==preloaded;i++)await wait(10);
    assert(player.current===preloaded&&!preloaded.paused,'Zero-fade boundary did not use the preloaded stream');
    assert(!reloaded,'Preloaded stream was discarded at the zero-fade boundary');
    return 'Real audio smoke passed: looping tuning static, decode, scheduled gapless handoff, pause/resume, streaming crossfade, preloaded zero-fade handoff, cleanup.';
  } finally {
    player.dispose();
    URL.revokeObjectURL(noiseUrl);
    urls.forEach((url) => URL.revokeObjectURL(url));
  }
};
