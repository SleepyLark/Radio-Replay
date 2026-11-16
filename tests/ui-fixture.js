// Test-only Tauri adapter. Exercises the built UI without touching a real library.
const contextBridge={exposeInMainWorld:(key,value)=>{window[key]=value;}};
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
        { path: 'C:\\Radio\\missing.mp3', name: 'Missing recording' },
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
let rejectRemoval=false;
contextBridge.exposeInMainWorld('__testFailRemoval',()=>rejectRemoval=true);
let playlistResult;
contextBridge.exposeInMainWorld('__testPlaylist',value=>playlistResult=value);
const calls = [];
if(new URLSearchParams(location.search).has('large')) {
  config.custom=[{key:'large',station:'Large Library',mode:'tracks',tracks:Array.from({length:20000},(_,i)=>({path:`C:/Fixture/Album ${Math.floor(i/1000)}/track-${i}.mp3`,name:`Track ${i}`}))}];
}
contextBridge.exposeInMainWorld('__testConfig',()=>config);
const settingPatches = [];
let rejectSetting = false;
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
  banner: location.origin + '/fixture-banner.svg',
  bannerColor: '#244b42',
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
contextBridge.exposeInMainWorld('__testSettings', () => config.settings);
contextBridge.exposeInMainWorld('__testSettingPatches', () => settingPatches);
contextBridge.exposeInMainWorld('__testFailSetting', () => {
  rejectSetting = true;
});
contextBridge.exposeInMainWorld('__testMessages', () => messages);
contextBridge.exposeInMainWorld('__testEmit', (event, payload) => {
  for (const [id, args] of listeners) {
    if (args.event === event) callbacks.get(args.handler)?.({ event, id, payload });
  }
});
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
      case 'open_config':
      case 'desktop_restore':
      case 'desktop_quit':
        return null;
      case 'desktop_action':
        messages.push(args.action);
        return null;
      case 'load_config':
        return structuredClone(config);
      case 'playback_backend':
        return 'web';
      case 'prepare_station': {
        const group = config.groups.find((g) => g.id === args.group);
        const station = config.custom.find((s) => s.key === args.key);
        const selected = station.tracks.filter(
          (t) =>
            !group ||
            group.members.includes(station.key) ||
            group.members.includes(station.key + '|' + t.path),
        );
        return {
          tracks: selected
            .filter((t) => !t.path.includes('missing.mp3'))
            .map((t) => ({ ...t, size: 480044, modified: 1, duration: 30 })),
          problems: selected
            .filter((t) => t.path.includes('missing.mp3'))
            .map((t) => ({
              ...t,
              kind: 'file',
              reason: 'File not found. It may have been moved or deleted.',
            })),
        };
      }
      case 'track_details':
        return {
          tags: {
            title:
              args.key === 'ambient'
                ? 'A long late night transmission from the quiet end of the radio dial — continuous music for the road ahead'
                : 'Embedded song',
            artist: 'Fixture artist',
            album: 'Fixture album',
            duration: 30,
          },
          sidecar: '[00:00]Opening lyric\n[00:20]Later lyric',
        };
      case 'remember_duration':
        return null;
      case 'add_network_stream': {
        const url=new URL(args.url.trim());
        if(!['http:','https:'].includes(url.protocol)||url.hostname==='127.0.0.1')throw new Error('Fixture blocked URL');
        return {path:url.href,name:args.name.trim()||url.hostname,source:'network'};
      }
      case 'choose_playlist':
        window.__lastPlaylistNetworkOnly=args.networkOnly;
        if(playlistResult==='error')throw new Error('Fixture invalid playlist');
        return playlistResult===undefined?{name:'Imported playlist',tracks:[{path:'first.wav',name:'First'},{path:'playlist-two.wav',name:'Playlist Two'},{path:'playlist-three.wav',name:'Playlist Three'}],issues:[{line:6,entry:'https://example.invalid/live',reason:'Network streams are not supported in local playlist imports'}],issueCount:1}:playlistResult;
      case 'choose_bumper_folder':
        return 'C:\\Radio\\Bumpers';
      case 'remove_station_tracks': {
        if(rejectRemoval){rejectRemoval=false;throw new Error('Fixture removal failed');}
        const paths=new Set(args.paths);config.custom=config.custom.map(s=>s.key===args.key?{...s,tracks:s.tracks.filter(t=>!paths.has(t.path))}:s);
        const members=new Set(args.paths.map(p=>args.key+'|'+p));config.groups=config.groups.map(g=>({...g,members:g.members.filter(m=>!members.has(m))}));
        return structuredClone(config);
      }
      case 'save_station': {
        const edit = args.edit;
        config.custom = config.custom.map((s) =>
          s.key === edit.key
            ? { ...s, ...edit.options, station: edit.name, mode: edit.mode, tracks: edit.tracks }
            : s,
        );
        if(!config.custom.some(s=>s.key===edit.key))config.custom.push({key:edit.key,station:edit.name,mode:edit.mode,tracks:edit.tracks,...edit.options});
        config.logos[edit.key] = edit.logoPath;
        return structuredClone(config);
      }
      case 'save_group':
        config.groups = config.groups.filter((g) => g.id !== args.group.id).concat(args.group);
        return structuredClone(config);
      case 'delete_group':
        config.groups = config.groups.filter((g) => g.id !== args.id);
        return structuredClone(config);
      case 'save_settings':
        if (rejectSetting) {
          rejectSetting = false;
          throw new Error('Fixture write failure');
        }
        settingPatches.push(args.patch);
        config.settings = { ...config.settings, ...args.patch };
        return structuredClone(config);
      case 'delete_stations': {
        const removed = new Set(args.keys);
        config.custom = config.custom.filter((s) => !removed.has(s.key));
        config.groups = config.groups.map((g) => ({
          ...g,
          members: g.members.filter(
            (m) => ![...removed].some((k) => m === k || m.startsWith(k + '|')),
          ),
        }));
        return structuredClone(config);
      }
      case 'clear_metadata_cache':
        return null;
      default:
        throw new Error('Unexpected test command ' + command);
    }
  },
});

const mediaHandlers=new Map();
const setMediaAction=navigator.mediaSession.setActionHandler.bind(navigator.mediaSession);
navigator.mediaSession.setActionHandler=(name,handler)=>{mediaHandlers.set(name,handler);setMediaAction(name,handler);};
window.__testMediaAction=name=>mediaHandlers.get(name)?.({action:name});
