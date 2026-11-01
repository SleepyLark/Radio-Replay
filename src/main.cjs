const { app, BrowserWindow, dialog, ipcMain, Menu, nativeImage, shell, Tray } = require('electron');
const fs = require('node:fs/promises');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { createHash } = require('node:crypto');
const { DurationCache } = require('./duration-cache.cjs');
let durationCache;
let scannedAudio = [];
let durationReferences = '';
const cache = () =>
  (durationCache ||= new DurationCache(
    path.join(app.getPath('userData'), 'duration-cache.sqlite'),
  ));

let window;
let tray;
const iconResource = (name) =>
  path.join(app.isPackaged ? process.resourcesPath : app.getAppPath(), 'assets', 'icons', name);

function showWindow() {
  if (!window || window.isDestroyed()) return;
  if (window.isMinimized()) window.restore();
  window.show();
  window.focus();
}

function tintedIcon(color) {
  const source = nativeImage.createFromPath(iconResource('tray-mask.png'));
  if (source.isEmpty()) return nativeImage.createFromPath(iconResource('radio.png'));
  const { width, height } = source.getSize();
  const bitmap = Buffer.from(source.toBitmap());
  const red = parseInt(color.slice(1, 3), 16);
  const green = parseInt(color.slice(3, 5), 16);
  const blue = parseInt(color.slice(5, 7), 16);
  for (let index = 0; index < bitmap.length; index += 4) {
    const alpha = bitmap[index + 3] / 255;
    bitmap[index] = Math.round(blue * alpha);
    bitmap[index + 1] = Math.round(green * alpha);
    bitmap[index + 2] = Math.round(red * alpha);
  }
  return nativeImage.createFromBitmap(bitmap, { width, height });
}

function applyAppIcon(color) {
  const image = tintedIcon(color);
  if (tray) tray.setImage(image);
  if (window && !window.isDestroyed() && typeof window.setIcon === 'function')
    window.setIcon(image);
}

function createTray() {
  tray = new Tray(tintedIcon('#c7f464'));
  tray.setToolTip('Radio Replay');
  tray.setContextMenu(
    Menu.buildFromTemplate([
      { label: 'Open Radio Replay', click: showWindow },
      { type: 'separator' },
      { label: 'Quit', click: () => app.quit() },
    ]),
  );
  tray.on('click', showWindow);
}
const AUDIO = new Set(['.mp3', '.m4a', '.aac', '.ogg', '.wav', '.flac']);
const SIDEcars = new Set(['.lrc', '.txt', '.json']);
const allowedAudio = new Set();
let libraryRoot = '';
function grantAudio(paths) {
  for (const file of paths) if (audioFile(file)) allowedAudio.add(path.normalize(file));
}
function requireGrantedAudio(file) {
  if (!audioFile(file) || !allowedAudio.has(path.normalize(file)))
    throw new Error('Audio file was not selected or loaded as part of the library');
}
const indexUrl = pathToFileURL(path.join(__dirname, 'renderer', 'index.html')).href;
function handle(channel, callback) {
  ipcMain.handle(channel, (event, ...args) => {
    if (event.senderFrame?.url !== indexUrl)
      throw new Error('Request came from an unexpected page');
    return callback(event, ...args);
  });
}
function audioFile(value) {
  return (
    typeof value === 'string' &&
    path.isAbsolute(value) &&
    AUDIO.has(path.extname(value).toLowerCase())
  );
}
let lastMusicDirectory = '';
const musicDirectoryPath = () => path.join(app.getPath('userData'), 'last-music-directory.txt');

