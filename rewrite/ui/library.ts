import { invoke, isTauri, convertFileSrc } from '@tauri-apps/api/core';
export interface Track {
  path: string;
  name: string;
  [key: string]: unknown;
}
export interface Station {
  key: string;
  station: string;
  tracks: Track[];
  [key: string]: unknown;
}
export interface Config {
  root: string;
  custom: Station[];
  hidden: string[];
  groups: Group[];
  logos: Record<string, unknown>;
  settings: Record<string, unknown>;
  [key: string]: unknown;
}
export const desktop = isTauri();
export const emptyConfig = (): Config => ({
  root: '',
  custom: [],
  hidden: [],
  groups: [],
  logos: {},
  settings: {},
});
export const loadConfig = () =>
  desktop ? invoke<Config>('load_config') : Promise.resolve(emptyConfig());
export const importConfig = () => invoke<Config | null>('import_config');
export interface PlayableTrack extends Track {
  size: number;
  modified: number;
  duration?: number | null;
}
export const prepareStation = (key: string, group = 'all') =>
  invoke<PlayableTrack[]>('prepare_station', { key, group });
export interface Group {
  id: string;
  name: string;
  members: string[];
}
export interface TrackDetails {
  tags: { title: string; artist: string; album: string; duration: number | null };
  sidecar: string | null;
}
export const trackDetails = (key: string, path: string) =>
  invoke<TrackDetails>('track_details', { key, path });
export const rememberDuration = (key: string, track: PlayableTrack, duration: number) =>
  invoke<void>('remember_duration', {
    key,
    path: track.path,
    size: track.size,
    modified: track.modified,
    duration,
  });
export const saveGroup = (group: Group) => invoke<Config>('save_group', { group });
export const deleteGroup = (id: string) => invoke<Config>('delete_group', { id });
export const saveSettings = (patch: Record<string, unknown>) =>
  invoke<Config>('save_settings', { patch });
export const clearMetadataCache = () => invoke<void>('clear_metadata_cache');
export const audioUrl = (path: string) => convertFileSrc(path);
export interface StationEdit {
  key: string;
  name: string;
  mode: string;
  tracks: Track[];
  logoPath: string | null;
  options?: Record<string, unknown>;
}
export const chooseAudio = (folder: boolean) => invoke<Track[]>('choose_audio', { folder });
export const chooseArtwork = () => invoke<string | null>('choose_artwork');
export const saveStation = (edit: StationEdit) => invoke<Config>('save_station', { edit });
export const deleteStation = (key: string) => invoke<Config>('delete_station', { key });
export const artworkUrl = (path: unknown) =>
  desktop && typeof path === 'string' && path ? convertFileSrc(path) : '';
