import type { Config, TrackProblem } from './library';
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

/** Only file failures that refer to this saved station; cache/folder warnings are not removals. */
export function removableProblemPaths(notification:Notification,config:Config):string[]{
  const paths=new Set(notification.problems.filter(p=>p.kind==='file').map(p=>displayPath(p.path)));
  return config.custom.find(s=>s.key===notification.stationKey)?.tracks.filter(t=>paths.has(displayPath(t.path))).map(t=>t.path)??[];
}