async function musicDirectory() {
  if (lastMusicDirectory) return lastMusicDirectory;
  try {
    lastMusicDirectory = await fs.readFile(musicDirectoryPath(), 'utf8');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  return lastMusicDirectory;
}

async function rememberMusicDirectory(folder) {
  lastMusicDirectory = folder;
  await fs.mkdir(path.dirname(musicDirectoryPath()), { recursive: true });
  await fs.writeFile(musicDirectoryPath(), folder, 'utf8');
}

function createWindow() {
  window = new BrowserWindow({
    width: 1320,
    height: 820,
    minWidth: 980,
    minHeight: 620,
    backgroundColor: '#0a1010',
    icon: iconResource('radio.png'),
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      sandbox: true,
      backgroundThrottling: false,
    },
  });
  // The player exposes its controls in the window, so omit Electron's default menu.
  window.removeMenu();
  if (!app.isPackaged)
    window.webContents.on('before-input-event', (_event, input) => {
      if (input.control && input.shift && input.key.toLowerCase() === 'i')
        window.webContents.toggleDevTools();
    });
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  window.webContents.on('will-navigate', (event) => event.preventDefault());
  window.on('closed', () => {
    window = null;
  });
  window.loadFile(path.join(__dirname, 'renderer', 'index.html'));
}

function imageAccent(imagePath) {
  const image = nativeImage.createFromPath(imagePath);
  if (image.isEmpty()) return '#c7f464';
  const pixels = image.resize({ width: 16, height: 16 }).toBitmap();
  let red = 0,
    green = 0,
    blue = 0,
    weight = 0;
  for (let i = 0; i < pixels.length; i += 4) {
    const alpha = pixels[i + 3] / 255;
    if (alpha < 0.25) continue;
    // Chromium's bitmap is BGRA. Give colorful pixels a little more influence.
    const r = pixels[i + 2],
      g = pixels[i + 1],
      b = pixels[i];
    const emphasis = alpha * (1 + (Math.max(r, g, b) - Math.min(r, g, b)) / 255);
    red += r * emphasis;
    green += g * emphasis;
    blue += b * emphasis;
    weight += emphasis;
  }
  if (!weight) return '#c7f464';
  const channel = (value) =>
    Math.max(45, Math.min(230, Math.round(value / weight)))
      .toString(16)
      .padStart(2, '0');
  return `#${channel(red)}${channel(green)}${channel(blue)}`;
}

async function walk(folder, isRoot = true) {
  // Folder imports and legacy scans share the same file filter. Never ingest Extras.
  let entries;
  try {
    entries = await fs.readdir(folder, { withFileTypes: true });
  } catch (error) {
    if (!isRoot && ['ENOENT', 'EACCES', 'EPERM'].includes(error.code)) return [];
    throw error;
  }
  const files = [];
  for (const entry of entries) {
    const full = path.join(folder, entry.name);
    if (entry.isDirectory()) {
      if (!/^extras?$/i.test(entry.name)) files.push(...(await walk(full, false)));
    } else if (AUDIO.has(path.extname(entry.name).toLowerCase())) files.push(full);
  }
  return files;
}

function stationFor(root, file) {
  // The old archive uses Game/Station.mp3 or Game/Station/track.mp3.
  const relative = path.relative(root, file);
  const parts = relative.split(path.sep);
  const game = parts.length > 1 ? parts[0] : 'Library';
  const lastFolder = parts.length > 2 ? parts[parts.length - 2] : null;
  const station = lastFolder || path.basename(file, path.extname(file));
  return { game, station, key: `${game}::${station}` };
}

