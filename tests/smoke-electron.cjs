const { app, BrowserWindow, ipcMain, nativeImage } = require('electron');
const path = require('node:path');
const fs = require('node:fs/promises');
const { pathToFileURL } = require('node:url');
const indexPath = path.join(__dirname, '..', 'src', 'renderer', 'index.html');
const indexUrl = pathToFileURL(indexPath).href;

for (const [channel, response] of [
  ['static:bundled', path.join(__dirname, '..', 'assets', 'audio', 'static.mp3')],
  ['config:load', null],
  ['config:save', 'test-config.json'],
])
  ipcMain.handle(channel, (event) => {
    if (event.senderFrame?.url !== indexUrl) throw new Error('Unexpected renderer URL');
    return response;
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
        settings.matchBannerAccent = true;
        applyStationVisuals(station);
      })()`);
      await new Promise((resolve) => setTimeout(resolve, 150));
      const picture = await window.webContents.capturePage();
      await fs.writeFile(path.join(__dirname, 'player-preview.png'), picture.toPNG());
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
