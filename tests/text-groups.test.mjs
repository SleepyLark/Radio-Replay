import test from 'node:test';
import assert from 'node:assert/strict';
import { parseTimedText, currentLine, groupLineup, StationHistory } from '../ui/playback/text.ts';

test('timed text handles BOM, multiple timestamps, long minutes, blank lines and malformed input', () => {
  const lines = parseTimedText(
    '\uFEFF[00:02.50][00:01]Repeated\n[120:00]Long recording\n[00:70]Invalid\n[01:02:03]Unsupported\n[00:03]\nuntimed',
  );
  assert.deepEqual(lines, [
    { at: 1, text: 'Repeated' },
    { at: 2.5, text: 'Repeated' },
    { at: 3, text: '' },
    { at: 7200, text: 'Long recording' },
  ]);
  assert.equal(currentLine(lines, 0.5), -1);
  assert.equal(currentLine(lines, 2.5), 1);
  assert.equal(currentLine(lines, 7201), 3);
  assert.equal(parseTimedText('[00:01]line\n'.repeat(20001)).length, 20000);
});
test('groups include full stations or individually selected tracks', () => {
  const stations = [
    { key: 'a', tracks: [{ path: 'first.mp3' }, { path: 'second.mp3' }] },
    { key: 'b', tracks: [{ path: 'last.wav' }] },
  ];
  assert.deepEqual(groupLineup(stations, { members: ['a|second.mp3'] }), [stations[0]]);
  assert.deepEqual(groupLineup(stations, { members: ['b'] }), [stations[1]]);
  assert.deepEqual(groupLineup(stations, { members: [] }), []);
});
test('random station history retraces and replays successful visits', () => {
  const stations = ['a', 'b', 'c'].map((key) => ({ key }));
  const history = new StationHistory();
  history.commit('a');
  history.commit('b');
  history.commit('c');
  const previous = history.next(stations, -1);
  assert.equal(previous.key, 'b');
  history.commit(previous.key);
  assert.equal(history.next(stations, 1).key, 'c');
  assert.equal(history.next(stations, -1).key, 'a');
});
test('randomly returning to an earlier station creates a fresh visit', () => {
  const history = new StationHistory();
  const lineup = ['a', 'b'].map((key) => ({ key }));
  history.commit('a');
  history.commit('b');
  const next = history.next(lineup, 1, () => 0);
  history.commit(next.key);
  assert.deepEqual(history.keys, ['a', 'b', 'a']);
});
