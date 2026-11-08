import test from 'node:test';
import assert from 'node:assert/strict';
import { stationAccent } from '../ui/appearance.ts';

test('Electron accent priority: station override, banner match, global default', () => {
  const settings = { accent: '#123456' };
  assert.equal(stationAccent({ accent: '#abcdef', bannerColor: '#555555' }, settings), '#abcdef');
  assert.equal(stationAccent({ bannerColor: '#555555' }, settings), '#555555');
  assert.equal(
    stationAccent({ bannerPath: 'banner.png', bannerAccent: '#888888' }, settings),
    '#888888',
  );
  assert.equal(stationAccent({ bannerPath: 'banner.png' }, settings, '#777777'), '#777777');
  assert.equal(
    stationAccent({ bannerColor: '#555555' }, { ...settings, matchBannerAccent: false }),
    '#123456',
  );
  assert.equal(stationAccent({ accent: 'invalid', bannerColor: 'invalid' }, settings), '#123456');
  assert.equal(stationAccent(undefined, {}), '#c7f464');
});
