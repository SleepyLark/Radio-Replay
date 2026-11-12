import {test} from 'node:test';
import assert from 'node:assert/strict';
import {moveFolder} from '../ui/playlist.ts';

const track=path=>({path,name:path});
const paths=tracks=>tracks.map(track=>track.path);

test('folder moves preserve recording order and do not mutate the original playlist',()=>{
  const tracks=['C:/A/one.mp3','C:/B/one.mp3','C:/A/two.mp3','C:/C/one.mp3','C:/B/two.mp3'].map(track);
  const original=paths(tracks);
  assert.deepEqual(paths(moveFolder(tracks,'C:/B/',undefined,'first')),[original[1],original[4],original[0],original[2],original[3]]);
  assert.deepEqual(paths(moveFolder(tracks,'C:/A/','C:/B/','before')),[original[0],original[2],original[1],original[3],original[4]]);
  assert.deepEqual(paths(moveFolder(tracks,'C:/B/','C:/C/','after')),[original[0],original[2],original[3],original[1],original[4]]);
  assert.deepEqual(paths(tracks),original);
});

test('moving a folder includes subfolders without matching similarly named siblings',()=>{
  const tracks=['C:\\A2\\other.mp3','C:\\A\\one.mp3','C:\\A\\Live\\two.mp3','C:\\B\\last.mp3'].map(track);
  assert.deepEqual(paths(moveFolder(tracks,'C:/A/',undefined,'first')),[tracks[1].path,tracks[2].path,tracks[0].path,tracks[3].path]);
  assert.equal(moveFolder(tracks,'C:/A/','C:/A/Live/','before'),tracks,'Overlapping destinations should not lose tracks');
  assert.equal(moveFolder(tracks,'C:/missing/',undefined,'first'),tracks);
});
