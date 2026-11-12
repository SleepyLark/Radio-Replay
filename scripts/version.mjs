import {readFileSync, writeFileSync, appendFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const read=file=>readFileSync(path.join(root,file),'utf8');
const jsonFiles=['package.json','package-lock.json','src-tauri/tauri.conf.json'];
const manifests=['crates/core/Cargo.toml','crates/audio/Cargo.toml','src-tauri/Cargo.toml'];
const crates=['radio-replay-core','radio-replay-audio','radio-replay-desktop'];
const manifestVersion=text=>text.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const lockBlocks=read('Cargo.lock').split('[[package]]');
const crateBlock=name=>lockBlocks.find(block=>block.match(/^name\s*=\s*"([^"]+)"/m)?.[1]===name);

try {
  if(process.argv[2]==='--set') {
    const version=process.argv[3];
    if(!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/.test(version??''))
      throw new Error('Use --set <version>, for example --set 0.6.6 or --set 0.7.0-beta.1');
    const pending=new Map();
    for(const file of jsonFiles) {
      const data=JSON.parse(read(file));
      data.version=version;
      if(file==='package-lock.json')data.packages[''].version=version;
      pending.set(file,JSON.stringify(data,null,2)+'\n');
    }
    for(const file of manifests) {
      const text=read(file);
      if(!manifestVersion(text))throw new Error(`Missing package version in ${file}`);
      pending.set(file,text.replace(/^version\s*=\s*"[^"]+"/m,`version = "${version}"`));
    }
    for(const name of crates) {
      const block=crateBlock(name);
      if(!block||!manifestVersion(block))throw new Error(`Missing ${name} in Cargo.lock`);
      lockBlocks[lockBlocks.indexOf(block)]=block.replace(/^version\s*=\s*"[^"]+"/m,`version = "${version}"`);
    }
    pending.set('Cargo.lock',lockBlocks.join('[[package]]'));
    // Prepare every change before writing; the resulting edits can be reviewed in Git.
    for(const [file,text] of pending)writeFileSync(path.join(root,file),text);
  } else if(process.argv[2]&&process.argv[2]!=='--tag') {
    throw new Error('Usage: node scripts/version.mjs [--set <version> | --tag <tag>]');
  }

  const version=JSON.parse(read('package.json')).version;
  if(!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?$/.test(version))throw new Error(`Invalid project version: ${version}`);
  const versions=new Map(jsonFiles.map(file=>[file,JSON.parse(read(file)).version]));
  versions.set('package-lock.json root package',JSON.parse(read('package-lock.json')).packages[''].version);
  for(const file of manifests)versions.set(file,manifestVersion(read(file)));
  for(const name of crates)versions.set(`Cargo.lock ${name}`,manifestVersion(crateBlock(name)??''));
  for(const [file,actual] of versions)if(actual!==version)throw new Error(`${file}: expected ${version}, found ${actual}`);
  const tag=process.argv[2]==='--tag'?process.argv[3]:process.env.GITHUB_REF_TYPE==='tag'?process.env.GITHUB_REF_NAME:undefined;
  if(process.argv[2]==='--tag'&&!tag)throw new Error('--tag requires a tag such as v0.6.5');
  if(tag&&tag!==`v${version}`)throw new Error(`Tag ${tag} does not match project version v${version}`);
  console.log(`All project versions agree: ${version}${tag?` (${tag})`:''}`);
  if(process.env.GITHUB_OUTPUT)appendFileSync(process.env.GITHUB_OUTPUT,`version=${version}\n`);
} catch(error) {
  console.error(error instanceof Error?error.message:String(error));
  process.exitCode=1;
}
