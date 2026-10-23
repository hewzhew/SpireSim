import test from 'node:test';
import assert from 'node:assert/strict';
import { cardGroups, handTransition, recordedSequence, sequenceView } from './sequence.mjs';
import { cardFaceView } from './card-face.mjs';

const card = id => ({ card_id: id, upgrades: 0, effective_cost: 1, current_block: id === 'ShrugItOff' ? 8 : id === 'Defend' ? 5 : 0,
  current_magic_number: id === 'ShrugItOff' ? 1 : 0, current_damage: id === 'Strike' ? 6 : 0, damage_by_monster_order: id === 'Strike' ? [9] : [] });
const definitions = ['Strike', 'Defend', 'ShrugItOff'].map(id => ({ kind: 'card', definition: {
  current: { card: id, card_type: id === 'Strike' ? 'Attack' : 'Skill', damage: { damage_per_hit: id === 'Strike' ? 6 : 0, hit_count: id === 'Strike' ? 1 : 0 },
    block: id === 'Strike' ? 0 : id === 'ShrugItOff' ? 8 : 5, draw_cards: id === 'ShrugItOff' ? 1 : 0, vulnerable: 0, ethereal: false },
}, face_templates: [{ upgrades: 0, field: 'DESCRIPTION', damage: id === 'Strike' ? 6 : 0, block: id === 'Defend' ? 5 : id === 'ShrugItOff' ? 8 : 0, magic: id === 'ShrugItOff' ? 1 : 0 }] }));
const rewardDefinitions = [
  { card: 'ThunderClap', card_type: 'Attack', cost: 1, damage: { damage_per_hit: 4 }, vulnerable: 1 },
  { card: 'Carnage', card_type: 'Attack', cost: 2, damage: { damage_per_hit: 20 }, ethereal: true },
  { card: 'Warcry', card_type: 'Skill', cost: 0, draw_cards: 1 },
].map(current => ({ kind: 'card', definition: { current }, face_templates: [{ upgrades: 0, field: 'DESCRIPTION', damage: current.damage?.damage_per_hit ?? 0, block: 0, magic: current.draw_cards ?? current.vulnerable ?? 0 }] }));
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
  return [{ action: 'play:ShrugItOff+0#0', before, after: middle, source_revision: 'source', operator: 'developer' },
    { action: 'play:Defend+0#0', before: middle, after, source_revision: 'source', operator: 'developer' }];
}

test('an action can use a card that entered the hand at an earlier recorded boundary', () => {
  const scene = recordedSequence(recording(), definitions, 'draw');
  assert.equal(scene.actions[1].card.card_id, 'Defend');
  assert.equal(scene.actions[1].hand_index, 1);
  assert.equal(scene.snapshots[0].hand.some(c => c.card_id === 'Defend'), false);
  assert.deepEqual(sequenceView(scene, 1).added, [{ card_id: 'Defend', upgrades: 0, count: 1 }]);
  assert.deepEqual(sequenceView(scene, 1).deltas, { energy: -1, block: 8, hp: 0, enemy_hp: 0 });
  assert.equal(scene.snapshots[0].hand[1].printed_lines.en[0], 'Deal 9 damage.');
  assert.deepEqual(scene.snapshots[0].hand[0].printed_lines.en, ['Gain 8 Block.', 'Draw 1 card.']);
  assert.equal(scene.snapshots[0].turn, 4);
});

test('hand movement removes the recorded duplicate slot and preserves the surviving public order', () => {
  assert.deepEqual(handTransition([card('Strike'), card('Defend'), card('Strike')], [card('Strike'), card('Defend')], 2),
    { played: 2, retained: [{ from: 0, to: 0 }, { from: 1, to: 1 }], added: [] });
  assert.deepEqual(handTransition([card('ShrugItOff'), card('Strike')], [card('Strike'), card('Defend')], 0),
    { played: 0, retained: [{ from: 1, to: 0 }], added: [1] });
});

