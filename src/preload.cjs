const { contextBridge, ipcRenderer, webUtils } = require('electron');
// Keep Node and filesystem access in the main process. The renderer gets specific operations only.
contextBridge.exposeInMainWorld('radio', {
  chooseFiles: () => ipcRenderer.invoke('files:choose'),
  chooseAudioFolder: () => ipcRenderer.invoke('folder:audio'),
  chooseImage: () => ipcRenderer.invoke('image:choose'),
  chooseBanner: () => ipcRenderer.invoke('banner:choose'),
  setAppAccent: (color) => ipcRenderer.send('app:accent', color),
  loadConfig: () => ipcRenderer.invoke('config:load'),
  saveConfig: (config) => ipcRenderer.invoke('config:save', config),
  showConfig: () => ipcRenderer.invoke('config:show'),
  bundledStatic: () => ipcRenderer.invoke('static:bundled'),
  // The renderer must supply an Array<File>; a DOM FileList loses entries across the bridge.
  pathsForFiles: async (files) => {
    const paths = Array.from(files, (file) => webUtils.getPathForFile(file)).filter((filePath) =>
      /\.(mp3|m4a|aac|ogg|wav|flac)$/i.test(filePath),
    );
    await ipcRenderer.invoke('files:registerDrop', paths);
    return paths;
  },
  scanLibrary: (root) => ipcRenderer.invoke('library:scan', root),
  readSidecar: (audioPath) => ipcRenderer.invoke('sidecar:read', audioPath),
  readTags: (audioPath) => ipcRenderer.invoke('tags:read', audioPath),
});
