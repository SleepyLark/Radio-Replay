// Validate actual build artifacts, including the native packaging boundary.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const mode=process.argv[2];assert.ok(['native','web'].includes(mode),'Pass native or web');
const dist=path.resolve(__dirname,'../dist'),assets=path.join(dist,'assets');
const names=fs.readdirSync(assets),scripts=names.filter(name=>name.endsWith('.js'));
const source=scripts.map(name=>fs.readFileSync(path.join(assets,name),'utf8'));
const html=fs.readFileSync(path.join(dist,'index.html'),'utf8');
const src=html.match(/<script\b[^>]*\bsrc="([^"]+)"/)[1];
const entry=fs.readFileSync(path.join(dist,src.replace(/^\//,'')),'utf8');
assert.ok(!entry.includes('AudioContext'),'Legacy decoding must not be in the startup chunk');
const media=names.filter(name=>name.endsWith('.mp3'));
if(mode==='native'){
  assert.equal(media.length,0,'Native frontend duplicated the Rust-embedded static sound');
  assert.ok(source.every(text=>!text.includes('AudioContext')),'Native package contains the legacy audio engine');
}else{
  assert.ok(media.length>0,'Web comparison lost its bundled tuning sound');
  assert.ok(source.some(text=>text.includes('AudioContext')),'Web comparison lost its audio engine');
}
console.log(`${mode} frontend passed: ${scripts.length} script chunks, ${media.length} browser MP3 assets; legacy engine excluded from startup.`);
