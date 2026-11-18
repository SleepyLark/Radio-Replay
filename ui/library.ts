import { invoke, isTauri, convertFileSrc } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {LibraryUpdate} from './library-update';
export interface Track {
  path: string;
  name: string;
  [key: string]: unknown;
}
export interface Station {
  key: string;
  station: string;
  tracks: Track[];
  bumpers?: BumperSettings|null;
  [key: string]: unknown;
}
export interface BumperSettings {enabled:boolean;folder:string;method:'songs'|'time';min:number;max:number;}
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
// Order mutations and their replies across editors/settings, including a final
// settings flush after navigation. A failed write does not stall later requests.
let writes:Promise<void>=Promise.resolve();
const write=<T>(command:string,args?:Record<string,unknown>)=>{
  const result=writes.then(()=>invoke<T>(command,args));
  writes=result.then(()=>{},()=>{});return result;
};
export interface AudioLevels {volume:number;muted:boolean;}
export interface PlaybackStatus {backend:'web'|'mpv';error:string|null;warning:string|null;levels:AudioLevels;}
export const playbackBackend = () => invoke<PlaybackStatus>('playback_backend');
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
export const importConfig = () => write<Config | null>('import_config');
export interface RecoveryInfo {error:string;canRestore:boolean;canReset:boolean;}
export const libraryRecoveryInfo = () => invoke<RecoveryInfo>('library_recovery_info');
export const recoverLibrary = (action:'restoreBackup'|'startFresh') => write<Config>('recover_library',{action});
export const exportConfig = () => invoke<boolean>('export_config');
export interface PlayableTrack extends Track {
  size: number;
  modified: number;
  duration?: number | null;
}
export interface TrackProblem {
  path: string;
  name: string;
  reason: string;
  kind: 'file' | 'cache' | 'scan';
}
export interface PreparedStation {
  tracks: PlayableTrack[];
  bumpers?: PlayableTrack[];
  problems: TrackProblem[];
}
export interface IndexedTrack {slot:number;id:string;name:string;duration:number|null;}
export interface IndexedStation {revision:string;tracks:IndexedTrack[];bumpers:IndexedTrack[];problems:TrackProblem[];}
export const prepareStation = (key: string, group = 'all') =>
  invoke<PreparedStation>('prepare_station', { key, group });
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
export const saveGroup = (group: Group) => write<LibraryUpdate>('save_group', { group });
export const deleteGroup = (id: string) => write<LibraryUpdate>('delete_group', { id });
export const saveSettings = (patch: Record<string, unknown>) =>
  write<LibraryUpdate>('save_settings', { patch });
export const clearMetadataCache = () => invoke<void>('clear_metadata_cache');
export const audioUrl = (path: string) => convertFileSrc(path);
export const resolveTuningSound = () => invoke<string | null>('resolve_tuning_sound');
export interface ScanNotices {skippedFolders:number;skippedEntries:number;issues:{path:string;reason:string}[];}
export interface ScanReport {tracks:Track[];notices:ScanNotices;}
export const recoverLegacyLibrary = () => write<{config:Config;notices:ScanNotices}>('recover_legacy_library');
export interface StationEdit {
  key: string;
  name: string;
  mode: string;
  tracks: Track[];
  logoPath: string | null;
  options?: Record<string, unknown>;
}
export const chooseAudio = (folder: boolean) => invoke<ScanReport|null>('choose_audio', { folder });
export interface PlaylistImport {name:string;tracks:Track[];issues:{line:number;entry:string;reason:string}[];issueCount:number;network?:boolean;}
export const choosePlaylist = (networkOnly=false) => invoke<PlaylistImport|null>('choose_playlist',{networkOnly});
export const addNetworkStream = (url:string,name:string) => invoke<Track>('add_network_stream',{url,name});
export const chooseBumperFolder = () => invoke<string|null>('choose_bumper_folder');
export const chooseArtwork = () => invoke<string | null>('choose_artwork');
export const saveStation = (edit: StationEdit) => write<LibraryUpdate>('save_station', { edit });
export const removeStationTracks = (key:string,paths:string[]) => write<LibraryUpdate>('remove_station_tracks',{key,paths});
export const deleteStation = (key: string) => write<LibraryUpdate>('delete_station', { key });
export const deleteStations = (keys: string[]) => write<LibraryUpdate>('delete_stations', { keys });
export const showConfig = () => invoke<void>('open_config');
export interface AudioDrop {
  id: number;
  tracks: Track[];
  error: string;
  notices: ScanNotices;
}
export async function listenAudioDrops(
  start: (id: number) => void,
  finish: (drop: AudioDrop) => void,
) {
  const listeners = await Promise.all([
    listen<number>('audio-drop-start', (event) => start(event.payload)),
    listen<AudioDrop>('audio-drop', (event) => finish(event.payload)),
  ]);
  return () => listeners.forEach((stop) => stop());
}
export const artworkUrl = (path: unknown) =>
  desktop && typeof path === 'string' && path ? convertFileSrc(path) : '';
