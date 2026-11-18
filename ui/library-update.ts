import type {Config,Station,Group} from './library';
export interface LibraryUpdate {
  settings?:Record<string,unknown>;
  stations?:Station[];
  removedStations?:string[];
  groups?:Group[];
  removedGroups?:string[];
  logos?:{key:string;removed:boolean;value:unknown}[];
}
function merge<T>(current:T[],updates:T[],removed:Set<string>,key:(value:T)=>string):T[] {
  const replacements=new Map(updates.map(value=>[key(value),value]));
  const result=current.filter(value=>!removed.has(key(value))).map(value=>{
    const id=key(value),next=replacements.get(id);replacements.delete(id);return next??value;
  });
  return [...result,...replacements.values()];
}
// Merge into the current library at response time, keeping unrelated objects and
// unknown fields. A late settings reply cannot replace a newer station/group.
export function applyLibraryUpdate(config:Config,change:LibraryUpdate):Config {
  const next={...config};
  if(change.settings!==undefined)next.settings=change.settings;
  const removed=new Set(change.removedStations??[]);
  if(change.stations?.length||removed.size)next.custom=merge(config.custom,change.stations??[],removed,s=>s.key);
  if(change.groups?.length||change.removedGroups?.length)next.groups=merge(config.groups,change.groups??[],new Set(change.removedGroups??[]),g=>g.id);
  if(change.logos?.length||removed.size){
    next.logos={...config.logos};
    for(const key of removed)delete next.logos[key];
    for(const logo of change.logos??[]){if(logo.removed)delete next.logos[logo.key];else Object.defineProperty(next.logos,logo.key,{value:logo.value,writable:true,configurable:true,enumerable:true});}
  }
  if(removed.size)next.hidden=config.hidden.filter(key=>!removed.has(key));
  return next;
}
