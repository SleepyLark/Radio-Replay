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
      const result = await window.webContents.executeJavaScript(
        `(async()=>{
      const wait=ms=>new Promise(r=>setTimeout(r,ms));const assert=(ok,message)=>{if(!ok)throw new Error(message);};
      const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
      await wait(200);Math.random=()=>0;document.querySelector('.station').click();for(let i=0;i<100&&!document.querySelector('.now-playing').textContent.includes('Embedded song');i++)await wait(50);await wait(200);
      assert(document.querySelector('.now-playing').textContent.includes('Fixture artist - Embedded song'),'Embedded tags not displayed');
      assert(document.querySelector('.lyrics .current')?.textContent==='Opening lyric','Synchronized lyric not highlighted');
      assert(window.__testCalls().filter(c=>c==='track_details').length===1,'Metadata fetched repeatedly on progress ticks');
      assert(getComputedStyle(document.querySelector('.brand')).color==='rgb(255, 136, 0)','Saved accent not applied');
      button('Groups').click();await wait(50);const name=document.querySelector('.editor input:not([type])');name.value='Favorites';name.dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('.track-choice input').click();await wait(20);button('Save group').click();await wait(100);
      const selector=document.querySelector('.group-selector select');selector.value=[...selector.options].find(o=>o.text==='Favorites').value;selector.dispatchEvent(new Event('change',{bubbles:true}));button('Stations').click();await wait(50);document.querySelector('.station').click();await wait(200);
      assert(window.__testCalls().filter(c=>c==='prepare_station').length===2,'Group station did not retune');
      button('Settings').click();await wait(50);const label=[...document.querySelectorAll('.editor label')].find(l=>l.textContent.includes('Show recording progress'));label.querySelector('input').click();button('Save settings').click();await wait(100);
      assert(!document.querySelector('.progress'),'Hidden progress setting not applied');
      button('Clear metadata cache').click();await wait(100);assert(window.__testCalls().includes('clear_metadata_cache'),'Cache clear not requested');
      return 'UI smoke passed: tags, timed lyrics, one metadata request per track, accent, group editing/filtering, settings and cache clearing.';
    })()`,
        true,
      );
      fs.writeFileSync(
        path.join(output, 'phase4-ui.png'),
        (await window.webContents.capturePage()).toPNG(),
      );
      console.log(result);
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
