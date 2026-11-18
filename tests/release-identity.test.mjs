import {test} from 'node:test';
import assert from 'node:assert/strict';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

test('tag validation rejects feature identities while permitting isolated development builds',()=>{
  const output=fileURLToPath(new URL('../out/',import.meta.url));
  mkdirSync(output,{recursive:true});
  const root=mkdtempSync(path.join(output,'release-identity-'));
  const version='0.6.6';
  const write=(file,value)=>{const target=path.join(root,file);mkdirSync(path.dirname(target),{recursive:true});writeFileSync(target,value);};
  try{
    write('scripts/version.mjs',readFileSync(new URL('../scripts/version.mjs',import.meta.url)));
    write('package.json',JSON.stringify({version}));
    write('package-lock.json',JSON.stringify({version,packages:{'':{version}}}));
    const crates=['core','audio','network','desktop'];
    for(const name of crates)write(name==='desktop'?'src-tauri/Cargo.toml':`crates/${name}/Cargo.toml`,`version = "${version}"\n`);
    write('Cargo.lock',crates.map(name=>`[[package]]\nname = "radio-replay-${name}"\nversion = "${version}"\n`).join(''));
    const run=(...args)=>spawnSync(process.execPath,[path.join(root,'scripts/version.mjs'),...args],{encoding:'utf8',env:{...process.env,GITHUB_OUTPUT:'',GITHUB_REF_TYPE:'',GITHUB_REF_NAME:''}});
    for(const suffix of ['network-preview','playlists-preview','bumpers-preview','mpv-preview']){
      write('src-tauri/tauri.conf.json',JSON.stringify({version,identifier:`com.local.radioreplay.${suffix}`}));
      const development=run();
      assert.equal(development.status,0,development.stderr||development.stdout);
      const tagged=run('--tag',`v${version}`);
      assert.equal(tagged.status,1);
      assert.match(tagged.stderr,/Release identifier must remain/);
    }
    write('src-tauri/tauri.conf.json',JSON.stringify({version,identifier:'com.local.radioreplay.preview'}));
    assert.equal(run('--tag',`v${version}`).status,0);
  }finally{assert.equal(path.dirname(root),path.resolve(output));rmSync(root,{recursive:true,force:true});}
});
