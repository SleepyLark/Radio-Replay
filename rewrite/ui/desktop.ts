import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
export interface DesktopSnapshot {
  station: string;
  title: string;
  artist: string;
  album: string;
  art: string;
  accent: string;
  playing: boolean;
  hasStation: boolean;
  canNavigate: boolean;
  seconds: number;
  duration: number;
  volume: number;
  muted: boolean;
}
export type DesktopAction =
  | { type: 'toggle' | 'play' | 'pause' | 'previous' | 'next' | 'mute' }
  | { type: 'volume'; value: number };
export const initialSnapshot = (): DesktopSnapshot => ({
  station: '',
  title: '',
  artist: '',
  album: '',
  art: '',
  accent: '#c7f464',
  playing: false,
  hasStation: false,
  canNavigate: false,
  seconds: 0,
  duration: 0,
  volume: 0.8,
  muted: false,
});
export const publishDesktop = (snapshot: DesktopSnapshot) =>
  invoke<void>('desktop_publish', { snapshot });
export const getDesktopSnapshot = () => invoke<DesktopSnapshot>('desktop_snapshot');
export const openMini = () => invoke<void>('desktop_open_mini');
export const restoreMain = () => invoke<void>('desktop_restore');
export const quitDesktop = () => invoke<void>('desktop_quit');
export const sendDesktopAction = (action: DesktopAction) =>
  invoke<void>('desktop_action', { action });
export const listenDesktop = (callback: (action: DesktopAction) => void) =>
  listen<DesktopAction>('desktop-command', (event) => callback(event.payload));
export const listenMini = (callback: (state: DesktopSnapshot) => void) =>
  listen<DesktopSnapshot>('mini-state', (event) => callback(event.payload));
