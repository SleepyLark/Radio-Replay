import type { PlayableTrack, Station } from '../library';
import { clockFor, fadeSeconds, position, shuffle, type Clock } from './timeline.ts';
import { tuningMilliseconds } from './tuning.ts';

export interface PlaybackState {
  key: string;
  station: string;
  title: string;
  path: string;
  playing: boolean;
  loading: boolean;
  seconds: number;
  duration: number;
  volume: number;
  muted: boolean;
  error: string;
  engine: 'stream' | 'buffered';
}
interface Entry {
  index: number;
  buffer: AudioBuffer;
  source: AudioBufferSourceNode;
  when: number;
  offset: number;
  end: number;
}
interface Buffered {
  context: AudioContext;
  gain: GainNode;
  entries: Entry[];
  buffers: Map<string, AudioBuffer>;
  loading: boolean;
}
export interface PlaybackOptions {
  prepare: (key: string) => Promise<PlayableTrack[]>;
  url: (path: string) => string;
  onState: (state: PlaybackState) => void;
  rememberDuration?: (key: string, track: PlayableTrack, duration: number) => Promise<void>;
  tuningSource?: (value: unknown) => Promise<string | null>;
  audio?: () => HTMLAudioElement;
  context?: () => AudioContext;
  fetchBytes?: (url: string, signal: AbortSignal) => Promise<ArrayBuffer>;
  now?: () => number;
  random?: () => number;
}
export class Playback {
  state: PlaybackState = {
    key: '',
    station: '',
    title: '',
    path: '',
    playing: false,
    loading: false,
    seconds: 0,
    duration: 0,
    volume: 0.8,
    muted: false,
    error: '',
    engine: 'stream',
  };
  private options: PlaybackOptions;
  private now: () => number;
  private random: () => number;
  private current: HTMLAudioElement;
  private spare: HTMLAudioElement;
  private controller = new AbortController();
  private token = 0;
  private wantPlay = false;
  private clock: Clock | null = null;
  private index = 0;
  private clocks = new Map<string, { signature: string; clock: Clock }>();
  private durations = new Map<string, number>();
  private buffered: Buffered | null = null;
  private fading: {
    started: number;
    outgoing: HTMLAudioElement;
    incoming: HTMLAudioElement;
    index: number;
  } | null = null;
  private advancing = false;
  private noise: HTMLAudioElement | null = null;
  private timer: ReturnType<typeof setInterval>;
  constructor(options: PlaybackOptions) {
    this.options = options;
    this.now = options.now ?? (() => performance.now());
    this.random = options.random ?? Math.random;
    this.current = (options.audio ?? (() => new Audio()))();
    this.spare = (options.audio ?? (() => new Audio()))();
    for (const element of [this.current, this.spare]) {
      element.addEventListener('ended', () => {
        if (element === this.current && !this.fading && !this.buffered && this.wantPlay)
          void this.advance();
      });
      element.addEventListener('error', () => {
        if (element === this.current && !this.state.loading && this.wantPlay && !this.fading)
          void this.advance();
      });
    }
    this.timer = setInterval(() => this.tick(), 40);
  }
  private emit(update: Partial<PlaybackState> = {}) {
    this.state = { ...this.state, ...update };
    this.options.onState({ ...this.state });
  }
  private level() {
    return this.state.muted ? 0 : this.state.volume;
  }
  private release(element: HTMLAudioElement) {
    element.pause();
    element.removeAttribute('src');
    element.load();
  }
  private stopSources() {
    if (this.noise) {
      this.release(this.noise);
      this.noise = null;
    }
    this.fading = null;
    this.release(this.current);
    this.release(this.spare);
    const state = this.buffered;
    this.buffered = null;
    if (state) {
      for (const entry of state.entries) {
        entry.source.onended = null;
        try {
          entry.source.stop();
        } catch {}
      }
      void state.context.close().catch(() => {});
    }
  }
  stop() {
    this.token++;
    this.controller.abort();
    this.controller = new AbortController();
    this.wantPlay = false;
    this.stopSources();
    this.clock = null;
    this.advancing = false;
    this.emit({
      key: '',
      station: '',
      title: '',
      path: '',
      playing: false,
      loading: false,
      seconds: 0,
      duration: 0,
      error: '',
    });
  }
  dispose() {
    this.stop();
    clearInterval(this.timer);
  }
  invalidate() {
    this.stop();
    this.clearCache();
  }
  clearCache() {
    this.clocks.clear();
    this.durations.clear();
  }
  setVolume(volume: number) {
    this.emit({ volume: Math.max(0, Math.min(1, volume)) });
    this.applyVolume();
  }
  mute() {
    this.emit({ muted: !this.state.muted });
    this.applyVolume();
  }
  private applyVolume() {
    if (this.noise) this.noise.volume = this.level();
    if (this.buffered) this.buffered.gain.gain.value = this.level();
    if (!this.fading) {
      this.current.volume = this.level();
      this.spare.volume = this.level();
    }
  }
  private async ready(element: HTMLAudioElement, signal: AbortSignal) {
    if (signal.aborted) throw new Error('Tuning changed');
    if (element.readyState >= 1 && Number.isFinite(element.duration) && element.duration > 0)
      return;
    await new Promise<void>((resolve, reject) => {
      const finish = (error?: Error) => {
        clearTimeout(timeout);
        element.removeEventListener('loadedmetadata', loaded);
        element.removeEventListener('error', failed);
        signal.removeEventListener('abort', aborted);
        error ? reject(error) : resolve();
      };
      const loaded = () =>
        Number.isFinite(element.duration) && element.duration > 0 ? finish() : failed();
      const failed = () => finish(new Error('Audio is missing or this codec is unsupported'));
      const aborted = () => finish(new Error('Tuning changed'));
      const timeout = setTimeout(() => finish(new Error('Audio metadata timed out')), 10000);
      element.addEventListener('loadedmetadata', loaded);
      element.addEventListener('error', failed);
      signal.addEventListener('abort', aborted, { once: true });
    });
  }
  private async probe(track: PlayableTrack, signal: AbortSignal, stationKey: string) {
    if (typeof track.duration === 'number' && Number.isFinite(track.duration) && track.duration > 0)
      return track.duration;
    const key = `${track.path}:${track.size}:${track.modified}`;
    const cached = this.durations.get(key);
    if (cached) return cached;
    const element = (this.options.audio ?? (() => new Audio()))();
    element.preload = 'metadata';
    element.src = this.options.url(track.path);
    try {
      await this.ready(element, signal);
      const duration = element.duration;
      this.durations.set(key, duration);
      void this.options.rememberDuration?.(stationKey, track, duration).catch(() => {});
      if (this.durations.size > 20000) this.durations.delete(this.durations.keys().next().value!);
      return duration;
    } catch (e) {
      if (signal.aborted) throw e;
      return 0;
    } finally {
      this.release(element);
    }
  }
  private beginTuning(settings: Record<string, unknown>, token: number, signal: AbortSignal) {
    const ms = this.options.tuningSource ? tuningMilliseconds(settings, this.random) : 0;
    let done = false;
    let sound: HTMLAudioElement | null = null;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    let finishMinimum = () => {};
    const minimum = new Promise<void>((resolve) => {
      finishMinimum = resolve;
      if (ms) timeout = setTimeout(resolve, ms);
      else resolve();
    });
    const stop = () => {
      if (done) return;
      done = true;
      clearTimeout(timeout);
      finishMinimum();
      signal.removeEventListener('abort', stop);
      if (sound) {
        this.release(sound);
        if (this.noise === sound) this.noise = null;
      }
    };
    signal.addEventListener('abort', stop, { once: true });
    if (signal.aborted) stop();
    void this.options
      .tuningSource?.(settings.staticPath)
      .then((source) => {
        if (!source || done || token !== this.token || signal.aborted || !this.wantPlay) return;
        sound = (this.options.audio ?? (() => new Audio()))();
        this.noise = sound;
        sound.loop = true;
        sound.src = source;
        sound.volume = this.level();
        void sound.play().catch(() => {});
      })
      .catch(() => {});
    // The timer sets a minimum delay. Only the owning tune (or cancellation)
    // stops static, so it covers preparation, metadata probes and audio startup.
    return { minimum, stop };
  }
  async tune(station: Station, settings: Record<string, unknown> = {}, context = 'all') {
    this.stop();
    const token = this.token;
    const signal = this.controller.signal;
    this.wantPlay = true;
    const transition = this.beginTuning(settings, token, signal);
    this.emit({
      key: station.key,
      station: station.station,
      loading: true,
      error: '',
      title: 'Tuning…',
    });
    try {
      const tracks = await this.options.prepare(station.key);
      if (token !== this.token) return;
      const lengths = new Array<number>(tracks.length);
      let next = 0;
      await Promise.all(
        Array.from({ length: Math.min(6, tracks.length) }, async () => {
          while (next < tracks.length && !signal.aborted) {
            const i = next++;
            lengths[i] = await this.probe(tracks[i], signal, station.key);
          }
        }),
      );
      if (token !== this.token) return;
      const playable = tracks.filter((_, i) => lengths[i] > 0);
      if (!playable.length)
        throw new Error(
          'No playable tracks in this station. Check the file paths and codec support.',
        );
      const durationByPath = new Map(tracks.map((t, i) => [t.path, lengths[i]]));
      const requestedFade = fadeSeconds(station, settings);
      const signature = JSON.stringify([
        playable.map((t) => [t.path, t.size, t.modified]),
        station.mode,
        requestedFade,
      ]);
      const clockKey = `${context}:${station.key}`;
      const existing = this.clocks.get(clockKey);
      const order =
        existing?.signature === signature
          ? existing.clock.order
          : station.mode === 'longset'
            ? playable
            : shuffle(playable, this.random);
      const clock =
        existing?.signature === signature
          ? existing.clock
          : clockFor(
              order,
              order.map((t) => durationByPath.get(t.path)!),
              requestedFade,
              this.now(),
              this.random,
            );
      this.clock = clock;
      this.clocks.delete(clockKey);
      this.clocks.set(clockKey, { signature, clock });
      let retained = [...this.clocks.values()].reduce((n, c) => n + c.clock.order.length, 0);
      for (const [key, value] of this.clocks) {
        if (this.clocks.size <= 32 && retained <= 20000) break;
        if (key === clockKey) continue;
        this.clocks.delete(key);
        retained -= value.clock.order.length;
      }
      await transition.minimum;
      if (token !== this.token) return;
      const at = position(clock, this.now());
      this.index = at.index;
      const skipped = station.tracks.length - playable.length;
      this.emit({
        error: skipped > 0 ? `${skipped} unavailable or unsupported track(s) skipped.` : '',
      });
      if (
        station.mode === 'longset' &&
        !clock.fade &&
        clock.order.length > 1 &&
        clock.lengths.every((n) => n <= 600) &&
        clock.order.every((t) => t.size <= 64 * 1024 * 1024)
      ) {
        if (await this.startBuffered(at.seconds, token)) {
          if (token === this.token) this.emit({ loading: false });
          return;
        }
      }
      if (token !== this.token) return;
      try {
        await this.startStream(at.index, at.seconds, token);
      } catch (e) {
        if (token !== this.token) return;
        this.emit({ error: String(e), loading: false });
        await this.advance();
      }
    } catch (e) {
      if (token === this.token) {
        this.wantPlay = false;
        this.stopSources();
        this.emit({
          playing: false,
          loading: false,
          title: 'Playback unavailable',
          error: String(e),
        });
      }
    } finally {
      transition.stop();
    }
  }
  private async startStream(index: number, seconds: number, token: number) {
    const clock = this.clock!;
    const element = this.current;
    element.src = this.options.url(clock.order[index].path);
    element.preload = 'auto';
    element.loop = clock.order.length === 1;
    await this.ready(element, this.controller.signal);
    if (token !== this.token) return;
    element.currentTime = Math.min(seconds, Math.max(0, element.duration - 0.01));
    element.volume = this.level();
    if (this.wantPlay) {
      await element.play();
      if (token !== this.token) return;
      if (!this.wantPlay) element.pause();
    }
    this.index = index;
    this.emit({
      engine: 'stream',
      title: clock.order[index].name,
      path: clock.order[index].path,
      loading: false,
      playing: this.wantPlay,
      duration: element.duration,
      seconds: element.currentTime,
    });
    this.preload();
  }
  private preload() {
    if (!this.clock || this.clock.order.length < 2) return;
    this.spare.preload = 'auto';
    this.spare.loop = false;
    this.spare.src = this.options.url(
      this.clock.order[(this.index + 1) % this.clock.order.length].path,
    );
  }
  private async advance() {
    if (this.advancing || !this.clock || !this.wantPlay) return;
    this.advancing = true;
    const token = this.token;
    const clock = this.clock;
    const start = this.index;
    this.release(this.current);
    this.release(this.spare);
    try {
      for (let attempted = 1; attempted <= clock.order.length; attempted++) {
        const index = (start + attempted) % clock.order.length;
        try {
          await this.startStream(index, 0, token);
          if (token !== this.token) return;
          clock.origin = this.now();
          clock.offset = clock.steps.slice(0, index).reduce((a, b) => a + b, 0);
          return;
        } catch (e) {
          if (token !== this.token) return;
          this.emit({ error: `Skipped ${clock.order[index].name}: ${String(e)}` });
        }
      }
      this.wantPlay = false;
      this.emit({
        playing: false,
        loading: false,
        error: 'No playable tracks remain in this station.',
      });
    } finally {
      if (token === this.token) this.advancing = false;
    }
  }
  async pause() {
    const token = this.token;
    const buffered = this.buffered;
    this.wantPlay = false;
    this.noise?.pause();
    this.current.pause();
    this.spare.pause();
    this.emit({ playing: false });
    try {
      if (buffered) await buffered.context.suspend();
    } catch (e) {
      if (token === this.token) this.emit({ error: String(e) });
    }
  }
  async play() {
    if (!this.clock || this.state.loading) return;
    this.wantPlay = true;
    const token = this.token;
    try {
      if (this.buffered) await this.buffered.context.resume();
      else {
        await this.current.play();
        if (token !== this.token) return;
        if (this.fading) await this.spare.play();
      }
      if (token === this.token) {
        if (!this.wantPlay) {
          this.current.pause();
          this.spare.pause();
        }
        this.emit({ playing: this.wantPlay });
      }
    } catch (e) {
      if (token === this.token) {
        this.wantPlay = false;
        this.emit({ playing: false, error: String(e) });
      }
    }
  }
  private async beginFade() {
    if (!this.clock || this.fading || !this.wantPlay) return;
    const token = this.token;
    const incoming = this.spare;
    const outgoing = this.current;
    const index = (this.index + 1) % this.clock.order.length;
    const fade = { started: 0, outgoing, incoming, index };
    this.fading = fade;
    try {
      await this.ready(incoming, this.controller.signal);
      if (token !== this.token || this.fading !== fade) return;
      incoming.currentTime = 0;
      incoming.volume = 0;
      await incoming.play();
      if (token !== this.token || this.fading !== fade) return;
      if (!this.wantPlay) incoming.pause();
      fade.started = incoming.currentTime;
    } catch {
      if (token === this.token && this.fading === fade) {
        this.fading = null;
        this.release(incoming);
        if (outgoing.ended) void this.advance();
      }
    }
  }
  private tick() {
    if (!this.clock || this.state.loading) return;
    if (this.buffered) {
      const entry = this.buffered.entries[0];
      if (entry)
        this.emit({
          seconds: Math.max(
            0,
            Math.min(
              entry.buffer.duration,
              entry.offset + this.buffered.context.currentTime - entry.when,
            ),
          ),
          duration: entry.buffer.duration,
        });
      return;
    }
    const fade = this.fading;
    if (fade) {
      const progress = Math.min(
        1,
        Math.max(0, (fade.incoming.currentTime - fade.started) / this.clock.fade),
      );
      fade.outgoing.volume = this.level() * Math.cos((progress * Math.PI) / 2);
      fade.incoming.volume = this.level() * Math.sin((progress * Math.PI) / 2);
      if (progress >= 1) {
        this.release(fade.outgoing);
        this.current = fade.incoming;
        this.spare = fade.outgoing;
        this.index = fade.index;
        this.fading = null;
        this.emit({
          title: this.clock.order[this.index].name,
          path: this.clock.order[this.index].path,
        });
        this.preload();
      }
    } else if (
      this.wantPlay &&
      this.clock.fade &&
      this.current.currentTime >= this.current.duration - this.clock.fade
    ) {
      void this.beginFade();
    }
    this.emit({
      seconds: this.current.currentTime || 0,
      duration: Number.isFinite(this.current.duration) ? this.current.duration : 0,
    });
  }
  private async decode(state: Buffered, index: number) {
    const track = this.clock!.order[index];
    const cached = state.buffers.get(track.path);
    if (cached) return cached;
    const bytes = await (
      this.options.fetchBytes ??
      (async (url, signal) => {
        const response = await fetch(url, { signal });
        if (!response.ok) throw new Error('Could not read audio');
        return response.arrayBuffer();
      })
    )(this.options.url(track.path), this.controller.signal);
    if (bytes.byteLength > 64 * 1024 * 1024) throw new Error('Recording exceeds buffer limit');
    const buffer = await state.context.decodeAudioData(bytes);
    const memory = buffer.length * buffer.numberOfChannels * 4;
    if (
      buffer.duration > 600 ||
      memory > 64 * 1024 * 1024 ||
      [...state.buffers.values()].reduce((n, b) => n + b.length * b.numberOfChannels * 4, memory) >
        128 * 1024 * 1024
    )
      throw new Error('Decoded audio exceeds buffer budget');
    state.buffers.set(track.path, buffer);
    return buffer;
  }
  private schedule(state: Buffered, index: number, buffer: AudioBuffer, when: number, offset = 0) {
    const source = state.context.createBufferSource();
    source.buffer = buffer;
    source.connect(state.gain);
    const entry: Entry = {
      index,
      buffer,
      source,
      when,
      offset,
      end: when + buffer.duration - offset,
    };
    source.onended = () => {
      if (this.buffered !== state || state.entries[0] !== entry) return;
      state.entries.shift();
      this.index = index;
      if (!state.entries.some((e) => e.index === index))
        state.buffers.delete(this.clock!.order[index].path);
      if (!state.entries.length) {
        this.buffered = null;
        void state.context.close().catch(() => {});
        this.emit({ engine: 'stream' });
        void this.advance();
        return;
      }
      this.index = state.entries[0].index;
      this.emit({
        title: this.clock!.order[this.index].name,
        path: this.clock!.order[this.index].path,
      });
      void this.fill(state);
    };
    state.entries.push(entry);
    source.start(when, offset);
    return entry;
  }
  private async fill(state: Buffered) {
    if (state.loading || this.buffered !== state || state.entries.length >= 3) return;
    state.loading = true;
    try {
      const tail = state.entries.at(-1)!;
      const index = (tail.index + 1) % this.clock!.order.length;
      const buffer = await this.decode(state, index);
      if (this.buffered !== state) return;
      // If decoding missed the boundary, drain the scheduled entries and stream the following track.
      if (tail.end <= state.context.currentTime) return;
      this.schedule(state, index, buffer, tail.end);
    } catch {
      /* Existing entries drain into streaming playback on failure. */
    } finally {
      state.loading = false;
    }
  }
  private async startBuffered(offset: number, token: number) {
    const context = (this.options.context ?? (() => new AudioContext()))();
    const state: Buffered = {
      context,
      gain: context.createGain(),
      entries: [],
      buffers: new Map(),
      loading: false,
    };
    state.gain.connect(context.destination);
    state.gain.gain.value = this.level();
    try {
      const first = await this.decode(state, this.index);
      if (token !== this.token) return false;
      const nextIndex = (this.index + 1) % this.clock!.order.length;
      const next = await this.decode(state, nextIndex);
      if (token !== this.token) return false;
      if (this.wantPlay) await context.resume();
      else await context.suspend();
      if (token !== this.token) return false;
      if (!this.wantPlay) await context.suspend();
      this.buffered = state;
      const firstEntry = this.schedule(
        state,
        this.index,
        first,
        context.currentTime + 0.1,
        Math.min(offset, Math.max(0, first.duration - 0.01)),
      );
      this.schedule(state, nextIndex, next, firstEntry.end);
      void this.fill(state);
      this.emit({
        engine: 'buffered',
        title: this.clock!.order[this.index].name,
        path: this.clock!.order[this.index].path,
        playing: this.wantPlay,
        duration: first.duration,
        seconds: offset,
      });
      return true;
    } catch {
      if (this.buffered === state) {
        this.buffered = null;
        for (const entry of state.entries) {
          entry.source.onended = null;
          try {
            entry.source.stop();
          } catch {}
        }
      }
      return false;
    } finally {
      if (this.buffered !== state) void context.close().catch(() => {});
    }
  }
}
