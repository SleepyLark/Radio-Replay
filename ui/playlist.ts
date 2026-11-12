import type {Track} from './library';

export function trackFolder(track:Track):string {
  const path=track.path.replaceAll('\\','/');
  return path.slice(0,path.lastIndexOf('/')+1)||'Audio files';
}

export function inFolder(track:Track,folder:string):boolean {
  return folder==='Audio files'?trackFolder(track)===folder:track.path.replaceAll('\\','/').startsWith(folder);
}

export function moveFolder(tracks:Track[],folder:string,target:string|undefined,position:'before'|'after'|'first'):Track[] {
  const moving:Track[]=[],remaining:Track[]=[];
  for(const track of tracks)(inFolder(track,folder)?moving:remaining).push(track);
  if(!moving.length)return tracks;
  let at=0;
  if(position!=='first') {
    if(!target)return tracks;
    let found=false;
    for(let index=0;index<remaining.length;index++) {
      if(!inFolder(remaining[index],target))continue;
      at=position==='after'?index+1:index;
      found=true;
      if(position==='before')break;
    }
    if(!found)return tracks;
  }
  // Array literals avoid the argument limit of splice(...moving) for large folders.
  return [...remaining.slice(0,at),...moving,...remaining.slice(at)];
}
