import type { Group, Station } from '../library';
export interface TimedLine {
  at: number;
  text: string;
}
export function parseTimedText(raw: string | null): TimedLine[] {
  const timed: TimedLine[] = [];
  for (const line of (raw ?? '').replace(/^\uFEFF/, '').split(/\r?\n/)) {
    const trimmed = line.trimStart();
    const prefix = trimmed.match(/^(?:\[\d+:[0-5]\d(?:\.\d+)?\])+/)?.[0];
    if (!prefix) continue;
    const text = trimmed.slice(prefix.length).trim();
    for (const match of prefix.matchAll(/\[(\d+):([0-5]\d(?:\.\d+)?)\]/g)) {
      const at = Number(match[1]) * 60 + Number(match[2]);
      if (Number.isFinite(at)) timed.push({ at, text });
      if (timed.length >= 20000) return timed.sort((a, b) => a.at - b.at);
    }
  }
  return timed.sort((a, b) => a.at - b.at);
}
export function currentLine(lines: TimedLine[], seconds: number): number {
  let low = 0,
    high = lines.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (lines[middle].at <= seconds) low = middle + 1;
    else high = middle;
  }
  return low - 1;
}
export function groupLineup(stations: Station[], group: Group | undefined): Station[] {
  if (!group) return stations;
  const members = new Set(group.members);
  return stations.filter(
    (s) => members.has(s.key) || s.tracks.some((t) => members.has(`${s.key}|${t.path}`)),
  );
}
export class StationHistory {
  keys: string[] = [];
  cursor = -1;
  private pending: number | null = null;
  cancelPending() {
    this.pending = null;
  }
  commit(key: string) {
    if (this.keys[this.cursor] === key) return;
    const target = this.pending;
    this.pending = null;
    if (target !== null && this.keys[target] === key) this.cursor = target;
    else {
      this.keys = this.keys.slice(0, this.cursor + 1);
      this.keys.push(key);
      this.cursor++;
    }
    const start = Math.max(0, this.cursor - 2);
    this.keys = this.keys.slice(start, this.cursor + 3);
    this.cursor -= start;
  }
  next(lineup: Station[], direction: number, random = Math.random): Station | undefined {
    this.pending = null;
    for (let i = this.cursor + direction; i >= 0 && i < this.keys.length; i += direction) {
      const found = lineup.find((s) => s.key === this.keys[i]);
      if (found) {
        this.pending = i;
        return found;
      }
    }
    if (direction < 0) return undefined;
    const choices = lineup.filter((s) => s.key !== this.keys[this.cursor]);
    return choices.length ? choices[Math.floor(random() * choices.length)] : lineup[0];
  }
}
