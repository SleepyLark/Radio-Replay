const {chromium}=require('playwright');
let browser;
const fs = require('node:fs');
const path = require('node:path');
const http = require('node:http');
const output = path.join(__dirname, '..', 'out');
fs.mkdirSync(output, { recursive: true });

const timeout = setTimeout(() => {
  console.error('UI smoke timed out');
  process.exit(1);
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
  if (req.url.split('?')[0] === '/fixture-banner.svg') {
    res.setHeader('Content-Type', 'image/svg+xml');
    res.end(
      '<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="400" viewBox="0 0 1200 400"><defs><linearGradient id="sky" x2="1" y2=".4"><stop stop-color="#173d37"/><stop offset=".55" stop-color="#7e9d69"/><stop offset="1" stop-color="#19374b"/></linearGradient></defs><rect width="1200" height="400" fill="url(#sky)"/><circle cx="920" cy="105" r="110" fill="#e4d49d"/><path d="M0 310 210 180 410 290 650 130 920 290 1200 190v210H0Z" fill="#173c36"/><path d="M0 365 350 265 700 360 1030 230 1200 295v105H0Z" fill="#0d2226"/></svg>',
    );
    return;
  }
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
(async () => {
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    browser=await chromium.launch({channel:process.env.RADIO_REPLAY_BROWSER||'msedge',headless:true,args:['--autoplay-policy=no-user-gesture-required']});
    const page=await browser.newPage({viewport:{width:1200,height:800}});
    await page.addInitScript({path:path.join(__dirname,'ui-fixture.js')});
    try {
      await page.goto(`http://127.0.0.1:${server.address().port}`);
      await new Promise((resolve) => setTimeout(resolve, 350));
      fs.writeFileSync(
        path.join(output, 'ui-home.png'),
        await page.screenshot(),
      );
      const result = await page.evaluate(
        `(async()=>{
        const wait=ms=>new Promise(r=>setTimeout(r,ms));
        const assert=(ok,message)=>{if(!ok)throw new Error(message);};
        const until=async(test,message)=>{for(let i=0;i<100;i++){if(test())return;await wait(40);}throw new Error(message);};
        const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
        const label=text=>[...document.querySelectorAll('label')].find(l=>l.textContent.includes(text));
        const checkBar=()=>{const bar=document.querySelector('.compact-player'),side=document.querySelector('aside').getBoundingClientRect(),box=bar.getBoundingClientRect();assert(Math.abs(box.left-side.right)<2,'Bar extends under sidebar');assert(Math.abs(box.right-innerWidth)<2&&Math.abs(box.bottom-innerHeight)<2,'Bar does not fill bottom of content pane');assert(Math.abs(side.bottom-innerHeight)<2,'Sidebar no longer spans full app height');assert(bar.scrollWidth<=bar.clientWidth,'Bar controls overflow');};
        Math.random=()=>0;
        assert(!document.querySelector('.compact-player'),'Bar visible before tuning');
        assert(!document.querySelector('.sidebar-tools'),'Sidebar footer remains');
        assert(!document.querySelector('aside').textContent.includes('Import library'),'Import remains in sidebar');
        assert(!document.querySelector('aside').textContent.includes('Quit'),'Quit remains in sidebar');
        button('MANAGE STATIONS').click();await until(()=>document.querySelector('dialog')?.open,'Manager missing');
        assert(document.querySelector('dialog .manage-import button')?.textContent==='Import library','Manager missing import');
        button('Cancel').click();await wait(30);
        document.querySelector('.home-station').click();
        await until(()=>document.querySelector('.track-title')?.textContent.includes('Embedded song'),'Embedded tags not displayed');
        assert(document.querySelector('.lyrics .current')?.textContent==='Opening lyric','Synced lyric missing');
        assert(!document.querySelector('#homeView'),'Home remains in player');
        assert(!document.querySelector('.compact-player'),'Bottom bar visible on player');
        assert(document.querySelectorAll('[aria-label="Pause"] svg rect').length===2,'Pause is not a two-bar SVG icon');
        assert(document.querySelector('[aria-label="Mute"] svg g'),'Speaker SVG missing');
        assert(getComputedStyle(document.querySelector('.signal')).color==='rgb(255, 136, 0)','Saved accent not applied');
        await wait(200);
        assert(window.__testCalls().filter(c=>c==='track_details').length===1,'Metadata fetched on progress ticks');
        await until(()=>!!document.querySelector('.toast'),'Skipped-file toast missing');
        assert(!document.querySelector('.deck > .notice')&&!document.querySelector('.deck > .error'),'Playback notice still occupies the top of the layout');
        const headerTop=document.querySelector('.radio-screen header').getBoundingClientRect().top;
        button('View files (1)').click();await until(()=>document.querySelector('dialog')?.open,'Problem files dialog did not open');
        assert(document.querySelector('.problem-list code').textContent==='C:\\\\Radio\\\\missing.mp3','Problematic path missing from details');
        assert(document.querySelector('.problem-list').textContent.includes('File not found'),'Failure reason missing');
        const problemSearch=document.querySelector('dialog input');problemSearch.value='not-present';problemSearch.dispatchEvent(new Event('input',{bubbles:true}));await wait(20);assert(document.querySelector('.problem-list').textContent.includes('No files match'),'Problem search did not filter');
        button('Close').click();await wait(20);document.querySelector('.toast-dismiss').click();await wait(300);
        assert(!document.querySelector('.toast'),'Dismissed toast returned during progress updates');
        assert(document.querySelector('[aria-label="Pause"]'),'Viewing/dismissing warning stopped playback');
        window.__testMediaAction('pause');await until(()=>document.querySelector('[aria-label="Play"]'),'Media pause failed');
        window.__testMediaAction('play');await until(()=>document.querySelector('[aria-label="Pause"]'),'Media play failed');
        assert(document.querySelector('.radio-screen header').getBoundingClientRect().top===headerTop,'Toast changed the player layout');
        button('← All Stations').click();await wait(30);assert(document.querySelector('.compact-player'),'Bar missing after returning to All Stations');checkBar();document.querySelector('.compact-title').click();await wait(30);assert(!document.querySelector('.compact-player'),'Bar remained after reopening player');
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
        document.querySelector('.member-list details').open=true;await until(()=>document.querySelector('.track-choice input'),'Track choices did not expand');document.querySelector('.track-choice input').click();await wait(20);
        button('Save group').click();await until(()=>!document.querySelector('dialog'),'Group did not save');
        assert(document.querySelector('.group-selector select').selectedOptions[0].textContent==='Favorites','New group not selected');
        assert(document.querySelectorAll('.sidebar-station').length===1,'Group filter did not apply');
        document.querySelector('.sidebar-station').click();await until(()=>document.querySelector('[aria-label="Pause"]'),'Group did not play');
        assert(window.__testCalls().filter(c=>c==='prepare_station').length===2,'Group station did not retune');
        assert(!document.querySelector('.toast'),'Filtered-out missing file produced a warning');
        button('⚙ Settings').click();await wait(30);assert(!label('Keep playing in the tray').querySelector('input').checked,'Close-to-tray starts enabled');button('Appearance').click();await wait(20);
        assert(!document.querySelector('.compact-player'),'Bar visible in Settings');
        label('Show recording progress').querySelector('input').click();
        await until(()=>window.__testSettings().showProgress===false,'Setting did not autosave');
        assert(!document.querySelector('.progress'),'Progress still visible');
        label('Display scanlines').querySelector('input').click();await until(()=>document.documentElement.classList.contains('no-scanlines'),'Scanline toggle failed');
        const layout=label('Station banner layout').querySelector('select');layout.value='column';layout.dispatchEvent(new Event('change',{bubbles:true}));
        await until(()=>window.__testSettings().bannerLayout==='column','Layout not saved');
        window.__testFailSetting();label('Reduce motion').querySelector('input').click();await until(()=>button('Retry save'),'Save failure not reported');
        await until(()=>document.querySelector('.toast')?.textContent.includes('Fixture write failure'),'Application error did not produce a toast');
        button('Retry save').click();await until(()=>window.__testSettings().reduceMotion===true,'Retry did not save');
        await until(()=>!document.querySelector('.deck > .error'),'Recovered settings error remains visible');
        await until(()=>!document.querySelector('.toast'),'Resolved application error toast remains visible');
        button('Advanced').click();await wait(20);button('Clear metadata cache').click();await until(()=>window.__testCalls().includes('clear_metadata_cache'),'Cache clear not requested');
        button('← Back to stations').click();await wait(30);
        assert(!document.querySelector('.dial').classList.contains('banner-row'),'Column layout not applied');
        assert(!document.querySelector('.compact-player'),'Bar visible after returning to player');
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
        return 'UI passed: file toast/details/search/dismissal, All Stations bar, playback, editor, groups, autosave/retry, and bulk removal.';
      })()`,
        true,
      );
      console.log(result);
      await new Promise((resolve) => setTimeout(resolve, 750));
      fs.writeFileSync(
        path.join(output, 'ui-toast.png'),
        await page.screenshot(),
      );
      await page.evaluate(
        `document.querySelector('.toast-details').click()`,
      );
      await new Promise((resolve) => setTimeout(resolve, 70));
      fs.writeFileSync(
        path.join(output, 'ui-file-problems.png'),
        await page.screenshot(),
      );
      await page.evaluate(
        `[...document.querySelectorAll('dialog button')].find(b=>b.textContent==='Close').click()`,
      );
      await new Promise((resolve) => setTimeout(resolve, 30));
      await page.evaluate(
        `document.querySelector('.toast-dismiss').click()`,
      );
      fs.writeFileSync(
        path.join(output, 'ui-player.png'),
        await page.screenshot(),
      );
      await page.evaluate(
        `document.querySelector('.back-stations').click()`,
      );
      await new Promise((r) => setTimeout(r, 50));
      fs.writeFileSync(
        path.join(output, 'ui-home-playing.png'),
        await page.screenshot(),
      );
      await page.evaluate(
        `document.querySelector('.compact-title').click()`,
      );
      await new Promise((r) => setTimeout(r, 30));
      await page.evaluate(`document.querySelector('.edit-link').click()`);
      await new Promise((r) => setTimeout(r, 100));
      fs.writeFileSync(
        path.join(output, 'ui-editor.png'),
        await page.screenshot(),
      );
      await page.evaluate(
        `[...document.querySelectorAll('dialog button')].find(b=>b.textContent==='Cancel').click()`,
      );
      await page.setViewportSize({width:760,height:560});
      await new Promise((r) => setTimeout(r, 100));
      console.log(
        await page.evaluate(
          `(()=>{if(document.querySelector('.deck').scrollWidth>document.querySelector('.deck').clientWidth)throw new Error('Player overflows minimum window');if(document.querySelector('.compact-player'))throw new Error('Bar visible on small player screen');return 'Responsive player fits 760 × 560';})()`,
        ),
      );
      fs.writeFileSync(
        path.join(output, 'ui-player-small.png'),
        await page.screenshot(),
      );
      await page.evaluate(
        `document.querySelector('.back-stations').click()`,
      );
      await new Promise((r) => setTimeout(r, 50));
      await page.evaluate(
        `(()=>{const bar=document.querySelector('.compact-player'),side=document.querySelector('aside').getBoundingClientRect();if(!bar||bar.getBoundingClientRect().bottom>innerHeight)throw new Error('All Stations bar missing or outside viewport');if(Math.abs(bar.getBoundingClientRect().left-side.right)>1||bar.scrollWidth>bar.clientWidth)throw new Error('Bar overlaps sidebar or overflows narrow pane');})()`,
      );
      fs.writeFileSync(
        path.join(output, 'ui-home-playing-small.png'),
        await page.screenshot(),
      );
      await page.setViewportSize({width:480,height:160});
      await page.goto(`http://127.0.0.1:${server.address().port}/index.html?mini=1`);
      console.log(
        await page.evaluate(
          `(async()=>{
        const wait=ms=>new Promise(r=>setTimeout(r,ms));const assert=(ok,message)=>{if(!ok)throw new Error(message);};
        for(let i=0;i<100&&!document.querySelector('.mini-titles strong')?.textContent.includes('Fixture Radio');i++)await wait(50);
        assert(document.querySelector('.mini-titles strong')?.textContent==='Fixture Radio','Mini snapshot not displayed');
        for(let i=0;i<100&&!document.querySelector('.mini-banner')?.naturalWidth;i++)await wait(30);
        assert(document.querySelector('.mini-banner')?.naturalWidth>0,'Mini banner did not load');
        assert(document.querySelector('.mini-banner').src.endsWith('/fixture-banner.svg'),'Mini background uses wrong image');
        assert(document.querySelectorAll('.volume-segments i').length===12&&document.querySelectorAll('.volume-segments .lit').length===10,'Shared stepped volume not displayed');
        assert(document.querySelector('[aria-label="Playback time"]').textContent==='00:05 / 00:30','Mini elapsed/total time missing');
        const time=document.querySelector('[aria-label="Playback time"]').getBoundingClientRect(),bar=document.querySelector('progress').getBoundingClientRect();
        assert(time.left>bar.right&&time.bottom<=innerHeight&&time.right<=innerWidth,'Time is not beside progress at bottom right');
        assert(!window.__testCalls().includes('load_config')&&!window.__testCalls().includes('prepare_station'),'Mini started another library or audio engine');
        document.querySelector('[aria-label="Pause"]').click();document.querySelector('[aria-label="Next station"]').click();
        const volume=document.querySelector('input[type="range"]');volume.value='.4';volume.dispatchEvent(new Event('input',{bubbles:true}));document.querySelector('[aria-label="Restore main window"]').click();await wait(100);
        const commands=window.__testMessages();assert(commands.some(c=>c.type==='toggle')&&commands.some(c=>c.type==='next')&&commands.some(c=>c.type==='volume'&&c.value===.4),'Mini commands not forwarded');assert(window.__testCalls().includes('desktop_restore'),'Restore not requested');
        assert(document.querySelector('.mini-player').getBoundingClientRect().height<=innerHeight,'Mini overflows viewport');
        const fixture={station:'Fixture Radio',title:'Embedded song',artist:'Fixture artist',album:'Fixture album',art:'',banner:location.origin+'/fixture-banner.svg',bannerColor:'#244b42',accent:'#ff8800',playing:true,hasStation:true,canNavigate:true,seconds:5,duration:30,volume:.8,muted:false};
        window.__testEmit('mini-state',{...fixture,station:'Second station',banner:fixture.banner+'?second=1',seconds:3661,duration:7200,volume:.4});await wait(50);
        assert(document.querySelector('.mini-banner').src.endsWith('?second=1'),'Mini banner did not follow station change');
        assert(document.querySelector('[aria-label="Playback time"]').textContent==='61:01 / 120:00','Long recording times formatted incorrectly');
        assert(document.querySelectorAll('.volume-segments .lit').length===5,'Volume segments did not follow snapshot');
        window.__testEmit('mini-state',{...fixture,muted:true});await wait(30);assert(document.querySelectorAll('.volume-segments .lit').length===0,'Muted volume segments remain lit');
        document.querySelector('.volume-module').dispatchEvent(new WheelEvent('wheel',{deltaY:-100,bubbles:true,cancelable:true}));await wait(20);
        assert(window.__testMessages().some(c=>c.type==='volume'&&Math.abs(c.value-.85)<.001),'Volume wheel not forwarded');
        window.__testEmit('mini-state',{...fixture,banner:location.origin+'/missing-banner.png'});for(let i=0;i<100&&document.querySelector('.mini-banner');i++)await wait(20);
        assert(!document.querySelector('.mini-banner'),'Broken banner did not fall back');
        window.__testEmit('mini-state',{...fixture,banner:'',seconds:0,duration:0});await wait(30);assert(!document.querySelector('.mini-banner')&&document.querySelector('[aria-label="Playback time"]').textContent==='00:00 / 00:00','Empty station fallback failed');
        window.__testEmit('mini-state',fixture);await wait(100);
        assert(!window.__testCalls().includes('prepare_station'),'Snapshot changes started another audio engine');
        return 'Mini UI passed: banner/fallback, shared volume steps and wheel, elapsed/total time, snapshot changes, controls, restore and single audio engine.';
      })()`,
          true,
        ),
      );
      fs.writeFileSync(
        path.join(output, 'phase5-mini.png'),
        await page.screenshot(),
      );
      await page.setViewportSize({width:1200,height:800});
      await page.goto(`http://127.0.0.1:${server.address().port}/index.html?large=1`);
      console.log(await page.evaluate(`(async()=>{
        const wait=ms=>new Promise(r=>setTimeout(r,ms));
        const until=async(test,message)=>{for(let i=0;i<200;i++){if(test())return;await wait(20);}throw new Error(message);};
        const assert=(ok,message)=>{if(!ok)throw new Error(message);};
        const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent.trim()===text);
        await until(()=>document.querySelector('.sidebar-station'),'Large library did not load');
        button('MANAGE STATIONS').click();await until(()=>document.querySelector('dialog')?.open,'Large manager missing');
        document.querySelector('.bulk-row .station-edit').click();
        await until(()=>document.querySelector('#fileList'),'Large editor missing');
        assert(document.querySelectorAll('.file-row').length===100,'Editor rendered the entire library');
        document.querySelector('.file-more').click();await until(()=>document.querySelectorAll('.file-row').length===200,'More files did not load');
        document.querySelector('[aria-label="Remove Track 150"]').click();
        await until(()=>!document.querySelector('[aria-label="Remove Track 150"]'),'Paged track could not be removed');
        assert(document.querySelectorAll('.file-row').length===200,'Page collapsed after editing');
        document.querySelectorAll('.file-folder')[1].open=true;
        await until(()=>document.querySelector('[aria-label="Remove Track 1000"]'),'Second folder did not expand');
        document.querySelectorAll('.file-folder')[0].open=false;
        await until(()=>!document.querySelector('[aria-label="Remove Track 0"]'),'Closed folder retained rows');
        document.querySelectorAll('.file-folder')[1].querySelector('summary button').click();
        await until(()=>document.querySelectorAll('.file-folder').length===19,'Remove folder only removed visible rows');
        button('Cancel').click();await until(()=>!document.querySelector('dialog'),'Editor did not cancel');
        assert(window.__testConfig().custom[0].tracks.length===20000,'Cancelled editor changed the library');
        document.querySelector('[aria-label="Create group"]').click();
        await until(()=>document.querySelector('dialog')?.open,'Large group editor missing');
        assert(document.querySelectorAll('.track-choice').length===0,'Closed group station rendered tracks');
        document.querySelector('.member-list details').open=true;
        await until(()=>document.querySelectorAll('.track-choice').length===100,'Group expansion was not bounded');
        document.querySelector('.group-more').click();await until(()=>document.querySelectorAll('.track-choice').length===200,'More group tracks did not load');
        document.querySelectorAll('.track-choice input')[150].click();
        document.querySelector('.member-list details').open=false;
        await until(()=>document.querySelectorAll('.track-choice').length===0,'Collapsed group retained tracks');
        document.querySelector('.member-list details').open=true;
        await until(()=>document.querySelectorAll('.track-choice').length===100,'Group did not reopen on first page');
        document.querySelector('.group-more').click();await until(()=>document.querySelectorAll('.track-choice').length===200,'Group page did not reopen');
        assert(document.querySelectorAll('.track-choice input')[150].checked,'Hidden group selection was lost');
        const name=document.querySelector('dialog input');name.value='Paged selection';name.dispatchEvent(new Event('input',{bubbles:true}));await wait(20);
        button('Save group').click();await until(()=>!document.querySelector('dialog'),'Paged group did not save');
        assert(window.__testConfig().groups[0].members[0]==='large|C:/Fixture/Album 0/track-150.mp3','Wrong paged track saved');
        return '20,000-track UI passed: bounded rows, expansion, pagination, folder removal, cancel and hidden group selections.';
      })()`));
      clearTimeout(timeout);
      await browser.close();
      server.close();
      process.exit(0);
    } catch (error) {
      console.error(error);
      console.error(
        await page.evaluate(
          'JSON.stringify({text:document.body.innerText,calls:window.__testCalls()})',
        ),
      );
      fs.writeFileSync(
        path.join(output, 'phase4-ui.png'),
        await page.screenshot(),
      );
      clearTimeout(timeout);
      await browser.close();
      server.close();
      process.exit(1);
    }
  })()
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });
