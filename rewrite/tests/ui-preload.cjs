// Test-only Tauri adapter. Exercises the built UI without touching a real library.
const { contextBridge } = require('electron');
let config = {
  root: '',
  custom: [
    {
      key: 'music',
      station: 'Fixture Radio',
      mode: 'tracks',
      tracks: [
        { path: 'first.wav', name: 'First' },
        { path: 'second.wav', name: 'Second' },
      ],
    },
  ],
  groups: [],
  hidden: [],
  logos: {},
  settings: { accent: '#ff8800', crossfade: 0, showProgress: true, tuningTransition: false },
};
for (const [key, station, mode] of [
  ['night', 'Night Drive', 'tracks'],
  ['coast', 'Coastline FM', 'longset'],
  ['ambient', 'Slow Signals', 'tracks'],
  ['jazz', 'After Hours', 'tracks'],
  ['archive', 'Archive 89.4', 'longset'],
])
  config.custom.push({ key, station, mode, tracks: [{ path: 'first.wav', name: 'First' }] });
const calls = [];
const messages = [];
const callbacks = new Map();
const listeners = new Map();
let nextId = 1;
let snapshot = {
  station: 'Fixture Radio',
  title: 'Embedded song',
  artist: 'Fixture artist',
  album: 'Fixture album',
  art: '',
  accent: '#ff8800',
  playing: true,
  hasStation: true,
  canNavigate: true,
  seconds: 5,
  duration: 30,
  volume: 0.8,
  muted: false,
};
contextBridge.exposeInMainWorld('isTauri', true);
contextBridge.exposeInMainWorld('__testCalls', () => calls);
contextBridge.exposeInMainWorld('__testMessages', () => messages);
contextBridge.exposeInMainWorld('__TAURI_EVENT_PLUGIN_INTERNALS__', {
  unregisterListener: (_event, id) => listeners.delete(id),
});
contextBridge.exposeInMainWorld('__TAURI_INTERNALS__', {
  metadata: {
    currentWindow: { label: location.search.includes('mini=1') ? 'mini' : 'main' },
    currentWebview: { label: location.search.includes('mini=1') ? 'mini' : 'main' },
  },
  transformCallback: (callback) => {
    const id = nextId++;
    callbacks.set(id, callback);
    return id;
  },
  convertFileSrc: () => location.origin + '/fixture.wav',
  invoke: async (command, args = {}) => {
    calls.push(command);
    switch (command) {
      case 'plugin:event|listen': {
        const id = nextId++;
        listeners.set(id, args);
        return id;
      }
      case 'plugin:event|unlisten':
        return null;
      case 'desktop_publish':
        snapshot = args.snapshot;
        return null;
      case 'desktop_snapshot':
        return snapshot;
      case 'desktop_open_mini':
      case 'desktop_restore':
      case 'desktop_quit':
        return null;
      case 'desktop_action':
        messages.push(args.action);
        return null;
      case 'load_config':
        return config;
      case 'prepare_station': {
        const group = config.groups.find((g) => g.id === args.group);
        const station = config.custom.find((s) => s.key === args.key);
        return station.tracks
          .filter(
            (t) =>
              !group ||
              group.members.includes(station.key) ||
              group.members.includes(station.key + '|' + t.path),
          )
          .map((t) => ({ ...t, size: 480044, modified: 1, duration: 30 }));
      }
      case 'track_details':
        return {
          tags: {
            title: 'Embedded song',
            artist: 'Fixture artist',
            album: 'Fixture album',
            duration: 30,
          },
          sidecar: '[00:00]Opening lyric\n[00:20]Later lyric',
        };
      case 'remember_duration':
        return null;
      case 'save_station': {
        const edit = args.edit;
        config.custom = config.custom.map((s) =>
          s.key === edit.key
            ? { ...s, ...edit.options, station: edit.name, mode: edit.mode, tracks: edit.tracks }
            : s,
        );
        config.logos[edit.key] = edit.logoPath;
        return config;
      }
      case 'save_group':
        config.groups = config.groups.filter((g) => g.id !== args.group.id).concat(args.group);
        return config;
      case 'delete_group':
        config.groups = config.groups.filter((g) => g.id !== args.id);
        return config;
      case 'save_settings':
        config.settings = { ...config.settings, ...args.patch };
        return config;
      case 'clear_metadata_cache':
        return null;
      default:
        throw new Error('Unexpected test command ' + command);
    }
  },
});
