import type { PlayableTrack, Station } from '../library';
export interface Clock {
  order: PlayableTrack[];
  lengths: number[];
  steps: number[];
  fade: number;
  total: number;
  origin: number;
  offset: number;
}
export function fadeSeconds(station: Station, settings: Record<string, unknown>) {
  const value = station.crossfade ?? (station.mode === 'tracks' ? (settings.crossfade ?? 5000) : 0);
  if (typeof value !== 'number' || !Number.isFinite(value)) return 0;
  return Math.min(30000, Math.max(0, value > 0 && value <= 30 ? value * 1000 : value)) / 1000;
}
export function shuffle<T>(values: T[], random = Math.random): T[] {
  const copy = [...values];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}
export function clockFor(
  order: PlayableTrack[],
  lengths: number[],
  requestedFade: number,
  now: number,
  random = Math.random,
): Clock {
  const fade = order.length > 1 ? Math.min(requestedFade, ...lengths.map((n) => n / 2)) : 0;
  const steps = lengths.map((n) => n - fade);
  const total = steps.reduce((a, b) => a + b, 0);
  return { order, lengths, steps, fade, total, origin: now, offset: random() * total };
}
export function position(clock: Clock, now: number) {
  let elapsed =
    (((clock.offset + (now - clock.origin) / 1000) % clock.total) + clock.total) % clock.total;
  for (let index = 0; index < clock.steps.length; index++) {
    if (elapsed < clock.steps[index] || index === clock.steps.length - 1)
      return { index, seconds: elapsed };
    elapsed -= clock.steps[index];
  }
  return { index: 0, seconds: 0 };
}
