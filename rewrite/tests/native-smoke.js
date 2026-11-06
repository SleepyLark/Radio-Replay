// Injected only by the explicit native-smoke Cargo feature and isolated profile.
void (async () => {
  const invoke = (command, args = {}) => window.__TAURI_INTERNALS__.invoke(command, args);
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const until = async (test, message) => {
    for (let i = 0; i < 200; i++) {
      if (test()) return;
      await wait(50);
    }
    throw new Error(message);
  };
  const assert = (value, message) => {
    if (!value) throw new Error(message);
  };
  try {
    if (new URLSearchParams(location.search).get('mini') === '1') {
      await until(
        () => document.querySelector('.mini-titles strong')?.textContent === 'Native A',
        'Mini snapshot did not arrive',
      );
      let denied = false;
      try {
        await invoke('save_settings', { patch: { accent: '#123456' } });
      } catch {
        denied = true;
      }
      assert(denied, 'Mini window could mutate library settings');
      document.querySelector('[aria-label="Pause"]').click();
      await until(
        () => !!document.querySelector('[aria-label="Play"]'),
        'Mini pause did not reach the main engine',
      );
      const volume = document.querySelector('input[type="range"]');
      volume.value = '.25';
      volume.dispatchEvent(new Event('input', { bubbles: true }));
      document.querySelector('[aria-label="Next station"]').click();
      await until(
        () => document.querySelector('.mini-titles strong')?.textContent === 'Native B',
        'Mini next did not retune',
      );
      await until(
        () => !!document.querySelector('[aria-label="Pause"]'),
        'Retuned station did not play',
      );
      await invoke('native_smoke_report', {
        phase: 'mini',
        ok: true,
        message: 'Snapshot, pause, volume, next station and playback verified',
      });
      document.querySelector('[aria-label="Restore main window"]').click();
    } else {
      await until(
        () => document.querySelectorAll('.station').length === 2,
        'Fixture library did not load',
      );
      Math.random = () => 0;
      assert(!document.querySelector('.compact-player'), 'Bar visible before tuning');
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'm', bubbles: true }));
      document.querySelector('.station').click();
      await until(
        () => !!document.querySelector('[aria-label="Pause"]'),
        'WebView2 audio playback failed',
      );
      assert(!document.querySelector('.compact-player'), 'Bar visible on player screen');
      document.querySelector('.back-stations').click();
      await until(() => !!document.querySelector('.compact-player'), 'Bar missing on All Stations');
      document.querySelector('.compact-title').click();
      await until(() => !!document.querySelector('.radio-screen'), 'Bar did not reopen player');
      assert(!document.querySelector('.compact-player'), 'Bar remained on reopened player');
      await until(
        () => !!document.querySelector('.lyrics .current'),
        'Native sidecar did not load',
      );
      await until(
        () =>
          ['#dialBanner', '#logo'].every(
            (selector) => document.querySelector(selector)?.naturalWidth > 0,
          ),
        'Saved banner or logo asset was not granted',
      );
      await document.fonts.ready;
      assert(
        document.fonts.check('600 24px Oswald') && document.fonts.check('12px "DM Mono"'),
        'Bundled radio fonts did not load',
      );
      document.querySelector('.edit-link').click();
      await until(() => document.querySelector('dialog')?.open, 'Native editor did not open');
      document.querySelector('dialog button[type="submit"]').click();
      await until(() => !document.querySelector('dialog'), 'Native station save did not complete');
      assert(document.querySelector('[aria-label="Pause"]'), 'Visual save stopped native audio');
      await invoke('native_smoke_report', {
        phase: 'drop',
        ok: true,
        message: 'Testing native drop grants and editor flow',
      });
      await until(() => document.querySelector('dialog')?.open, 'Native drop did not open editor');
      await until(
        () => document.querySelectorAll('.file-row').length === 2,
        'Dropped native audio files missing',
      );
      const droppedName = document.querySelector('dialog input');
      droppedName.value = 'Native Dropped';
      droppedName.dispatchEvent(new Event('input', { bubbles: true }));
      await wait(30);
      document.querySelector('dialog button[type="submit"]').click();
      await until(
        () => !document.querySelector('dialog'),
        'Native drop grants did not allow saving',
      );
      assert(
        document.querySelector('[aria-label="Pause"]'),
        'Saving another station stopped playback',
      );
      document.querySelector('.choose.secondary').click();
      await until(() => document.querySelector('dialog')?.open, 'Native station manager failed');
      const row = [...document.querySelectorAll('.bulk-row')].find((e) =>
        e.textContent.includes('Native Dropped'),
      );
      row.querySelector('input').click();
      await wait(30);
      document.querySelector('dialog .danger').click();
      await wait(30);
      document.querySelector('dialog .danger').click();
      await until(() => !document.querySelector('dialog'), 'Native bulk removal failed');
      assert(
        document.querySelector('[aria-label="Pause"]'),
        'Removing another station stopped playback',
      );
      await wait(300);
      const snapshot = await invoke('desktop_snapshot');
      assert(
        snapshot.hasStation && snapshot.duration > 0 && snapshot.seconds >= 0,
        'Desktop state missing playback',
      );
      await invoke('native_smoke_report', {
        phase: 'playback',
        ok: true,
        message: 'Local asset playback, sidecar, metadata and desktop snapshot verified',
      });
      await invoke('native_smoke_report', {
        phase: 'closeToTray',
        ok: true,
        message: 'Testing main-window close without stopping playback',
      });
      await wait(200);
      assert((await invoke('desktop_snapshot')).playing, 'Closing to tray stopped the player');
      [...document.querySelectorAll('button')]
        .find((button) => button.textContent.trim().endsWith('Mini player'))
        .click();
      await until(
        () => document.querySelector('.now-playing strong')?.textContent === 'Native B',
        'Main player did not receive mini next',
      );
      await wait(800);
      const final = await invoke('desktop_snapshot');
      assert(Math.abs(final.volume - 0.25) < 0.001, 'Mini volume did not persist');
      await invoke('native_smoke_report', {
        phase: 'complete',
        ok: true,
        message: 'Native desktop integration passed',
      });
    }
  } catch (error) {
    await invoke('native_smoke_report', {
      phase: 'error',
      ok: false,
      message: String(error) + ' | ' + document.body.innerText.slice(-2000),
    });
  }
})();
