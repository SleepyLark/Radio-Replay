import { invoke, isTauri } from '@tauri-apps/api/core';
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
