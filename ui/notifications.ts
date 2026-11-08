import type { TrackProblem } from './library';
export interface Notification {
  id: number;
  source: 'app' | 'playback';
  severity: 'warning' | 'error';
  message: string;
  stationKey?: string;
  station?: string;
  problems: TrackProblem[];
}
export function displayPath(path: string): string {
  if (path.startsWith('\\\\?\\UNC\\')) return '\\\\' + path.slice(8);
  return path.startsWith('\\\\?\\') ? path.slice(4) : path;
}
