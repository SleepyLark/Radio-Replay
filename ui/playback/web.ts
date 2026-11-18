// Loaded only when the backend explicitly selects the Web Audio comparison.
import {Playback,type PlaybackState} from './engine';
import {prepareStation,rememberDuration,audioUrl,resolveTuningSound,type AudioLevels} from '../library';
import bundledStatic from '../../assets/audio/static.mp3?url';

export function createWebPlayback(onState:(state:PlaybackState)=>void,group:()=>string,levels:AudioLevels) {
  return new Playback({levels,prepare:key=>prepareStation(key,group()),rememberDuration,url:audioUrl,
    tuningSource:async value=>{
      if(value===''||value==='off')return null;
      if(typeof value!=='string'||value==='bundled')return bundledStatic;
      const path=await resolveTuningSound();return path?audioUrl(path):null;
    },onState});
}
