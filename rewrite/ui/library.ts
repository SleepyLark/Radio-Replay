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
  groups: unknown[];
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
export interface StationEdit {
  key: string;
  name: string;
  mode: string;
  tracks: Track[];
  logoPath: string | null;
}
export const chooseAudio = (folder: boolean) => invoke<Track[]>('choose_audio', { folder });
export const chooseArtwork = () => invoke<string | null>('choose_artwork');
export const saveStation = (edit: StationEdit) => invoke<Config>('save_station', { edit });
export const deleteStation = (key: string) => invoke<Config>('delete_station', { key });
export const artworkUrl = (path: unknown) =>
  desktop && typeof path === 'string' && path ? convertFileSrc(path) : '';