handle('files:choose', async () => {
  const result = await dialog.showOpenDialog(window, {
    defaultPath: (await musicDirectory()) || undefined,
    properties: ['openFile', 'multiSelections'],
    filters: [{ name: 'Audio', extensions: ['mp3', 'm4a', 'aac', 'ogg', 'wav', 'flac'] }],
  });
  if (!result.canceled && result.filePaths.length) {
    grantAudio(result.filePaths);
    await rememberMusicDirectory(path.dirname(result.filePaths[0]));
  }
  return result.canceled ? [] : result.filePaths;
});
handle('image:choose', async () => {
  const result = await dialog.showOpenDialog(window, {
    properties: ['openFile'],
    filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }],
  });
  return result.canceled ? null : result.filePaths[0];
});
handle('banner:choose', async () => {
  const result = await dialog.showOpenDialog(window, {
    properties: ['openFile'],
    filters: [{ name: 'Banner image', extensions: ['png', 'jpg', 'jpeg', 'webp', 'gif'] }],
  });
  if (result.canceled) return null;
  const imagePath = result.filePaths[0];
  return { path: imagePath, accent: imageAccent(imagePath) };
});
ipcMain.on('app:accent', (event, color) => {
  if (
    event.sender !== window?.webContents ||
    typeof color !== 'string' ||
    !/^#[0-9a-f]{6}$/i.test(color)
  )
    return;
  applyAppIcon(color);
});
handle('library:scan', async (_, root) => {
  if (typeof root !== 'string' || !path.isAbsolute(root)) throw new Error('Invalid library path');
  if (!libraryRoot || path.normalize(root) !== path.normalize(libraryRoot))
    throw new Error('Library path was not loaded from settings');
  const files = await walk(root);
  scannedAudio = files;
  grantAudio(files);
  const grouped = new Map();
  for (const file of files) {
    const info = stationFor(root, file);
    if (!grouped.has(info.key)) grouped.set(info.key, { ...info, tracks: [] });
    grouped
      .get(info.key)
      .tracks.push({ path: file, name: path.basename(file, path.extname(file)) });
  }
  return [...grouped.values()]
    .map((s) => ({
      ...s,
      tracks: s.tracks.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true })),
    }))
    .sort((a, b) => a.game.localeCompare(b.game) || a.station.localeCompare(b.station));
});
handle('folder:audio', async () => {
  const result = await dialog.showOpenDialog(window, {
    defaultPath: (await musicDirectory()) || undefined,
    properties: ['openDirectory'],
  });
  if (result.canceled) return null;
  const folder = result.filePaths[0];
  await rememberMusicDirectory(folder);
  const files = await walk(folder);
  grantAudio(files);
  return { name: path.basename(folder), files };
});
handle('files:registerDrop', (_, paths) => {
  if (!Array.isArray(paths) || !paths.every(audioFile)) throw new Error('Invalid dropped files');
  grantAudio(paths);
});
const configPath = () => path.join(app.getPath('userData'), 'stations.json');
// Keep audio outside the app archive so Chromium can load it as a file in packaged builds.
handle('static:bundled', () =>
  path.join(
    app.isPackaged ? process.resourcesPath : app.getAppPath(),
    'assets',
    'audio',
    'static.mp3',
  ),
);
let configWrites = Promise.resolve();
let configLoadFailed = false;
let configLoaded = false;
handle('config:load', async () => {
  try {
    const config = JSON.parse(await fs.readFile(configPath(), 'utf8'));
    if (!config || typeof config !== 'object' || Array.isArray(config))
      throw new Error('Invalid stations.json structure');
    libraryRoot = typeof config.root === 'string' ? config.root : '';
    if (Array.isArray(config.custom))
      for (const station of config.custom)
        if (Array.isArray(station?.tracks)) grantAudio(station.tracks.map((track) => track?.path));
    configLoadFailed = false;
    configLoaded = true;
    return config;
  } catch (error) {
    if (error.code === 'ENOENT') return null;
    configLoadFailed = true;
    throw error;
  }
});
handle('config:save', (_, config) => {
  if (configLoadFailed) throw new Error('stations.json could not be loaded; fix it before saving.');
  if (
    !config ||
    typeof config !== 'object' ||
    Array.isArray(config) ||
    !Array.isArray(config.custom) ||
    !Array.isArray(config.groups) ||
    !Array.isArray(config.hidden) ||
    typeof config.root !== 'string'
  )
    throw new Error('Invalid station configuration');
  if (
    config.custom.some(
      (station) =>
        !station ||
        !Array.isArray(station.tracks) ||
        station.tracks.some(
          (track) =>
            !audioFile(track?.path) ||
            (configLoaded && !allowedAudio.has(path.normalize(track.path))),
        ),
    )
  )
    throw new Error('Station contains an audio file that was not selected');
  if (configLoaded && config.root !== libraryRoot)
    throw new Error('Library path cannot be changed by the renderer');
  if (!configLoaded)
    grantAudio(config.custom.flatMap((station) => station.tracks.map((track) => track.path)));
  if (!libraryRoot && config.root && path.isAbsolute(config.root)) libraryRoot = config.root;
  // Serialize writes because quick UI changes can otherwise race on the same temp file.
  configWrites = configWrites
    .catch(() => {})
    .then(async () => {
      const data = JSON.stringify(config, null, 2) + '\n';
      await fs.mkdir(path.dirname(configPath()), { recursive: true });
      try {
        await fs.copyFile(configPath(), configPath() + '.bak');
      } catch (error) {
        if (error.code !== 'ENOENT') throw error;
      }
      await fs.writeFile(configPath() + '.tmp', data, 'utf8');
      for (let attempt = 0; ; attempt++) {
        try {
          await fs.rename(configPath() + '.tmp', configPath());
          break;
        } catch (error) {
          if (attempt === 3 || !['EPERM', 'EACCES'].includes(error.code)) throw error;
          await new Promise((resolve) => setTimeout(resolve, 100 * (attempt + 1)));
        }
      }
      configLoaded = true;
      if (durationCache) {
        const referenced = function* () {
          yield* scannedAudio;
          for (const station of config.custom) for (const track of station.tracks) yield track.path;
        };
        const hash = createHash('sha256');
        for (const file of referenced()) hash.update(file).update('\0');
        const signature = hash.digest('hex');
        if (signature !== durationReferences) {
          durationReferences = signature;
          await durationCache.retain(referenced());
        }
      }
      return configPath();
    });
  return configWrites;
});
handle('config:show', () => shell.showItemInFolder(configPath()));
handle('duration:lookup', (_, audioPath) => {
  requireGrantedAudio(audioPath);
  return cache().lookup(audioPath);
});
handle('duration:remember', (_, audioPath, signature, duration) => {
  requireGrantedAudio(audioPath);
  return cache().remember(audioPath, signature, duration);
});
handle('duration:clear', () => {
  cache().clear();
  durationReferences = '';
  return true;
});
app.on('will-quit', () => durationCache?.close());
handle('sidecar:read', async (_, audioPath) => {
  requireGrantedAudio(audioPath);
  const ext = path.extname(audioPath);
  const base = audioPath.slice(0, audioPath.length - ext.length);
  for (const ext of SIDEcars) {
    try {
      return { extension: ext, text: await fs.readFile(base + ext, 'utf8') };
    } catch {
      /* try next */
    }
  }
  return null;
});
const tagCache = new Map();
handle('tags:read', (_, audioPath) => {
  requireGrantedAudio(audioPath);
  if (tagCache.has(audioPath)) {
    const entry = tagCache.get(audioPath);
    tagCache.delete(audioPath);
    tagCache.set(audioPath, entry);
    return entry;
  }
  if (!tagCache.has(audioPath)) {
    tagCache.set(
      audioPath,
      import('music-metadata')
        .then(({ parseFile }) => parseFile(audioPath, { skipCovers: true, duration: false }))
        .then(({ common }) => ({
          artist: common.artist || '',
          title: common.title || '',
          album: common.album || '',
        }))
        .catch(() => ({ artist: '', title: '', album: '' })),
    );
    if (tagCache.size > 512) tagCache.delete(tagCache.keys().next().value);
  }
  return tagCache.get(audioPath);
});
const hasSingleInstanceLock = app.requestSingleInstanceLock();
if (!hasSingleInstanceLock) app.quit();
else
  app.on('second-instance', () => {
    if (window) {
      if (window.isMinimized()) window.restore();
      window.focus();
    }
  });

app.whenReady().then(() => {
  if (!hasSingleInstanceLock) return;
  app.setAppUserModelId('com.local.radioreplay');
  createWindow();
  createTray();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
