const { app, BrowserWindow, ipcMain, nativeImage } = require('electron');
const path = require('node:path');
const fs = require('node:fs/promises');
const { pathToFileURL } = require('node:url');
const indexPath = path.join(__dirname, '..', 'src', 'renderer', 'index.html');
const indexUrl = pathToFileURL(indexPath).href;
// Keep browser storage and caches separate from the user's installed radio.
const profilePath = path.join(__dirname, '..', 'out', 'smoke-profile');
require('node:fs').mkdirSync(profilePath, { recursive: true });
app.setPath('userData', profilePath);
let savedConfig;
let droppedPaths = [];

for (const [channel, response] of [
  ['static:bundled', path.join(__dirname, '..', 'assets', 'audio', 'static.mp3')],
  ['config:load', null],
  ['config:save', 'test-config.json'],
  ['tags:read', { title: 'Embedded title', artist: 'Artist', album: 'Album' }],
])
  ipcMain.handle(channel, (event, payload) => {
    if (event.senderFrame?.url !== indexUrl) throw new Error('Unexpected renderer URL');
    if (channel === 'config:save') savedConfig = payload;
    return response;
  });
ipcMain.handle('sidecar:read', (event, file) => {
  if (event.senderFrame?.url !== indexUrl) throw new Error('Unexpected renderer URL');
  return {
    extension: '.lrc',
    text: file.includes('plain')
      ? 'Untimed text that must stay out of the player'
      : file.includes('second')
        ? '[00:00.00]Other recording title\n[00:10.00]Next song'
        : '[00:00.00]First title\n[00:10.00]Second title',
  };
});
ipcMain.handle('files:registerDrop', (event, paths) => {
  if (event.senderFrame?.url !== indexUrl) throw new Error('Unexpected renderer URL');
  droppedPaths = paths;
});

