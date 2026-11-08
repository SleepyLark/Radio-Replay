import test from 'node:test';
import assert from 'node:assert/strict';
import { tuningMilliseconds } from '../ui/playback/tuning.ts';
test('tuning delays honor disable, bounds and reversed imported ranges', () => {
  assert.equal(
    tuningMilliseconds({ tuningTransition: false }, () => 0.5),
    0,
  );
  assert.equal(
    tuningMilliseconds({ tuningMinMs: 100, tuningMaxMs: 200 }, () => 0.5),
    150,
  );
  assert.equal(
    tuningMilliseconds({ tuningMinMs: 300, tuningMaxMs: 100 }, () => 0),
    100,
  );
  assert.equal(
    tuningMilliseconds({ tuningMinMs: -10, tuningMaxMs: 9000 }, () => 0),
    0,
  );
});
