import test from 'node:test';
import assert from 'node:assert/strict';
import { cardGroups, recordedSequence, sequenceView } from './sequence.mjs';

const card = id => ({ card_id: id, upgrades: 0, effective_cost: 1, current_block: id === 'ShrugItOff' ? 8 : id === 'Defend' ? 5 : 0,
  current_damage: id === 'Strike' ? 6 : 0, damage_by_monster_order: id === 'Strike' ? [9] : [] });
const definitions = ['Strike', 'Defend', 'ShrugItOff'].map(id => ({ kind: 'card', definition: {
  current: { card: id, card_type: id === 'Strike' ? 'Attack' : 'Skill', damage: { damage_per_hit: id === 'Strike' ? 6 : 0, hit_count: id === 'Strike' ? 1 : 0 },
    block: id === 'Strike' ? 0 : id === 'ShrugItOff' ? 8 : 5, draw_cards: id === 'ShrugItOff' ? 1 : 0, vulnerable: 0, ethereal: false },
} }));
function frame(revision, hand, draw, command, handIndex = 0) {
  const zone = cards => ({ evidence: 'public_unordered_collection', cards: cards.map(card) });
  return { session_id: 'recording', head_revision: revision, position: { kind: 'decision', public_snapshot_id: 'snapshot-' + revision,
    actions: command ? [{ semantic_action: command, candidate: { domain: 'combat_atomic', action: { kind: 'play_card', hand_index: handIndex, target_monster_index: null } } }] : [],
    observation: { domain: 'combat', run_context: { encounter_id: 'Cultist', floor: 2 }, observation: {
      information_access: 'public', private_rng: 'must never enter the viewer',
      player: { hp: 60, max_hp: 80, gold: 100, block: revision ? 8 : 0, relics: [], powers: [] }, turn: { turn_count: 3, energy: 3 - revision },
      cards: { hand: zone(hand), draw: zone(draw), discard: zone([]), exhaust: zone([]) },
      monsters: [{ enemy: { status: 'known', enemy_id: 'Cultist' }, hp: 25, max_hp: 51, block: 0, powers: [],
        intent: { intent: { Attack: { damage: 6, hits: 1 } }, preview_damage_per_hit: 6 } }],
    } },
  } };
}
function recording() {
  const before = frame(0, ['ShrugItOff', 'Strike'], ['Defend', 'Strike'], 'play:ShrugItOff+0#0');
  const middle = frame(1, ['Strike', 'Defend'], ['Strike'], 'play:Defend+0#0', 1);
  const after = frame(2, ['Strike'], ['Strike']);
  return [{ action: 'play:ShrugItOff+0#0', before, after: middle, source_revision: 'source' },
    { action: 'play:Defend+0#0', before: middle, after, source_revision: 'source' }];
}

test('an action can use a card that entered the hand at an earlier recorded boundary', () => {
  const scene = recordedSequence(recording(), definitions, 'draw');
  assert.equal(scene.actions[1].card.card_id, 'Defend');
  assert.equal(scene.snapshots[0].hand.some(c => c.card_id === 'Defend'), false);
  assert.deepEqual(sequenceView(scene, 1).added, [{ card_id: 'Defend', upgrades: 0, count: 1 }]);
  assert.deepEqual(sequenceView(scene, 1).deltas, { energy: -1, block: 8, hp: 0, enemy_hp: 0 });
  assert.equal(scene.snapshots[0].hand[1].effects[0].damage, 9);
  assert.deepEqual(scene.snapshots[0].hand[0].effects, [{ kind: 'block', block: 8 }, { kind: 'draw', count: 1 }]);
  assert.equal(scene.snapshots[0].turn, 4);
});

test('an earlier cursor contains neither later actions nor their newly revealed hand', () => {
  const scene = recordedSequence(recording(), definitions, 'draw');
  const earlier = structuredClone(sequenceView(scene, 0));
  assert.deepEqual(earlier.completed, []);
  assert.deepEqual(earlier.added, []);
  scene.snapshots[1].hand.push(card('Bash'));
  scene.actions[1].command = 'future action';
  assert.deepEqual(sequenceView(scene, 0), earlier);
  assert.throws(() => sequenceView(scene, -1), /cursor/);
  assert.throws(() => sequenceView(scene, 3), /cursor/);
});

test('unordered piles group duplicates and ignore supplied array order', () => {
  assert.deepEqual(cardGroups([card('Strike'), card('Defend'), card('Strike')]), cardGroups([card('Strike'), card('Strike'), card('Defend')]));
  const entries = recording(), first = recordedSequence(entries, definitions, 'draw');
  entries[0].before.position.observation.observation.cards.draw.cards.reverse();
  const second = recordedSequence(entries, definitions, 'draw');
  assert.deepEqual(first.snapshots[0].piles.draw, second.snapshots[0].piles.draw);
  assert.equal(first.snapshots[0].piles.draw.ordered, false);
});

test('only a public ordered collection retains its supplied order', () => {
  const entries = recording(), pile = entries[0].before.position.observation.observation.cards.draw;
  pile.evidence = 'public_ordered_collection';
  const scene = recordedSequence(entries, definitions, 'draw');
  assert.equal(scene.snapshots[0].piles.draw.ordered, true);
  assert.deepEqual(scene.snapshots[0].piles.draw.cards.map(c => c.card_id), ['Defend', 'Strike']);
  pile.evidence = 'hidden';
  assert.throws(() => recordedSequence(entries, definitions, 'draw'), /Unobservable pile/);
});

test('source identities bind each action to the actual preceding public boundary', () => {
  const entries = recording();
  entries[1].source_revision = 'another-producer';
  assert.throws(() => recordedSequence(entries, definitions, 'draw'), /Disconnected/);
  entries[1].source_revision = 'source';
  entries[1].before = structuredClone(entries[1].before);
  entries[1].before.position.public_snapshot_id = 'another-branch';
  assert.throws(() => recordedSequence(entries, definitions, 'draw'), /Disconnected/);
  entries[1].before = entries[0].after;
  entries[1].after.session_id = 'another-session';
  assert.throws(() => recordedSequence(entries, definitions, 'draw'), /Disconnected/);
  delete entries[0].source_revision;
  assert.throws(() => recordedSequence(entries, definitions, 'draw'), /source revision/);
});

test('the presentation copies public fields instead of spreading the source observation', () => {
  const entries = recording();
  assert.ok(!JSON.stringify(recordedSequence(entries, definitions, 'draw')).includes('private_rng'));
  entries[0].before.position.observation.observation.information_access = 'privileged_exact';
  assert.throws(() => recordedSequence(entries, definitions, 'draw'), /public information/);
});
