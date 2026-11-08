// Real audio verification using the installed browser runtime.
const {chromium}=require('playwright');
let browser;
const { buildSync } = require('esbuild');
const path = require('node:path');
const timer = setTimeout(() => {
  console.error('Audio smoke timed out');
  process.exit(1);
}, 30000);
(async () => {
    browser=await chromium.launch({channel:process.env.RADIO_REPLAY_BROWSER||'msedge',headless:true,args:['--autoplay-policy=no-user-gesture-required']});
    const page=await browser.newPage();
    const bundle = buildSync({
      entryPoints: [path.join(__dirname, 'audio-browser.mjs')],
      bundle: true,
      write: false,
      format: 'iife',
      platform: 'browser',
    }).outputFiles[0].text;
    await page.goto('data:text/html,<title>Radio Replay audio verification</title>');
    try {
      const result = await page.evaluate(
        bundle + '\nwindow.audioSmoke();',
      );
      console.log(result);
      clearTimeout(timer);
      await browser.close();
      process.exit(0);
    } catch (error) {
      console.error(error);
      clearTimeout(timer);
      await browser.close();
      process.exit(1);
    }
  })()
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });
