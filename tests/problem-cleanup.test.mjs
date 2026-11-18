import {test} from 'node:test';
import assert from 'node:assert/strict';
import {removableProblemPaths} from '../ui/notifications.ts';
test('problem cleanup selects saved file failures only, without using the dialog search',()=>{
  const config={custom:[{key:'a',tracks:[{path:'bad.mp3'},{path:'cache.mp3'},{path:'good.mp3'}]},{key:'b',tracks:[{path:'other.mp3'}]}]};
  const notification={stationKey:'a',problems:[{kind:'file',path:'bad.mp3'},{kind:'file',path:'bad.mp3'},{kind:'cache',path:'cache.mp3'},{kind:'scan',path:'good.mp3'},{kind:'file',path:'other.mp3'},{kind:'scan',path:'bumper-folder'}]};
  assert.deepEqual(removableProblemPaths(notification,config),['bad.mp3']);
  assert.deepEqual(removableProblemPaths({...notification,stationKey:'deleted'},config),[]);
});
