// Injected only by the explicit native-smoke Cargo feature and isolated profile.
void (async () => {
  const invoke = (command, args = {}) => window.__TAURI_INTERNALS__.invoke(command, args);
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const until = async (test, message) => {
    for (let i = 0; i < 200; i++) {
      if (await test()) return;
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
      await until(
        () => document.querySelector('.mini-banner')?.naturalWidth > 0,
        'Native mini banner did not load',
      );
      const initialMini = await invoke('desktop_snapshot');
      assert(
        document.querySelector('.mini-banner').src === initialMini.banner,
        'Native mini received wrong banner',
      );
      assert(
        document.querySelectorAll('.volume-segments i').length === 12,
        'Native stepped volume missing',
      );
      assert(
        document.querySelector('[aria-label="Playback time"]').textContent.includes('/ 00:10'),
        'Native mini track length missing',
      );
      let denied = false;
      try {
        await invoke('save_settings', { patch: { accent: '#123456' } });
      } catch(error) {
        denied = String(error).includes('Main player required');
      }
      assert(denied, 'Mini window could mutate library settings');
      for(const command of ['load_config','clear_metadata_cache']){
        let denied=false;try{await invoke(command);}catch(error){denied=String(error).includes('Main player required');}
        assert(denied,`Mini bypassed the Rust window-label guard for ${command}`);
      }
      let nativeDenied=false;
      try{await invoke('native_control',{control:{type:'play'}});}catch{nativeDenied=true;}
      assert(nativeDenied,'Mini bypassed the main-player native audio command boundary');
      let linkDenied=false;
      try{await invoke('add_network_stream',{url:'https://radio.plaza.one/mp3',name:''});}catch{linkDenied=true;}
      assert(linkDenied,'Mini could add network stream grants');
      document.querySelector('[aria-label="Pause"]').click();
      await until(
        () => !!document.querySelector('[aria-label="Play"]'),
        'Mini pause did not reach the main engine',
      );
      const volume = document.querySelector('input[type="range"]');
      volume.value = '.25';
      volume.dispatchEvent(new Event('input', { bubbles: true }));
      await until(
        () => document.querySelector('.volume-value')?.textContent === '25%'&&document.querySelector('[aria-label="Mute"]'),
        'Moving the mini volume slider did not unmute to the new level',
      );
      assert(
        document.querySelectorAll('.volume-segments .lit').length === 3,
        'Native mini volume steps incorrect',
      );
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
      const link=await invoke('add_network_stream',{url:'http://radio.plaza.one/mp3',name:'Nightwave Plaza'});
      assert(link.path==='http://radio.plaza.one/mp3'&&link.name==='Nightwave Plaza'&&link.source==='network','Manual URL command did not return the validated track');
      const song=await invoke('add_network_stream',{url:'https://radio.plaza.one/example.mp3',name:''});
      assert(song.path.endsWith('/example.mp3')&&song.name==='radio.plaza.one','Direct MP3 URL command failed');
      let unsafeDenied=false;
      try{await invoke('add_network_stream',{url:'http://127.0.0.1/private',name:''});}catch{unsafeDenied=true;}
      assert(unsafeDenied,'Manual URL command accepted a private address');
      await invoke('native_smoke_report',{phase:'manualLinks',ok:true,message:'Real main-window URL command, direct MP3 and private address rejection verified without fetching audio'});
      await until(
        () => document.querySelectorAll('.station').length === 2,
        'Fixture library did not load',
      );
      Math.random = () => 0;
      const startup=await invoke('playback_backend');
      const audioBackend=startup.backend;
      if(startup.error){
        await until(()=>document.querySelector('.audio-runtime-error'),'Missing runtime did not produce a startup message');
        assert(document.querySelectorAll('.audio-runtime-error').length===1,'Runtime startup message was duplicated');
        assert([...document.querySelectorAll('.home-station')].every(button=>button.disabled),'Tuning is enabled without a runtime');
        assert(!document.querySelector('.choose').disabled,'Missing runtime disabled library editing');
        document.querySelector('.choose').click();
        await until(()=>document.querySelector('dialog')?.open,'Library editor was unavailable without audio');
        await invoke('native_smoke_report',{phase:'runtimeMissing',ok:true,message:'One startup runtime message, disabled tuning and usable library editor verified'});
        return;
      }
      await until(async()=>{const state=await invoke('desktop_snapshot');return state.audioReady&&state.volume===startup.levels.volume&&state.muted===startup.levels.muted;},'Saved audio levels did not reach the UI before tuning');
      if(audioBackend==='mpv'){
        const state=await invoke('native_snapshot');
        assert(state.volume===startup.levels.volume&&state.muted===startup.levels.muted,'Native runtime did not restore saved levels before tuning');
      }
      await invoke('native_smoke_report',{phase:'audioPreferencesStartup',ok:true,message:JSON.stringify(startup.levels)});
      const tuningProgress=[];
      if(audioBackend==='mpv')await invoke('plugin:event|listen',{
        event:'tuning-progress',target:{kind:'Any'},
        handler:window.__TAURI_INTERNALS__.transformCallback(event=>{
          tuningProgress.push(event.payload);if(tuningProgress.length>256)tuningProgress.shift();
        }),
      });
      let preparedGeneration=0;
      if(audioBackend==='mpv')await invoke('native_control',{control:{type:'volume',value:0}});
      const prepareCache=async()=>{
        if(audioBackend==='mpv'){
          const {generation}=await invoke('native_begin',{delayMs:0});
          preparedGeneration=generation;
          return (await invoke('native_prepare',{generation,key:'a',group:'all'})).tracks;
        }
        return (await invoke('prepare_station',{key:'a',group:'all'})).tracks;
      };
      if(audioBackend==='mpv'){
        for(const command of ['prepare_station','resolve_tuning_sound']){
          let denied=false;try{await invoke(command,{key:'a'});}catch{denied=true;}
          assert(denied,`Native build exposed the legacy ${command} command`);
        }
      }
      let emitDenied=false;
      try{await invoke('plugin:event|emit',{event:'native-audio-state',payload:{}});}catch{emitDenied=true;}
      assert(emitDenied,'Renderer can forge native state events');
      const uncached = await prepareCache();
      if(audioBackend==='mpv'){
        await until(()=>tuningProgress.some(p=>p.generation===preparedGeneration&&p.done===p.total&&p.total>=uncached.length),'Real tuning progress did not arrive through IPC');
        await invoke('native_smoke_report',{phase:'tuningProgress',ok:true,message:'Real bounded metadata preparation emitted generation-tagged progress'});
      }
      await invoke('native_smoke_report', {
        phase: 'startupCache',
        ok: true,
        message: `${uncached.filter((track) => track.duration > 0).length}/${uncached.length} prepared durations using the selected backend`,
      });
      const measured = uncached[0];
      let durationArgs={key:'a',path:measured.path,size:measured.size,modified:measured.modified,duration:10};
      if(audioBackend==='mpv'){
        assert(uncached.every(track=>track.path===undefined&&track.size===undefined&&Number.isSafeInteger(track.slot)),'Native preparation leaked the full file catalogue');
        const state=await invoke('native_load',{generation:preparedGeneration,key:'a',slots:uncached.map(t=>t.slot),index:0,seconds:0});
        assert(state.path,'Loaded native state did not identify its current recording');
        durationArgs={key:'a',path:state.path,duration:10};
      }
      await invoke('remember_duration',durationArgs);
      await invoke('native_smoke_report', {
        phase: 'cacheReopen',
        ok: true,
        message: 'Reopening SQLite after duration write through native IPC',
      });
      const warm = await prepareCache();
      if(audioBackend==='mpv'){
        await invoke('native_smoke_report',{phase:'changePreparedFixture',ok:true,message:'Changing isolated fixture timestamp after preparation'});
        let changedDenied=false;
        try{await invoke('native_load',{generation:preparedGeneration,key:'a',slots:warm.map(t=>t.slot),index:0,seconds:0});}catch(error){changedDenied=String(error).includes('Audio changed');}
        assert(changedDenied,'Indexed loading failed to recheck prepared file identity');
        await invoke('native_control',{control:{type:'stop'}});
        await invoke('native_control',{control:{type:'volume',value:.8}});
      }
      assert(warm[0].duration === 10, 'Duration write did not survive native IPC');
      [...document.querySelectorAll('button')]
        .find((b) => b.textContent.trim() === '⚙ Settings')
        .click();
      await until(() => !!document.querySelector('#settingsView'), 'Settings did not open');
      const trayChoice = [...document.querySelectorAll('label')]
        .find((l) => l.textContent.includes('Keep playing in the tray'))
        .querySelector('input');
      assert(!trayChoice.checked, 'Close-to-tray default is enabled');
      trayChoice.click();
      await until(
        () => document.querySelector('#settingsView [role="status"]')?.textContent === 'Saved',
        'Tray opt-in did not save',
      );
      document.querySelector('.settings-back').click();
      await until(() => !!document.querySelector('#homeView'), 'Home did not return');
      assert(!document.querySelector('.compact-player'), 'Bar visible before tuning');
      if(audioBackend==='mpv') {
        // Native playback, including tuning static, must not fall back to Web Audio.
        window.Audio=class {constructor(){throw new Error('Native player created browser Audio');}};
        window.AudioContext=class {constructor(){throw new Error('Native player created AudioContext');}};
      }
      window.dispatchEvent(new KeyboardEvent('keydown', { key: 'm', bubbles: true }));
      document.querySelector('.station').click();
      if(audioBackend==='mpv') {
        await until(async()=>{const state=await invoke('native_snapshot');return state.tuning&&state.tuningPlaying;},'Native tuning static did not start');
      }
      await until(
        () => !!document.querySelector('[aria-label="Pause"]'),
        'Audio playback failed',
      );
      if(audioBackend==='mpv') {
        const native=await invoke('native_snapshot');
        assert(native.playing&&native.duration>0,'libmpv is not supplying the player state');
        assert(!native.tuning&&!native.tuningPlaying,'Native static survived the transition to station audio');
        let rejected=false;
        try{await invoke('native_load',{generation:native.generation,key:'a',slots:[0,999999],index:0,seconds:0});}catch{rejected=true;}
        assert(rejected,'Native audio accepted an unprepared track index');
        for(const forged of [{key:'b',slots:[0,1]},{key:'a',slots:[0,0]},{key:'a',paths:['C:/not-prepared/private.wav']}]){
          let denied=false;try{await invoke('native_load',{generation:native.generation,index:0,seconds:0,...forged});}catch{denied=true;}
          assert(denied,'Native audio accepted a foreign station, duplicate slot or legacy path request');
        }
        assert((await invoke('native_snapshot')).playing,'Rejected audio command interrupted valid playback');
        await invoke('native_smoke_report',{phase:'libmpv',ok:true,message:'Native tuning static, station playback, file authorization and absence of browser audio verified'});
        await wait(300);
        await invoke('native_smoke_report',{phase:'publicationReset',ok:true,message:''});
        await wait(2000);
        await invoke('native_smoke_report',{phase:'publicationSamplePlaying',ok:true,message:'2000 ms steady playback'});
        await invoke('native_smoke_report',{phase:'publicationReset',ok:true,message:''});
        document.querySelector('[aria-label="Pause"]').click();
        await until(()=>!!document.querySelector('[aria-label="Play"]'),'Performance sample pause failed');
        await until(async()=>!(await invoke('desktop_snapshot')).playing,'Pause snapshot did not reach desktop');
        await wait(350);
        await invoke('native_smoke_report',{phase:'publicationSamplePauseControl',ok:true,message:'Pause reached desktop and native media'});
        await invoke('native_smoke_report',{phase:'publicationReset',ok:true,message:''});
        await wait(1000);
        await invoke('native_smoke_report',{phase:'publicationSamplePaused',ok:true,message:'1000 ms settled pause'});
        document.querySelector('[aria-label="Play"]').click();
        await until(()=>!!document.querySelector('[aria-label="Pause"]'),'Performance sample resume failed');
        await until(async()=>(await invoke('desktop_snapshot')).playing,'Resume snapshot did not reach desktop');
        await wait(100);
        await invoke('native_smoke_report',{phase:'publicationSampleResumeControl',ok:true,message:'Resume reached desktop and native media'});
      }
      await until(() => !!document.querySelector('.toast'), 'Native missing-file toast not shown');
      document.querySelector('.toast-details').click();
      await until(() => document.querySelector('dialog')?.open, 'Native file details did not open');
      assert(
        document.querySelector('.problem-list code')?.textContent.includes('missing-fixture.mp3'),
        'Native missing path not reported',
      );
      assert(
        document.querySelector('.problem-list')?.textContent.includes('File not found'),
        'Native failure reason not reported',
      );
      [...document.querySelectorAll('dialog button')]
        .find((b) => b.textContent === 'Close')
        .click();
      await until(() => !document.querySelector('dialog'), 'File report did not close');
      await wait(300);
      assert(!document.querySelector('.toast'), 'Dismissed native toast reappeared');
      assert(!document.querySelector('.compact-player'), 'Bar visible on player screen');
      document.querySelector('.back-stations').click();
      await until(() => !!document.querySelector('.compact-player'), 'Bar missing on All Stations');
      const bar = document.querySelector('.compact-player').getBoundingClientRect(),
        sidebar = document.querySelector('aside').getBoundingClientRect();
      assert(Math.abs(bar.left - sidebar.right) < 2, 'Bar extends beneath sidebar');
      assert(
        document.querySelector('[aria-label="Pause"] svg rect'),
        'Pause is not an SVG control',
      );
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
      if(audioBackend==='mpv'){
        const current=await invoke('native_snapshot');
        await invoke('track_details',{key:'a',path:current.path});
      }
      await invoke('native_smoke_report', {
        phase: 'drop',
        ok: true,
        message: 'Testing partial native drop grants and scan notices in the editor',
      });
      await until(() => document.querySelector('dialog')?.open, 'Native drop did not open editor');
      await until(
        () => document.querySelectorAll('.file-row').length === 2,
        'Dropped native audio files missing',
      );
      await until(()=>document.querySelector('.scan-notices')?.textContent.includes('1 entry could not be read'),'Partial native drop lost its scan notice');
      assert(document.querySelector('.scan-notices code')?.textContent.includes('missing-drop-only.wav'),'Native scan notice lost the skipped path');
      const droppedName = document.querySelector('dialog input');
      droppedName.value = 'Native Dropped';
      droppedName.dispatchEvent(new Event('input', { bubbles: true }));
      await wait(30);
      document.querySelector('dialog button[type="submit"]').click();
      await until(
        () => !document.querySelector('dialog'),
        'Native drop grants did not allow saving',
      );
      const droppedStation=(await invoke('load_config')).custom.find(station=>station.station==='Native Dropped');
      assert(droppedStation?.tracks.length===2,'Saved drop station missing');
      let preparedDrop,dropGeneration;
      if(audioBackend==='mpv'){
        const current=await invoke('native_snapshot');
        dropGeneration=current.generation;
        preparedDrop=await invoke('native_prepare',{generation:current.generation,key:droppedStation.key,group:'all'});
      }else await invoke('prepare_station',{key:droppedStation.key,group:'all'});
      const removedPath=droppedStation.tracks[0].path;
      await invoke('track_details',{key:droppedStation.key,path:removedPath});
      const sourceEdit={key:droppedStation.key,name:droppedStation.station,mode:droppedStation.mode,tracks:droppedStation.tracks.slice(1),logoPath:null,options:{crossfade:-1}};
      let badEditRejected=false;
      try{await invoke('save_station',{edit:sourceEdit});}catch{badEditRejected=true;}
      assert(badEditRejected,'Invalid source edit unexpectedly saved');
      await invoke('track_details',{key:droppedStation.key,path:removedPath});
      await invoke('save_station',{edit:{...sourceEdit,options:{}}});
      let revoked=false;
      try{await invoke('track_details',{key:droppedStation.key,path:removedPath});}catch{revoked=true;}
      assert(revoked,'Removed station path retained metadata authority');
      if(audioBackend==='mpv'){
        let staleLoadRejected=false;
        try{await invoke('native_load',{generation:dropGeneration,key:droppedStation.key,slots:preparedDrop.tracks.map(track=>track.slot),index:0,seconds:0,fade:0});}catch{staleLoadRejected=true;}
        assert(staleLoadRejected,'Source edit left the old native lineup loadable');
      }
      const currentPath=audioBackend==='mpv'?(await invoke('native_snapshot')).path:(await invoke('desktop_snapshot')).trackId.split('\0')[1];
      await invoke('track_details',{key:'a',path:currentPath});
      await invoke('native_smoke_report',{phase:'grantInvalidation',ok:true,message:'Appearance/failed edits retain grants; saved source edits revoke only their station namespace'});
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
      await invoke('native_smoke_report',{phase:'unreadableCloseFixture',ok:true,message:'Making the isolated config unreadable before close'});
      let rejectedCloseSetting=false;
      try{await invoke('save_settings',{patch:{closeToTray:false}});}catch{rejectedCloseSetting=true;}
      assert(rejectedCloseSetting,'Unreadable config accepted a settings write');
      await invoke('native_smoke_report', {
        phase: 'closeToTray',
        ok: true,
        message: 'Testing cached tray preference after an unreadable config and rejected save',
      });
      await wait(200);
      assert((await invoke('desktop_snapshot')).playing, 'Closing to tray stopped the player');
      await invoke('native_smoke_report',{phase:'restoreCloseFixture',ok:true,message:'Close retained playback without rereading the config'});
      if(!document.querySelector('[aria-label="Mini player"]'))document.querySelector('.compact-title')?.click();
      await until(()=>document.querySelector('[aria-label="Mini player"]'),'Player PIP button missing');
      document.querySelector('[aria-label="Mini player"]').click();
      await until(
        () => document.querySelector('.now-playing strong')?.textContent === 'Native B',
        'Main player did not receive mini next',
      );
      await wait(800);
      const final = await invoke('desktop_snapshot');
      assert(Math.abs(final.volume - 0.25) < 0.001, 'Mini volume did not persist');
      const settingsReply=await invoke('save_settings', { patch: { closeToTray: false } });
      assert(settingsReply.settings.closeToTray===false&&settingsReply.custom===undefined&&settingsReply.root===undefined,'Settings command returned a whole library instead of its compact update');
      if(audioBackend==='mpv') {
        await invoke('native_smoke_report',{phase:'prepareBumpers',ok:true,message:'Preparing isolated music station and bumper folder'});
        const started=await invoke('native_begin',{delayMs:0});
        const prepared=await invoke('native_prepare',{generation:started.generation,key:'a',group:'all'});
        assert(prepared.tracks.length===2&&prepared.bumpers.length===1,'Bumper folder was not prepared separately');
        assert(Math.abs(prepared.bumpers[0].duration-2)<.1,'Bumper duration was not measured');
        const slots=[...prepared.tracks,...prepared.bumpers].map(t=>t.slot);
        await invoke('native_load',{generation:started.generation,key:'a',slots,index:0,seconds:9.5,fade:1,bumperCount:1,programmeContext:'all',seed:19});
        await until(async()=>{const s=await invoke('native_snapshot');return s.bumper&&s.playing;},'Native IPC never entered bumper');
        let clip=await invoke('native_snapshot');
        assert(clip.index===2&&clip.path.includes(prepared.bumpers[0].name)&&!clip.transitioning,'Bumper overlapped music or used wrong file');
        await invoke('native_control',{control:{type:'pause'}});
        clip=await invoke('native_snapshot');await wait(200);
        const paused=await invoke('native_snapshot');
        assert(!paused.playing&&Math.abs(paused.seconds-clip.seconds)<.1,'Bumper did not pause');
        await invoke('native_control',{control:{type:'play'}});
        await until(async()=>{const s=await invoke('native_snapshot');return !s.bumper&&s.index===1&&s.playing;},'Music queue did not resume after bumper');
        const again=await invoke('native_begin',{delayMs:0});
        const cached=await invoke('native_prepare',{generation:again.generation,key:'a',group:'all'});
        assert(cached.bumpers[0].duration>0,'Bumper duration was not cached');
        const againSlots=[...cached.tracks,...cached.bumpers].map(t=>t.slot);
        await invoke('native_load',{generation:again.generation,key:'a',slots:againSlots,index:0,seconds:0,fade:1,bumperCount:1,programmeContext:'all',seed:19});
        await until(async()=>{const s=await invoke('native_snapshot');return s.index===1&&s.playing;},'Native IPC lost station programme on retune');
        await invoke('native_smoke_report',{phase:'bumpers',ok:true,message:'Folder preparation, cached duration, no-overlap insertion, pause, next song and retune verified through native IPC'});
      }
      if(audioBackend==='mpv'&&await invoke('native_smoke_report',{phase:'networkEnabled',ok:true,message:''})) {
        await invoke('native_smoke_report',{phase:'prepareNetwork',ok:true,message:'Opt-in public network station'});
        const started=await invoke('native_begin',{delayMs:0});
        const prepared=await invoke('native_prepare',{generation:started.generation,key:'a',group:'all'});
        assert(prepared.tracks.length===1&&prepared.tracks[0].duration===null,'Network source was probed as local media');
        const slots=prepared.tracks.map(t=>t.slot);
        let denied=false;try{await invoke('native_load',{generation:started.generation,key:'a',slots:[999],index:0,seconds:0,fade:0});}catch{denied=true;}assert(denied,'Unprepared network index accepted');
        await invoke('native_load',{generation:started.generation,key:'a',slots,index:0,seconds:0,fade:0});
        await until(async()=>{const s=await invoke('native_snapshot');return s.playing&&s.path.startsWith('http');},'Guarded network stream did not play through IPC');
        await invoke('native_control',{control:{type:'pause'}});assert(!(await invoke('native_snapshot')).playing,'Network pause failed');
        await invoke('native_control',{control:{type:'play'}});
        await invoke('native_smoke_report',{phase:'network',ok:true,message:'Prepared saved URL, unknown duration, grant enforcement, guarded HTTP audio and pause/resume verified'});
      }
      if(audioBackend==='mpv'){
        await invoke('native_control',{control:{type:'volume',value:.37}});
        await invoke('native_control',{control:{type:'mute'}});
      }
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