app.whenReady().then(async () => {
  const window = new BrowserWindow({
    show: false,
    width: 1320,
    height: 820,
    webPreferences: {
      preload: path.join(__dirname, '..', 'src', 'preload.cjs'),
      contextIsolation: true,
      sandbox: true,
      backgroundThrottling: false,
    },
  });
  window.webContents.on('preload-error', (_event, preloadPath, error) => {
    process.stderr.write(`Preload error in ${preloadPath}: ${error.stack || error}\n`);
  });
  try {
    const mask = nativeImage.createFromPath(
      path.join(__dirname, '..', 'assets', 'icons', 'tray-mask.png'),
    );
    const appIcon = nativeImage.createFromPath(
      path.join(__dirname, '..', 'assets', 'icons', 'radio.ico'),
    );
    if (
      appIcon.isEmpty() ||
      mask.isEmpty() ||
      nativeImage.createFromBitmap(mask.toBitmap(), mask.getSize()).isEmpty()
    )
      throw new Error('Tray icon could not be loaded');
    await window.loadFile(indexPath);
    const result = await window.webContents.executeJavaScript(String.raw`(async () => {
      await Promise.all([
        document.fonts.load('400 12px "DM Mono"'),
        document.fonts.load('600 24px Oswald'),
      ]);
      return {
        bridge: typeof window.radio?.pathsForFiles === 'function',
        url: url('C:\\Music\\a #?.mp3'),
        unc: url('\\\\server\\share\\a#?.mp3'),
        name: new DOMParser().parseFromString(
          stationFormMarkup({ station: 'A" & <b>', crossfade: null }, ''), 'text/html'
        ).querySelector('#newName').value,
        lrc: parse({ extension: '.lrc', text: '\uFEFF[01:30.00][00:10.00]Chorus' }).timed,
        safeJson: parse({ extension: '.json', text: '{"timed":{}}' }).timed,
        fonts: document.fonts.check('400 12px "DM Mono"') && document.fonts.check('600 24px Oswald'),
        page: !!document.querySelector('#stationList'),
      };
    })()`);
    if (
      !result.bridge ||
      !result.fonts ||
      !result.page ||
      !result.url.includes('%23%3F') ||
      result.unc !== 'file://server/share/a%23%3F.mp3' ||
      result.name !== 'A" & <b>' ||
      result.lrc.length !== 2 ||
      result.lrc[0].at !== 10 ||
      result.lrc[1].at !== 90 ||
      result.safeJson.length !== 0
    )
      throw new Error(`Smoke test failed: ${JSON.stringify(result)}`);
    const editorState = await window.webContents.executeJavaScript(`(() => {
      const form = new DOMParser().parseFromString(stationFormMarkup(null, '', ''), 'text/html');
      return {
        useGlobalFade: form.querySelector('#useGlobalFade').checked,
        bannerColorOff: !form.querySelector('#useBannerColor').checked,
        bannerPicker: !!form.querySelector('#browseBanner'),
      };
    })()`);
    if (!editorState.useGlobalFade || !editorState.bannerColorOff || !editorState.bannerPicker)
      throw new Error(`Station editor defaults failed: ${JSON.stringify(editorState)}`);
    const fadeState = await window.webContents.executeJavaScript(String.raw`(() => {
      stationEditor(null, ['C:\\Music\\song.mp3']);
      const initiallyDisabled = document.querySelector('#stationFade').disabled;
      document.querySelector('#useGlobalFade').click();
      const enabledAfterUncheck = !document.querySelector('#stationFade').disabled;
      document.querySelector('#editor').close();
      return { initiallyDisabled, enabledAfterUncheck };
    })()`);
    if (!fadeState.initiallyDisabled || !fadeState.enabledAfterUncheck)
      throw new Error(`Crossfade option failed: ${JSON.stringify(fadeState)}`);
    const tuningState = await window.webContents.executeJavaScript(`(() => {
      document.querySelector('#trackAlbum').textContent = 'Old album';
      document.querySelector('#metadata').textContent = 'Old lyric';
      tune({ key: 'empty', station: 'Empty station', mode: 'tracks', tracks: [] });
      return {
        title: document.querySelector('#trackTitle').textContent,
        album: document.querySelector('#trackAlbum').textContent,
        lyric: document.querySelector('#metadata').textContent,
      };
    })()`);
    if (tuningState.title !== 'Tuning…' || tuningState.album || tuningState.lyric)
      throw new Error(`Tuning details were not cleared: ${JSON.stringify(tuningState)}`);
    const playbackState = await window.webContents.executeJavaScript(String.raw`(async () => {
      const tracks = [{ path: 'C:\\Music\\first.mp3', name: 'First recording' }, { path: 'C:\\Music\\second.mp3', name: 'Second recording' }];
      durations.set(tracks[0].path, 120); durations.set(tracks[1].path, 180);
      selected = { key: 'broadcast', station: 'Broadcast', mode: 'longset', tracks };
      await showTrackDetails(tracks[0], selected, token);
      const firstTitle = $('#trackTitle').textContent;
      await showTrackDetails(tracks[1], selected, token);
      const secondTitle = $('#trackTitle').textContent;
      const broadcastLyric = $('#metadata').textContent;
      const clock = await clockFor(selected, token);
      selected.textMode = 'lyrics'; metadata();
      const lyric = $('#metadata').textContent;
      selected.textMode = 'ignore';
      await showTrackDetails(tracks[1], selected, token);
      const ignored = $('#metadata').textContent;
      selected.textMode = 'titles';
      await showTrackDetails({ path: 'C:\\Music\\plain.mp3', name: 'Plain recording' }, selected, token);
      const plainTitle = $('#trackTitle').textContent;
      const plainLyric = $('#metadata').textContent;
      sidecar = parse({ extension: '.lrc', text: '[00:30.00]Future title' });
      metadata();
      const beforeFirst = $('#trackTitle').textContent;
      return { firstTitle, secondTitle, broadcastLyric, lyric, ignored, plainTitle, plainLyric, beforeFirst,
        order: clock.order.map(t => t.name), fade: clock.fade,
        invalidLrc: parse({ extension: '.lrc', text: '[00:99]Invalid\nJust notes' }).timed.length,
        invalidJson: parse({ extension: '.json', text: JSON.stringify({timed: [{at: -1, text: 'bad'}, {at: '2', text: 'bad'}]}) }).timed.length,
      };
    })()`);
    if (
      playbackState.firstTitle !== 'First title' ||
      playbackState.secondTitle !== 'Other recording title' ||
      playbackState.broadcastLyric ||
      playbackState.lyric !== 'Other recording title' ||
      playbackState.ignored ||
      playbackState.plainTitle !== 'Artist - Embedded title' ||
      playbackState.plainLyric ||
      playbackState.beforeFirst !== 'Artist - Embedded title' ||
      playbackState.fade !== 0 ||
      playbackState.order.join(',') !== 'First recording,Second recording' ||
      playbackState.invalidLrc ||
      playbackState.invalidJson
    )
      throw new Error(`Broadcast metadata failed: ${JSON.stringify(playbackState)}`);
    const creationState = await window.webContents.executeJavaScript(String.raw`(async () => {
      selected = null; custom = []; stations = []; groups = []; logos = {};
      const files = ['C:\\Music\\first.mp3', 'C:\\Music\\second.mp3'];
      stationEditor(null, files);
      $('#newName').value = 'Combined broadcast';
      document.querySelector('[data-preset="longset"]').click();
      $('#editorSave').click();
      const combined = custom[0];
      stationEditor(combined);
      const editGroupingAbsent = !$('#newGrouping');
      $('#textMode').value = 'lyrics';
      $('#useStationAccent').checked = true; $('#stationAccent').value = '#a83657';
      $('#editorSave').click();
      const edited = custom[0];
      stationEditor(null, [...files]);
      $('#newGrouping').value = 'separate'; $('#newGrouping').dispatchEvent(new Event('change'));
      const hidesIdentity = $('#stationIdentity').hidden && $('#stationAppearance').hidden;
      $('#editorSave').click();
      const split = custom.slice(1).map(s => ({ name: s.station, count: s.tracks.length, mode: s.mode }));
      await save();
      const stored = JSON.parse(JSON.stringify({ custom, settings, groups, logos, hidden, root }));
      sanitizeConfig(stored);
      return { combinedCount: combined.tracks.length, combinedMode: combined.mode, editGroupingAbsent, hidesIdentity,
        sameKey: edited.key === combined.key, textMode: custom[0].textMode, accent: custom[0].accent, split };
    })()`);
    if (
      creationState.combinedCount !== 2 ||
      creationState.combinedMode !== 'longset' ||
      !creationState.editGroupingAbsent ||
      !creationState.hidesIdentity ||
      !creationState.sameKey ||
      creationState.textMode !== 'lyrics' ||
      creationState.accent !== '#a83657' ||
      creationState.split.length !== 2 ||
      creationState.split[0].name !== 'first' ||
      creationState.split.some((s) => s.count !== 1 || s.mode !== 'tracks') ||
      savedConfig.custom.length !== 3
    )
      throw new Error(`Station creation failed: ${JSON.stringify(creationState)}`);
    const controlsState = await window.webContents.executeJavaScript(`(() => {
      $('#playerView').hidden = false;
      $('#volume').value = 65; $('#volume').dispatchEvent(new Event('input'));
      $('#mute').click();
      const allMuted = audio.muted && spare.muted && staticAudio.muted;
      $('#mute').click();
      document.body.dispatchEvent(new KeyboardEvent('keydown', { key: '+', bubbles: true }));
      const volume = Number($('#volume').value);
      const lit = document.querySelectorAll('.volume-segments i.lit').length;
      $('.volume-module').dispatchEvent(new WheelEvent('wheel', { deltaY: 60, bubbles: true, cancelable: true }));
      const wheelVolume = Number($('#volume').value);
      const noTune = stepTarget === null;
      $('#stationSearch').value = 'combined'; render();
      const searchCount = document.querySelectorAll('.station').length;
      $('#stationSearch').value = ''; render();
      $('#openSettings').click();
      document.querySelector('[data-settings-tab="appearance"]').click();
      $('#showScanlines').click(); $('#reduceMotion').click();
      $('#listDensity').value = 'compact'; $('#listDensity').dispatchEvent(new Event('change'));
      const appearance = document.documentElement.classList.contains('no-scanlines') && document.documentElement.classList.contains('reduce-motion') && document.documentElement.dataset.density === 'compact';
      const panel = !document.querySelector('[data-settings-panel="appearance"]').hidden && document.querySelector('[data-settings-panel="playback"]').hidden;
      $('#settingsBack').click();
      settings.showScanlines = true; settings.reduceMotion = false; settings.listDensity = 'comfortable'; applySettings();
      manageStations();
      const disabledDelete = $('#editorSave').disabled;
      document.querySelector('.bulk-station').click();
      const deleteLabel = $('#editorSave').textContent;
      $('#editor').close();
      return { allMuted, unmuted: !audio.muted && !spare.muted, volume, lit, wheelVolume, noTune, searchCount, appearance, panel, disabledDelete, deleteLabel };
    })()`);
    if (
      !controlsState.allMuted ||
      !controlsState.unmuted ||
      controlsState.volume !== 70 ||
      controlsState.lit !== 8 ||
      controlsState.wheelVolume !== 65 ||
      !controlsState.noTune ||
      controlsState.searchCount !== 1 ||
      !controlsState.appearance ||
      !controlsState.panel ||
      !controlsState.disabledDelete ||
      controlsState.deleteLabel !== 'Delete 1 station'
    )
      throw new Error(`UI controls failed: ${JSON.stringify(controlsState)}`);
    // Use Chromium's native file drag data; JS-created Files have no disk paths.
    window.webContents.debugger.attach('1.3');
    const dragFiles = async (files, selector) => {
      const point = await window.webContents.executeJavaScript(`(() => {
        const target = document.querySelector(${JSON.stringify(selector)});
        target.scrollIntoView({block: 'center'});
        const rect = target.getBoundingClientRect();
        return {x: rect.left + rect.width / 2, y: rect.top + rect.height / 2};
      })()`);
      for (const type of ['dragEnter', 'dragOver', 'drop'])
        await window.webContents.debugger.sendCommand('Input.dispatchDragEvent', {
          type,
          ...point,
          data: { items: [], files, dragOperationsMask: 1 },
        });
      await new Promise((resolve) => setTimeout(resolve, 150));
    };
    const audioPath = path.join(__dirname, '..', 'assets', 'audio', 'static.mp3');
    await dragFiles([audioPath], '#import');
    const dropState = await window.webContents.executeJavaScript(`({
      open: $('#editor').open, paths: importPaths, error: $('#libraryPath').textContent
    })`);
    if (
      !dropState.open ||
      !dropState.paths.includes(audioPath) ||
      !droppedPaths.includes(audioPath)
    )
      throw new Error(`Native file drop failed: ${JSON.stringify(dropState)}`);
    const otherAudioPath = path.join(__dirname, '..', 'static.mp3');
    const unsupportedPath = path.join(__dirname, '..', 'README.md');
    await dragFiles([otherAudioPath, unsupportedPath], '#editorTitle');
    const appendState = await window.webContents.executeJavaScript(`({
      paths: importPaths, error: $('#editorError').textContent,
      highlighted: document.documentElement.classList.contains('file-drag-active')
    })`);
    if (
      appendState.paths.length !== 2 ||
      !appendState.paths.includes(otherAudioPath) ||
      appendState.error ||
      appendState.highlighted ||
      droppedPaths.length !== 1 ||
      droppedPaths[0] !== otherAudioPath
    )
      throw new Error(`Drop into station editor failed: ${JSON.stringify(appendState)}`);
    await dragFiles([otherAudioPath], '#fileList');
    await dragFiles([unsupportedPath], '#editorSave');
    const filteredState = await window.webContents.executeJavaScript(`({
      count: importPaths.length, open: $('#editor').open, error: $('#editorError').textContent
    })`);
    if (
      filteredState.count !== 2 ||
      !filteredState.open ||
      !filteredState.error.includes('Drop audio files')
    )
      throw new Error(`Duplicate or unsupported drop failed: ${JSON.stringify(filteredState)}`);
    await window.webContents.executeJavaScript(`$('#editor').close()`);
    // Exercise the actual hover selector with Chromium input, not a CSS approximation.
    const playPoint = await window.webContents.executeJavaScript(`(() => {
      $('#homeView').hidden = true; $('#playerView').hidden = false;
      $('#play').style.transition = 'none';
      $('#play').scrollIntoView({block: 'center'});
      const rect = $('#play').getBoundingClientRect();
      return {x: rect.left + rect.width / 2, y: rect.top + rect.height / 2};
    })()`);
    await window.webContents.debugger.sendCommand('Input.dispatchMouseEvent', {
      type: 'mouseMoved',
      ...playPoint,
    });
    await new Promise((resolve) => setTimeout(resolve, 150));
    const hoverState = await window.webContents.executeJavaScript(`(() => {
      const play = $('#play'); const style = getComputedStyle(play);
      return {hovered: play.matches(':hover'), color: style.color, background: style.backgroundColor};
    })()`);
    if (
      !hoverState.hovered ||
      hoverState.color === hoverState.background ||
      hoverState.background !== 'rgb(20, 32, 20)'
    )
      throw new Error(`Play hover contrast failed: ${JSON.stringify(hoverState)}`);
    window.webContents.debugger.detach();
    const foldersState = await window.webContents.executeJavaScript(`(() => {
      const paths = Array.from({length: 40}, (_, i) => 'C:/Music/' + (i < 25 ? 'Album A/' : 'Album B/Live/') + 'recording-' + i + '.mp3');
      stationEditor({ key: 'folders', station: 'Folders', mode: 'longset', tracks: paths.map(path => ({path, name: path})) });
      const list = $('#fileList');
      const folders = [...list.querySelectorAll('[data-folder]')].map(folder => folder.dataset.folder);
      const scrolls = list.scrollHeight > list.clientHeight && getComputedStyle(list).overflowY === 'auto';
      const albumA = list.querySelector('[data-folder="C:/Music/Album A"]');
      albumA.querySelector('summary').click();
      const collapsed = !albumA.open && !albumA.querySelector('.file-row').checkVisibility();
      list.querySelector('[data-down="25"]').click();
      const preservesOrder = importPaths[25] === paths[26] && importPaths[26] === paths[25];
      list.querySelector('[data-remove="26"]').click();
      const preservedCollapse = !list.querySelector('[data-folder="C:/Music/Album A"]').open;
      const removedRightFile = importPaths.length === 39 && !importPaths.includes(paths[25]);
      const lastNumber = list.querySelector('[data-up="38"]').closest('.file-row').querySelector('.file-number').textContent;
      const remaining = [...importPaths];
      list.querySelector('[data-folder-first="C:/Music/Album B"]').click();
      const expected = [...remaining.filter(p => p.includes('/Album B/')), ...remaining.filter(p => !p.includes('/Album B/'))];
      const folderFirst = JSON.stringify(importPaths) === JSON.stringify(expected);
      const stillCollapsed = !list.querySelector('[data-folder="C:/Music/Album A"]').open;
      const alreadyFirstDisabled = list.querySelector('[data-folder-first="C:/Music/Album B"]').disabled;
      $('#useStationAccent').checked = true; $('#stationAccent').value = '#a83657';
      $('#useStationAccent').dispatchEvent(new Event('change'));
      const previewMatchesPlayer = getComputedStyle($('#previewName')).color === getComputedStyle($('#stationName')).color;
      $('#editorSave').click();
      const savedOrder = JSON.stringify(custom.find(s => s.key === 'folders').tracks.map(t => t.path)) === JSON.stringify(expected);
      return {folders, scrolls, collapsed, preservesOrder, preservedCollapse, removedRightFile, lastNumber, folderFirst, stillCollapsed, alreadyFirstDisabled, previewMatchesPlayer, savedOrder};
    })()`);
    if (
      foldersState.folders.join('|') !==
        'C:/Music|C:/Music/Album A|C:/Music/Album B|C:/Music/Album B/Live' ||
      !foldersState.scrolls ||
      !foldersState.collapsed ||
      !foldersState.preservesOrder ||
      !foldersState.preservedCollapse ||
      !foldersState.removedRightFile ||
      foldersState.lastNumber !== '39' ||
      !foldersState.folderFirst ||
      !foldersState.stillCollapsed ||
      !foldersState.alreadyFirstDisabled ||
      !foldersState.previewMatchesPlayer ||
      !foldersState.savedOrder
    )
      throw new Error(`Scrollable folder tree failed: ${JSON.stringify(foldersState)}`);
    const reorderState = await window.webContents.executeJavaScript(`(() => {
      const paths = ['C:/Music/A/1.mp3', 'C:/Music/A/2.mp3', 'C:/Music/B/Live/3.mp3', 'C:/Music/B/Live/4.mp3', 'C:/Music/C/5.mp3'];
      stationEditor({key: 'reorder', station: 'Reorder', mode: 'longset', tracks: paths.map(path => ({path, name: path}))});
      const initialBoundaries = document.querySelector('[data-folder-up="C:/Music/A"]').disabled && document.querySelector('[data-folder-down="C:/Music/C"]').disabled;
      document.querySelector('[data-folder-down="C:/Music/B"]').click();
      const down = JSON.stringify(importPaths) === JSON.stringify([...paths.slice(0, 2), paths[4], ...paths.slice(2, 4)]);
      document.querySelector('[data-folder-up="C:/Music/B"]').click();
      const back = JSON.stringify(importPaths) === JSON.stringify(paths);
      document.querySelector('[data-folder-up="C:/Music/B"]').click();
      const up = JSON.stringify(importPaths) === JSON.stringify([...paths.slice(2, 4), ...paths.slice(0, 2), paths[4]]);
      $('#editor').close();
      return {initialBoundaries, down, back, up};
    })()`);
    if (Object.values(reorderState).some((value) => !value))
      throw new Error('Folder reorder failed: ' + JSON.stringify(reorderState));
    const mediaState = await window.webContents.executeJavaScript(`(() => {
      selected = stations[0]; groupId = 'all';
      const event = new KeyboardEvent('keydown', {key: 'MediaTrackNext', bubbles: true, cancelable: true});
      document.body.dispatchEvent(event);
      const next = stepTarget === stations[1] && event.defaultPrevented;
      clearTimeout(stepTimer); stepTarget = null;
      document.body.dispatchEvent(new KeyboardEvent('keydown', {key: 'MediaTrackPrevious', bubbles: true, cancelable: true}));
      const previous = stepTarget === stations.at(-1);
      clearTimeout(stepTimer); stepTarget = null;
      stationMediaActions.nexttrack();
      const systemNext = stepTarget === stations[1];
      clearTimeout(stepTimer); stepTarget = null;
      stationMediaActions.previoustrack();
      const systemPrevious = stepTarget === stations.at(-1);
      clearTimeout(stepTimer); stepTarget = null; selected = null;
      return {next, previous, systemNext, systemPrevious};
    })()`);
    if (Object.values(mediaState).some((value) => !value))
      throw new Error('Media station navigation failed: ' + JSON.stringify(mediaState));
    const playMediaState = await window.webContents.executeJavaScript(`(async () => {
      audio.src = url(${JSON.stringify(audioPath)}); audio.loop = true; audio.muted = true;
      await audio.play();
      document.body.dispatchEvent(new KeyboardEvent('keydown', {key: 'MediaPlayPause', bubbles: true, cancelable: true}));
      const paused = audio.paused;
      document.body.dispatchEvent(new KeyboardEvent('keydown', {key: 'MediaPlayPause', bubbles: true, cancelable: true}));
      await audio.play();
      const resumed = !audio.paused;
      stationMediaActions.pause(); stationMediaActions.pause();
      const systemPause = audio.paused;
      stationMediaActions.play(); await audio.play(); stationMediaActions.play();
      const systemPlay = !audio.paused;
      audio.pause(); audio.loop = false; audio.removeAttribute('src'); audio.load(); updateVolume();
      return {paused, resumed, systemPause, systemPlay};
    })()`);
    if (Object.values(playMediaState).some((value) => !value))
      throw new Error('Media play/pause failed: ' + JSON.stringify(playMediaState));
    window.setSize(980, 620);
    const layoutState = await window.webContents.executeJavaScript(`(() => {
      stationEditor(null, Array.from({length: 40}, (_, i) => 'C:/Music/recording-' + i + '.mp3'));
      const body = $('#editorBody');
      body.scrollTop = body.scrollHeight;
      const save = $('#editorSave').getBoundingClientRect();
      const dialog = $('#editor').getBoundingClientRect();
      const scrolls = body.scrollHeight > body.clientHeight;
      const actionsVisible = save.bottom <= innerHeight && save.top >= 0 && save.bottom <= dialog.bottom;
      $('#editor').close();
      return { scrolls, actionsVisible };
    })()`);
    if (!layoutState.scrolls || !layoutState.actionsVisible)
      throw new Error(`Dialog layout failed: ${JSON.stringify(layoutState)}`);
    window.setSize(1320, 820);
    if (process.env.RADIO_REPLAY_CAPTURE === '1') {
      await window.webContents.executeJavaScript(String.raw`(() => {
        const station = { key: 'preview', station: 'MIDNIGHT FM', mode: 'tracks',
          tracks: [{ path: 'C:\\Music\\song.mp3', name: 'Song' }],
          bannerColor: '#a83657', bannerPath: '', spinLogo: false };
        stations = [station]; selected = station;
        document.querySelector('#homeView').hidden = true;
        document.querySelector('#playerView').hidden = false;
        document.querySelector('#frequency').textContent = '01';
        document.querySelector('#stationName').textContent = station.station;
        document.querySelector('#trackTitle').textContent = 'Now Playing Tonight';
        document.querySelector('#trackAlbum').textContent = 'Sample Album';
        document.querySelector('#metadata').textContent = 'Sample lyric line';
        settings.matchBannerAccent = false;
        document.querySelector('#stationSub').textContent = '12 TRACKS · SHUFFLE PROGRAM';
        document.querySelector('#recordingProgress').value = 0.28;
        document.querySelector('#time').textContent = '16:53 / 59:59';
        applyStationVisuals(station);
      })()`);
      await new Promise((resolve) => setTimeout(resolve, 750));
      const picture = await window.webContents.capturePage();
      const captureDir = path.join(__dirname, '..', 'out', 'ui-review');
      await fs.mkdir(captureDir, { recursive: true });
      await fs.writeFile(path.join(captureDir, 'player.png'), picture.toPNG());
      await window.webContents.executeJavaScript(`stationEditor(selected)`);
      await new Promise((resolve) => setTimeout(resolve, 200));
      await fs.writeFile(
        path.join(captureDir, 'editor.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      await window.webContents.executeJavaScript(`(() => {
        $('#editor').close();
        const paths = Array.from({length: 36}, (_, i) => 'C:/Music/' + (i < 24 ? 'Night Drive/' : 'Live Sessions/2026/') + 'Recording ' + (i + 1) + '.mp3');
        stationEditor({ key: 'tree-preview', station: 'MIDNIGHT FM', mode: 'longset', tracks: paths.map(path => ({path, name: path})) });
        $('#fileList [data-folder="C:/Music/Night Drive"]').open = false;
        $('#fileList').scrollIntoView({block: 'center'});
      })()`);
      // A hidden window can retain its previous compositor frame until capture requests a repaint.
      await window.webContents.capturePage();
      await new Promise((resolve) => setTimeout(resolve, 200));
      await fs.writeFile(
        path.join(captureDir, 'track-folders.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      await window.webContents.executeJavaScript(
        `$('#editor').close(); $('#openSettings').click(); document.querySelector('[data-settings-tab="appearance"]').click()`,
      );
      await new Promise((resolve) => setTimeout(resolve, 200));
      await fs.writeFile(
        path.join(captureDir, 'settings.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      await window.webContents.executeJavaScript(
        `$('#settingsBack').click(); stations = Array.from({length:112}, (_, i) => ({key: 'station-' + i, station: 'Station ' + (i + 1), tracks: [], mode: 'longset'})); render(); manageStations();`,
      );
      await new Promise((resolve) => setTimeout(resolve, 200));
      await fs.writeFile(
        path.join(captureDir, 'manage.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
    }
    process.stdout.write(`Smoke test passed: ${JSON.stringify(result)}\n`);
  } catch (error) {
    process.stderr.write(`${error.stack || error}\n`);
    app.exit(1);
    return;
  } finally {
    window.destroy();
    app.quit();
  }
});
