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
  settings: { accent: '#ff8800', crossfade: 0, showProgress: true },
};
const calls = [];
contextBridge.exposeInMainWorld('isTauri', true);
contextBridge.exposeInMainWorld('__testCalls', () => calls);
contextBridge.exposeInMainWorld('__TAURI_INTERNALS__', {
  convertFileSrc: () => location.origin + '/fixture.wav',
  invoke: async (command, args = {}) => {
    calls.push(command);
    switch (command) {
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