test('drawing an existing card kind still produces a new public hand slot', () => {
  assert.deepEqual(handTransition([card('ShrugItOff'), card('Strike')], [card('Strike'), card('Strike')], 0),
    { played: 0, retained: [{ from: 1, to: 0 }], added: [1] });
});

test('unmatched hand changes do not fabricate movement between card instances', () => {
  assert.equal(handTransition([card('ShrugItOff'), card('Strike'), card('Defend')], [card('Defend'), card('Strike')], 0), null);
  assert.equal(handTransition([card('ShrugItOff'), card('Strike')], [], 0), null);
  assert.equal(handTransition([card('Strike')], [], -1), null);
  assert.equal(handTransition([card('Strike')], [], 1), null);
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

test('a recorded model battle crosses end-turn and reward boundaries without inventing a final enemy state', () => {
  const first = frame(0, ['Strike'], ['Defend'], 'end');
  first.position.actions[0].candidate.action = { kind: 'end_turn' };
  const next = frame(1, ['Strike'], ['Defend'], 'play:Strike+0#0@0');
  const reward = { session_id: 'recording', head_revision: 2, capabilities: { permission_envelope: { information_ceiling: 'public_only' } },
    position: { kind: 'decision', public_snapshot_id: 'snapshot-2', actions: [{ semantic_action: 'claim:Gold#0', candidate: {
      domain: 'strategic', action: { kind: 'claim_reward', reward: { kind: 'gold', amount: 10 } } } }],
      observation: { domain: 'strategic', observation: { run: { current_hp: 60, max_hp: 80, gold: 100 }, cards: [{ card: 'Strike', count: 1 }], relics: [],
        context: { kind: 'reward', pending_rewards: [{ kind: 'gold', amount: 10 }] } } } } };
  const map = structuredClone(reward); map.head_revision = 3; map.position.public_snapshot_id = 'snapshot-3';
  map.position.observation.observation.run.gold = 110; map.position.observation.observation.context = { kind: 'map' };
  const rows = [{ before: first, after: next, action: 'end' }, { before: next, after: reward, action: 'play:Strike+0#0@0' },
    { before: reward, after: map, action: 'claim:Gold#0' }].map(row => ({ ...row, operator: 'model', source_revision: 'source' }));
  const scene = recordedSequence(rows, definitions, 'model');
  assert.equal(scene.provenance.operator, 'model');
  assert.equal(scene.actions[0].kind, 'end_turn');
  assert.equal(scene.snapshots[2].kind, 'reward');
  assert.equal(scene.snapshots[2].reward_gold, 10);
  assert.ok(!('enemy' in scene.snapshots[2]));
  assert.ok(!('hand' in scene.snapshots[2]));
  assert.equal(scene.snapshots[3].gold, 110);
  assert.ok(Object.values(sequenceView(scene, 2).deltas).every(Number.isFinite));
  rows[1].operator = 'developer';
  assert.throws(() => recordedSequence(rows, definitions, 'mixed'), /operators/);
});

test('fresh reward faces retain official target, lifecycle and selection text in both languages', () => {
  const faces = rewardDefinitions.map(({ definition: { current } }) => cardFaceView({ card_id: current.card, upgrades: 0 }, rewardDefinitions));
  assert.deepEqual(faces.map(card => card.cost), [1, 2, 0]);
  assert.deepEqual(faces.map(card => card.type), ['attack', 'attack', 'skill']);
  assert.equal(faces[0].printed_lines.en[0], 'Deal 4 damage and apply 1 Vulnerable to ALL enemies.');
  assert.equal(faces[0].printed_lines['zh-CN'][0], '对所有敌人造成 4 点伤害，给予1层易伤。');
  assert.deepEqual(faces[1].printed_lines.en, ['Ethereal.', 'Deal 20 damage.']);
  assert.deepEqual(faces[2].printed_lines.en, ['Draw 1 card.', 'Put a card from your hand onto the top of your draw pile.', 'Exhaust.']);
  assert.ok(faces[2].printed_lines['zh-CN'][1].includes('抽牌堆顶部'));
  assert.throws(() => cardFaceView({ card_id: 'Carnage', upgrades: 0 }, []), /Missing captured/);
  assert.throws(() => cardFaceView({ card_id: 'Warcry', upgrades: 1 }, rewardDefinitions), /Missing captured/);
});

test('reward offers become full faces only at their recorded selection boundary', () => {
  const before = { session_id: 'reward', head_revision: 0, capabilities: { permission_envelope: { information_ceiling: 'public_only' } },
    position: { kind: 'decision', public_snapshot_id: 'r0', actions: [{ semantic_action: 'open-card#0', candidate: { domain: 'strategic', action: { kind: 'open_card_reward' } } }],
      observation: { domain: 'strategic', run_context: {}, observation: { run: { current_hp: 60, max_hp: 80, gold: 110 }, relics: [], cards: [],
        context: { kind: 'reward', pending_rewards: [{ kind: 'card_reward', cards: rewardDefinitions.map(({ definition: { current } }) => ({ card: current.card, upgrades: 0 })) }] } } } } };
  const after = structuredClone(before); after.head_revision = 1; after.position.public_snapshot_id = 'r1';
  after.position.observation.observation.context.kind = 'card_reward';
  const scene = recordedSequence([{ before, after, action: 'open-card#0', operator: 'model', source_revision: 'source' }], rewardDefinitions, 'reward');
  assert.deepEqual(scene.snapshots[0].reward_cards, []);
  assert.deepEqual(scene.snapshots[1].reward_cards.map(card => card.card_id), ['ThunderClap', 'Carnage', 'Warcry']);
  assert.ok(scene.snapshots[1].reward_cards.every(card => card.printed_lines.en.length));
  assert.equal(sequenceView(scene, 0).completed.length, 0);
});

test('reward alternatives follow legal candidates and native resource evidence, including blocked healing', () => {
  const before = { session_id: 'reward', head_revision: 0, capabilities: { permission_envelope: { information_ceiling: 'public_only' } },
    position: { kind: 'decision', public_snapshot_id: 'r0', actions: [
      { semantic_action: 'skip-card#0', candidate: { domain: 'strategic', action: { kind: 'skip_card_reward' } } },
      { semantic_action: 'singing-bowl#0', candidate: { domain: 'strategic', action: { kind: 'singing_bowl' } } },
    ], observation: { domain: 'strategic', run_context: {}, observation: {
      run: { act: 1, floor: 2, current_hp: 60, max_hp: 80, gold: 110 }, relics: [], cards: [], context: { kind: 'card_reward', pending_rewards: [] },
    } } }, decision_evidence: { resource_options: [{ semantic_action: 'singing-bowl#0', max_hp_gain: 2, hp_gain: 0 }] } };
  const after = structuredClone(before); after.head_revision = 1; after.position.public_snapshot_id = 'r1';
  after.position.observation.observation.run.max_hp = 82; after.position.observation.observation.context.kind = 'reward';
  const rows = [{ before, after, action: 'singing-bowl#0', operator: 'model', source_revision: 'source' }];
  const scene = recordedSequence(rows, [], 'bowl');
  assert.deepEqual(scene.snapshots[0].reward_alternatives, [
    { kind: 'skip_card_reward', command: 'skip-card#0' }, { kind: 'singing_bowl', command: 'singing-bowl#0', max_hp_gain: 2 },
  ]);
  assert.equal(scene.actions[0].max_hp_gain, 2);
  assert.equal(scene.snapshots[1].hp, 60);
  assert.deepEqual(scene.snapshots[1].reward_alternatives, []);
  delete before.decision_evidence;
  assert.throws(() => recordedSequence(rows, [], 'bowl'), /Singing Bowl resource evidence/);
});
