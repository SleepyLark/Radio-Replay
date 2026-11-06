const { app, BrowserWindow } = require('electron');
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const output = path.join(__dirname, '..', 'out');
fs.mkdirSync(output, { recursive: true });
app.setPath('userData', path.join(output, 'ui-profile'));
app.commandLine.appendSwitch('autoplay-policy', 'no-user-gesture-required');
const timeout = setTimeout(() => {
  console.error('UI smoke timed out');
  app.exit(1);
}, 30000);
const dist = path.resolve(__dirname, '..', 'dist');
const samples = 30 * 8000;
const wav = Buffer.alloc(44 + samples * 2);
wav.write('RIFF', 0);
wav.writeUInt32LE(wav.length - 8, 4);
wav.write('WAVEfmt ', 8);
wav.writeUInt32LE(16, 16);
wav.writeUInt16LE(1, 20);
wav.writeUInt16LE(1, 22);
wav.writeUInt32LE(8000, 24);
wav.writeUInt32LE(16000, 28);
wav.writeUInt16LE(2, 32);
wav.writeUInt16LE(16, 34);
wav.write('data', 36);
wav.writeUInt32LE(samples * 2, 40);
const server = http.createServer((req, res) => {
  if (req.url === '/fixture.wav') {
    res.setHeader('Content-Type', 'audio/wav');
    res.end(wav);
    return;
  }
  const target = path.resolve(
    dist,
    '.' + decodeURIComponent(req.url === '/' ? '/index.html' : req.url.split('?')[0]),
  );
  if (!target.startsWith(dist + path.sep)) {
    res.writeHead(403);
    res.end();
    return;
  }
  fs.readFile(target, (error, bytes) => {
    if (error) {
      res.writeHead(404);
      res.end();
      return;
    }
    res.setHeader(
      'Content-Type',
      target.endsWith('.js')
        ? 'text/javascript'
        : target.endsWith('.css')
          ? 'text/css'
          : 'text/html',
    );
    res.end(bytes);
  });
});
app
  .whenReady()
  .then(async () => {
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    const window = new BrowserWindow({
      show: false,
      width: 1200,
      height: 800,
      webPreferences: {
        offscreen: true,
        preload: path.join(__dirname, 'ui-preload.cjs'),
        sandbox: false,
        contextIsolation: true,
        nodeIntegration: false,
        backgroundThrottling: false,
      },
    });
    try {
      await window.loadURL(`http://127.0.0.1:${server.address().port}`);
      await new Promise((resolve) => setTimeout(resolve, 350));
      fs.writeFileSync(
        path.join(output, 'ui-home.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      const result = await window.webContents.executeJavaScript(
        `(async()=>{
        const wait=ms=>new Promise(r=>setTimeout(r,ms));
        const assert=(ok,message)=>{if(!ok)throw new Error(message);};
        const until=async(test,message)=>{for(let i=0;i<100;i++){if(test())return;await wait(40);}throw new Error(message);};
        const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
        const label=text=>[...document.querySelectorAll('label')].find(l=>l.textContent.includes(text));
        Math.random=()=>0;
        document.querySelector('.home-station').click();
        await until(()=>document.querySelector('.track-title')?.textContent.includes('Embedded song'),'Embedded tags not displayed');
        assert(document.querySelector('.lyrics .current')?.textContent==='Opening lyric','Synced lyric missing');
        assert(!document.querySelector('#homeView'),'Home remains in player');
        assert(document.querySelector('.compact-player'),'Bottom bar missing from player');
        assert(getComputedStyle(document.querySelector('.signal')).color==='rgb(255, 136, 0)','Saved accent not applied');
        await wait(200);
        assert(window.__testCalls().filter(c=>c==='track_details').length===1,'Metadata fetched on progress ticks');
        button('← All Stations').click();await wait(30);document.querySelector('.home-station').click();await wait(60);
        assert(window.__testCalls().filter(c=>c==='prepare_station').length===1,'Returning to player restarted audio');
        button('Edit station').click();await until(()=>document.querySelector('dialog')?.open,'Editor not modal');
        const name=document.querySelector('dialog input');name.value='Fixture Radio Live';name.dispatchEvent(new Event('input',{bubbles:true}));await wait(20);
        assert(document.querySelector('#previewName').textContent==='Fixture Radio Live','Live name preview failed');
        [...document.querySelectorAll('dialog summary')].find(e=>e.textContent==='Appearance').click();
        label('Use a banner color').querySelector('input').click();await wait(20);
        const color=label('Banner color').querySelector('input');color.value='#244b42';color.dispatchEvent(new Event('input',{bubbles:true}));
        label('Override player accent').querySelector('input').click();await wait(20);
        const accent=label('Station accent').querySelector('input');accent.value='#c7f464';accent.dispatchEvent(new Event('input',{bubbles:true}));await wait(20);
        assert(getComputedStyle(document.querySelector('.banner-preview')).backgroundColor==='rgb(36, 75, 66)','Live banner color failed');
        button('Save station').click();await until(()=>!document.querySelector('dialog'),'Editor did not close');
        assert(document.querySelector('.now-playing strong').textContent==='Fixture Radio Live','Rename missing from player');
        assert(document.querySelector('[aria-label="Pause"]'),'Edit stopped playback');
        assert(window.__testCalls().filter(c=>c==='prepare_station').length===1,'Appearance edit retuned audio');
        await until(()=>getComputedStyle(document.querySelector('.signal')).color==='rgb(199, 244, 100)','Station accent not applied');
        const callsBeforeVolume=window.__testCalls().filter(c=>c==='prepare_station').length;
        document.querySelector('.radio-screen .volume-module').dispatchEvent(new WheelEvent('wheel',{deltaY:-100,bubbles:true,cancelable:true}));await wait(30);
        assert(document.querySelector('.radio-screen .volume-value').textContent==='85%','Volume wheel did not adjust');
        assert(window.__testCalls().filter(c=>c==='prepare_station').length===callsBeforeVolume,'Volume wheel tuned stations');
        document.querySelector('[aria-label="Create group"]').click();await until(()=>document.querySelector('dialog')?.open,'Group dialog missing');
        const groupName=document.querySelector('dialog input');groupName.value='Favorites';groupName.dispatchEvent(new Event('input',{bubbles:true}));
        document.querySelector('.member-list details').open=true;document.querySelector('.track-choice input').click();await wait(20);
        button('Save group').click();await until(()=>!document.querySelector('dialog'),'Group did not save');
        assert(document.querySelector('.group-selector select').selectedOptions[0].textContent==='Favorites','New group not selected');
        assert(document.querySelectorAll('.sidebar-station').length===1,'Group filter did not apply');
        document.querySelector('.sidebar-station').click();await until(()=>document.querySelector('[aria-label="Pause"]'),'Group did not play');
        assert(window.__testCalls().filter(c=>c==='prepare_station').length===2,'Group station did not retune');
        button('⚙ Settings').click();await wait(30);button('Appearance').click();await wait(20);
        label('Show recording progress').querySelector('input').click();
        await until(()=>window.__testSettings().showProgress===false,'Setting did not autosave');
        assert(!document.querySelector('.progress'),'Progress still visible');
        label('Display scanlines').querySelector('input').click();await until(()=>document.documentElement.classList.contains('no-scanlines'),'Scanline toggle failed');
        const layout=label('Station banner layout').querySelector('select');layout.value='column';layout.dispatchEvent(new Event('change',{bubbles:true}));
        await until(()=>window.__testSettings().bannerLayout==='column','Layout not saved');
        window.__testFailSetting();label('Reduce motion').querySelector('input').click();await until(()=>button('Retry save'),'Save failure not reported');
        button('Retry save').click();await until(()=>window.__testSettings().reduceMotion===true,'Retry did not save');
        await until(()=>!document.querySelector('.deck > .error'),'Recovered settings error remains visible');
        button('Advanced').click();await wait(20);button('Clear metadata cache').click();await until(()=>window.__testCalls().includes('clear_metadata_cache'),'Cache clear not requested');
        button('← Back to stations').click();await wait(30);
        assert(!document.querySelector('.dial').classList.contains('banner-row'),'Column layout not applied');
        assert(document.querySelector('.compact-player'),'Persistent player missing');
        button('MANAGE STATIONS').click();await until(()=>document.querySelector('dialog')?.open,'Manager not modal');
        const rows=document.querySelectorAll('.bulk-row');rows[1].querySelector('input').click();rows[2].querySelector('input').click();await wait(20);
        button('Delete 2 stations').click();await wait(20);assert(!window.__testCalls().includes('delete_stations'),'Bulk removal skipped confirmation');
        button('Confirm delete').click();await until(()=>!document.querySelector('dialog'),'Bulk removal did not finish');
        assert(document.querySelector('[aria-label="Pause"]'),'Deleting other stations stopped playback');
        assert(window.__testCalls().includes('delete_stations'),'Bulk command not used');
        const selector=document.querySelector('.group-selector select');selector.value='all';selector.dispatchEvent(new Event('change',{bubbles:true}));await wait(20);
        [...document.querySelectorAll('.sidebar-station')].find(e=>e.textContent.includes('Slow Signals')).click();
        await until(()=>document.querySelector('.track-title')?.textContent.includes('quiet end'),'Long fixture title missing');await wait(50);
        assert(document.querySelector('.track-title').getAnimations().length===0,'Reduced motion still animates titles');
        button('⚙ Settings').click();await wait(20);button('Appearance').click();await wait(20);
        label('Reduce motion').querySelector('input').click();label('Show recording progress').querySelector('input').click();label('Display scanlines').querySelector('input').click();
        const row=label('Station banner layout').querySelector('select');row.value='row';row.dispatchEvent(new Event('change',{bubbles:true}));
        await until(()=>window.__testSettings().bannerLayout==='row'&&!window.__testSettings().reduceMotion,'Appearance defaults not restored');
        button('← Back to stations').click();await wait(100);
        if(!matchMedia('(prefers-reduced-motion: reduce)').matches)await until(()=>document.querySelector('.track-title').getAnimations().length>0,'Long title did not bounce');
        document.querySelector('.sidebar-station').click();await until(()=>document.querySelector('.track-title')?.textContent.includes('Embedded song'),'First station not restored');
        window.__testEmit('audio-drop-start',1);window.__testEmit('audio-drop',{id:1,error:'',tracks:[{path:'first.wav',name:'First'},{path:'first.wav',name:'First'},{path:'drop.wav',name:'Dropped song'}]});
        await until(()=>document.querySelector('dialog')?.open,'Drop did not open station editor');
        assert(document.querySelectorAll('.file-row').length===2,'Dropped files not deduplicated');
        window.__testEmit('audio-drop-start',2);button('Cancel').click();await wait(20);window.__testEmit('audio-drop',{id:2,error:'',tracks:[{path:'late.wav',name:'Late'}]});await wait(20);
        assert(!document.querySelector('dialog'),'Late drop reopened a cancelled editor');
        return 'UI passed: Electron structure, persistent bar, tags/lyric, editor, group dialogs, volume wheel, autosave/retry, scanlines, banner layout and bulk removal.';
      })()`,
        true,
      );
      console.log(result);
      await new Promise((resolve) => setTimeout(resolve, 750));
      fs.writeFileSync(
        path.join(output, 'ui-player.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      await window.webContents.executeJavaScript(`document.querySelector('.edit-link').click()`);
      await new Promise((r) => setTimeout(r, 100));
      fs.writeFileSync(
        path.join(output, 'ui-editor.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      await window.webContents.executeJavaScript(
        `[...document.querySelectorAll('dialog button')].find(b=>b.textContent==='Cancel').click()`,
      );
      window.setContentSize(760, 560);
      await new Promise((r) => setTimeout(r, 100));
      console.log(
        await window.webContents.executeJavaScript(
          `(()=>{if(document.querySelector('.deck').scrollWidth>document.querySelector('.deck').clientWidth)throw new Error('Player overflows minimum window');if(document.querySelector('.compact-player').getBoundingClientRect().bottom>innerHeight)throw new Error('Bottom bar out of view');return 'Responsive player and bottom bar fit 760 × 560';})()`,
        ),
      );
      fs.writeFileSync(
        path.join(output, 'ui-player-small.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      window.setContentSize(480, 160);
      await window.loadURL(`http://127.0.0.1:${server.address().port}/index.html?mini=1`);
      console.log(
        await window.webContents.executeJavaScript(
          `(async()=>{
        const wait=ms=>new Promise(r=>setTimeout(r,ms));const assert=(ok,message)=>{if(!ok)throw new Error(message);};
        for(let i=0;i<100&&!document.querySelector('.mini-titles strong')?.textContent.includes('Fixture Radio');i++)await wait(50);
        assert(document.querySelector('.mini-titles strong')?.textContent==='Fixture Radio','Mini snapshot not displayed');
        assert(!window.__testCalls().includes('load_config')&&!window.__testCalls().includes('prepare_station'),'Mini started another library or audio engine');
        document.querySelector('[aria-label="Pause"]').click();document.querySelector('[aria-label="Next station"]').click();
        const volume=document.querySelector('input[type="range"]');volume.value='.4';volume.dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('[aria-label="Restore main window"]').click();await wait(100);
        const commands=window.__testMessages();assert(commands.some(c=>c.type==='toggle')&&commands.some(c=>c.type==='next')&&commands.some(c=>c.type==='volume'&&c.value===.4),'Mini commands not forwarded');assert(window.__testCalls().includes('desktop_restore'),'Restore not requested');
        assert(document.querySelector('.mini-player').getBoundingClientRect().height<=innerHeight,'Mini overflows viewport');
        return 'Mini UI smoke passed: snapshot display, control forwarding, volume, restore, and no duplicate audio engine.';
      })()`,
          true,
        ),
      );
      fs.writeFileSync(
        path.join(output, 'phase5-mini.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      clearTimeout(timeout);
      window.destroy();
      server.close();
      app.exit(0);
    } catch (error) {
      console.error(error);
      console.error(
        await window.webContents.executeJavaScript(
          'JSON.stringify({text:document.body.innerText,calls:window.__testCalls()})',
        ),
      );
      fs.writeFileSync(
        path.join(output, 'phase4-ui.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      clearTimeout(timeout);
      window.destroy();
      server.close();
      app.exit(1);
    }
  })
  .catch((error) => {
    console.error(error);
    app.exit(1);
  });
