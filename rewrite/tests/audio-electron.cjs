// Real Chromium audio smoke test; the application remains Tauri-only.
const { app, BrowserWindow } = require('electron');
const { buildSync } = require('esbuild');
const path = require('node:path');
const profile = path.join(__dirname, '..', 'out', 'audio-smoke-profile');
require('node:fs').mkdirSync(profile, { recursive: true });
app.setPath('userData', profile);
app.commandLine.appendSwitch('autoplay-policy', 'no-user-gesture-required');
const timer = setTimeout(() => {
  console.error('Audio smoke timed out');
  app.exit(1);
}, 30000);
app
  .whenReady()
  .then(async () => {
    const window = new BrowserWindow({
      show: false,
      webPreferences: {
        nodeIntegration: false,
        contextIsolation: true,
        backgroundThrottling: false,
      },
    });
    const bundle = buildSync({
      entryPoints: [path.join(__dirname, 'audio-browser.mjs')],
      bundle: true,
      write: false,
      format: 'iife',
      platform: 'browser',
    }).outputFiles[0].text;
    await window.loadURL('data:text/html,<title>Radio Replay audio verification</title>');
    try {
      const result = await window.webContents.executeJavaScript(
        bundle + '\nwindow.audioSmoke();',
        true,
      );
      console.log(result);
      clearTimeout(timer);
      window.destroy();
      app.exit(0);
    } catch (error) {
      console.error(error);
      clearTimeout(timer);
      window.destroy();
      app.exit(1);
    }
  })
  .catch((error) => {
    console.error(error);
    app.exit(1);
  });
