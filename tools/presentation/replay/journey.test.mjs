import test from 'node:test';
import assert from 'node:assert/strict';
import { publicMapView, recordedJourney, replayMap } from './journey.mjs';

const graph = { id: 'map-0', act: 1, boss: 'Hexaghost', nodes: [
  { x: 2, y: 0, room: 'monster', destinations: [[3, 1]] },
  { x: 3, y: 1, room: 'question_mark', destinations: [[3, 2]] },
  { x: 3, y: 2, room: 'boss', destinations: [] },
] };
const sequence = { id: 'model', kind: 'sequence', encounter_id: 'Cultist', maps: [graph], snapshots: [
  { kind: 'combat', location: { act: 1, floor: 37, node: [2, 0], map_id: 'map-0' } },
  { kind: 'card_reward', location: { act: 1, floor: 37, node: [2, 0], map_id: 'map-0' } },
  { kind: 'map', location: { act: 1, floor: 37, node: [2, 0], map_id: 'map-0' } },
  { kind: 'combat', location: { act: 1, floor: 38, node: [3, 1], map_id: 'map-0' } },
] };

test('map projection copies only published room geometry and validates graph references', () => {
  const source = { current: [2, 0], boss: 'Hexaghost', hidden_result: 'not public', node_tuple_fields: ['x', 'y', 'room', 'emerald_key', 'destinations'],
    nodes: [[2, 0, 'monster', false, [[3, 1]]], [3, 1, 'question_mark', true, []]] };
  const projected = publicMapView(source);
  assert.ok(!JSON.stringify(projected).includes('hidden_result'));
  assert.deepEqual(projected.current, [2, 0]);
  assert.equal(projected.nodes[1].room, 'question_mark');
  source.node_tuple_fields.reverse();
  assert.throws(() => publicMapView(source), /tuple layout/);
  source.node_tuple_fields.reverse(); source.nodes[0][4] = [[9, 9]];
  assert.throws(() => publicMapView(source), /map reference/);
});

test('map history contains only observed visits up to the cursor and returns target boundaries', () => {
  const start = replayMap(sequence, 0);
  assert.equal(start.nodes.filter(node => node.target).length, 1);
  assert.deepEqual(start.route, []);
  const later = replayMap(sequence, 3);
  assert.deepEqual(later.route, [['2,0', '3,1']]);
  assert.deepEqual(later.nodes[0].target, { scene_id: 'model', cursor: 0 });
  assert.deepEqual(later.nodes[1].target, { scene_id: 'model', cursor: 3 });
  assert.equal(later.nodes[2].target, undefined);
  assert.equal(later.nodes[1].current, true);
  assert.deepEqual(replayMap(sequence, 0), start);
});

test('recordings keep independent clips separate and group phases using actual floor metadata', () => {
  const manual = structuredClone(sequence); manual.id = 'manual';
  const story = { scenes: [sequence, manual], recordings: [
    { id: 'model-recording', scene_ids: ['model'] }, { id: 'manual-recording', scene_ids: ['manual'] },
  ] };
  const records = recordedJourney(story);
  assert.equal(records.length, 2);
  assert.deepEqual(records[0].visits.map(visit => visit.floor), [37, 38]);
  assert.deepEqual(records[0].visits[0].node, [2, 0]);
  assert.deepEqual(records[0].visits[0].phases.map(phase => phase.kind), ['combat', 'reward', 'map']);
  assert.equal(records[1].visits[0].phases[0].scene_id, 'manual');
  story.recordings[1].scene_ids = ['model'];
  assert.throws(() => recordedJourney(story), /duplicate recording/);
});

test('different floors and acts at the same map coordinate remain separate visits', () => {
  const clip = structuredClone(sequence);
  clip.snapshots[1].location.floor = 38;
  clip.snapshots[2].location.act = 2;
  const visits = recordedJourney({ scenes: [clip], recordings: [{ id: 'one', scene_ids: ['model'] }] })[0].visits;
  assert.equal(visits.length, 4);
  delete clip.snapshots[1].location.floor;
  assert.throws(() => recordedJourney({ scenes: [clip], recordings: [{ id: 'one', scene_ids: ['model'] }] }), /visit location/);
});
