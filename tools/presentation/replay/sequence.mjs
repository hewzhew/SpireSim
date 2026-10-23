// Project recorded public observations. No game execution or private state.
import { cardFaceView } from './card-face.mjs';
import { publicMapView } from './journey.mjs';

const snake = value => value.replace(/[A-Z]/g, (letter, index) => (index ? '_' : '') + letter.toLowerCase());
const key = card => JSON.stringify([card.card_id, card.upgrades ?? 0]);

export function cardGroups(cards) {
  const groups = new Map();
  for (const card of cards) {
    const id = key(card);
    const group = groups.get(id) ?? { card_id: card.card_id, upgrades: card.upgrades ?? 0, count: 0 };
    group.count++; groups.set(id, group);
  }
  return [...groups.values()].sort((a, b) => key(a).localeCompare(key(b), 'en'));
}

function boundary(frame) {
  const view = frame.position?.observation;
  if (typeof frame.session_id !== 'string' || !frame.session_id || !Number.isSafeInteger(frame.head_revision)
      || typeof frame.position?.public_snapshot_id !== 'string' || !frame.position.public_snapshot_id) throw new Error('Missing recorded boundary identity');
  if (frame.position?.kind !== 'decision' || !['combat', 'strategic'].includes(view?.domain)) throw new Error('A sequence boundary must be a recorded public decision');
  if (frame.capabilities?.permission_envelope?.information_ceiling !== undefined
      && frame.capabilities.permission_envelope.information_ceiling !== 'public_only') throw new Error('A sequence requires public information');
  if (view.domain === 'strategic' && frame.capabilities?.permission_envelope?.information_ceiling !== 'public_only') throw new Error('A sequence requires public strategic evidence');
  if (view.domain === 'combat' && view.observation.information_access !== 'public') throw new Error('A sequence requires public information');
  return view.observation;
}

function cardView(card, definitions) {
  return cardFaceView(card, definitions, true);
}

function snapshot(frame, definitions, maps) {
  const state = boundary(frame);
  const context = frame.position.observation.run_context;
  const run = frame.position.observation.domain === 'strategic' ? state.run : context;
  const map = publicMapView(state.public_map ?? context?.public_map);
  const location = { act: run?.act, floor: run?.floor };
  if (map) {
    const graph = { act: run.act, boss: map.boss, nodes: map.nodes };
    let captured = maps.find(item => JSON.stringify(item.graph) === JSON.stringify(graph));
    if (!captured) { captured = { id: 'map-' + maps.length, graph }; maps.push(captured); }
    Object.assign(location, { map_id: captured.id, node: map.current });
  }
  const source = { session_id: frame.session_id, head_revision: frame.head_revision, public_snapshot_id: frame.position.public_snapshot_id };
  if (frame.position.observation.domain === 'strategic') {
    if (!['reward', 'card_reward', 'map', 'event'].includes(state.context.kind)) throw new Error('Unbound strategic scene');
    const rewards = state.context.pending_rewards ?? [];
    if (rewards.some(reward => !['gold', 'card_reward', 'potion', 'relic'].includes(reward.kind))) throw new Error('Unbound recorded reward');
    const reward_alternatives = state.context.kind === 'card_reward' ? frame.position.actions.flatMap(candidate => {
      if (candidate.candidate.domain !== 'strategic') return [];
      const kind = candidate.candidate.action.kind;
      if (kind === 'skip_card_reward') return [{ kind, command: candidate.semantic_action }];
      if (kind !== 'singing_bowl') return [];
      const evidence = frame.decision_evidence?.resource_options?.find(option => option.semantic_action === candidate.semantic_action);
      if (!Number.isSafeInteger(evidence?.max_hp_gain)) throw new Error('Missing captured Singing Bowl resource evidence');
      return [{ kind, command: candidate.semantic_action, max_hp_gain: evidence.max_hp_gain }];
    }) : [];
    return { source, location, kind: state.context.kind, hp: state.run.current_hp, max_hp: state.run.max_hp, gold: state.run.gold,
      event_id: state.context.event,
      event_options: state.context.kind === 'event' ? frame.position.actions.filter(item => item.candidate.action.kind === 'choose_event_option').map(item => ({
        command: item.semantic_action, event_id: item.candidate.action.event, option_index: item.candidate.action.option_index,
        action: item.candidate.action.action,
        transition: item.candidate.action.public_semantics.structured_semantics?.transition,
        effects: item.candidate.action.public_semantics.structured_semantics?.effects,
      })) : [],
      reward_alternatives,
      reward_items: rewards.filter(reward => ['potion', 'relic'].includes(reward.kind)).map(reward => ({ kind: reward.kind, id: reward[reward.kind] })),
      relic_ids: state.relics.map(relic => relic.relic), deck_count: state.cards.reduce((count, card) => count + card.count, 0),
      reward_gold: rewards.filter(reward => reward.kind === 'gold').reduce((total, reward) => total + reward.amount, 0),
      has_card_reward: rewards.some(reward => reward.kind === 'card_reward'),
      reward_cards: state.context.kind === 'card_reward' ? rewards.filter(reward => reward.kind === 'card_reward').flatMap(reward => reward.cards.map(card => cardFaceView({ card_id: card.card, upgrades: card.upgrades }, definitions))) : [] };
  }
  const enemies = state.monsters.map(monster => {
  if (monster.enemy.status !== 'known') throw new Error('Unidentified public enemy');
  let intent = { kind: 'unknown' };
  if (monster.intent.intent) {
    const token = monster.intent.intent;
    const kind = typeof token === 'string' ? token : Object.keys(token)[0];
    const payload = typeof token === 'string' ? {} : token[kind];
    intent = { kind: snake(kind) };
    if (intent.kind.startsWith('attack')) Object.assign(intent, { damage: monster.intent.preview_damage_per_hit, hits: payload.hits });
    if (intent.kind === 'heal' && payload.amount !== null) intent.amount = payload.amount;
  }
  return { enemy_id: monster.enemy.enemy_id, hp: monster.hp, max_hp: monster.max_hp, block: monster.block, intent,
    powers: monster.powers.map(power => ({ power_id: power.power, amount: power.amount })) };
  });
  const piles = {};
  for (const zone of ['draw', 'discard', 'exhaust']) {
    const pile = state.cards[zone];
    if (!['public_ordered_collection', 'public_unordered_collection'].includes(pile.evidence)) throw new Error(`Unobservable pile: ${zone}`);
    const ordered = pile.evidence === 'public_ordered_collection';
    piles[zone] = { ordered, count: pile.cards.length,
      cards: ordered ? pile.cards.map(card => ({ card_id: card.card_id, upgrades: card.upgrades, count: 1 })) : cardGroups(pile.cards) };
  }
  return {
    source, location, kind: 'combat', encounter_id: context.encounter_id,
    hp: state.player.hp, max_hp: state.player.max_hp, gold: state.player.gold, block: state.player.block,
    relic_ids: state.player.relics.map(relic => relic.id),
    turn: state.turn.turn_count + 1, energy: state.turn.energy,
    hand: state.cards.hand.cards.map(card => cardView(card, definitions)), piles,
    powers: state.player.powers.map(power => ({ power_id: power.power, amount: power.amount })),
    enemies,
  };
}

export function recordedSequence(entries, definitions, id) {
  if (!entries.length) throw new Error('A sequence requires actual recorded actions');
  if (typeof entries[0].source_revision !== 'string' || !entries[0].source_revision) throw new Error('Missing producer source revision');
  const first = entries[0].before;
  const operator = entries[0].operator;
  if (!['developer', 'model'].includes(operator) || entries.some(entry => entry.operator !== operator)) throw new Error('Missing or mixed sequence operators');
  const maps = [], snapshots = [snapshot(first, definitions, maps)], actions = [];
  for (const entry of entries) {
    const previous = snapshots.at(-1).source;
    if (entry.source_revision !== entries[0].source_revision || entry.before.session_id !== first.session_id || entry.after.session_id !== first.session_id
        || entry.before.position.public_snapshot_id !== previous.public_snapshot_id
        || entry.before.head_revision !== previous.head_revision) throw new Error('Disconnected sequence boundaries');
    const action = entry.before.position.actions.find(action => action.semantic_action === entry.action);
    if (!action) throw new Error('Recorded action has no corresponding public candidate');
    const chosen = action.candidate.action, state = boundary(entry.before);
    let projected;
    if (action.candidate.domain === 'combat_atomic' && chosen.kind === 'play_card') {
      const card = state.cards.hand.cards[chosen.hand_index];
      if (!card) throw new Error('Recorded action has no corresponding hand card');
      projected = { card: cardView(card, definitions), hand_index: chosen.hand_index, target: chosen.target_monster_index,
        ...(Number.isInteger(chosen.target_monster_index) ? { target_enemy_id: state.monsters[chosen.target_monster_index].enemy.enemy_id } : {}) };
    } else if (action.candidate.domain === 'combat_atomic' && chosen.kind === 'end_turn') projected = {};
    else if (action.candidate.domain === 'strategic' && chosen.kind === 'claim_reward') projected = { reward_kind: chosen.reward.kind, amount: chosen.reward.amount, reward_id: chosen.reward[chosen.reward.kind] };
    else if (action.candidate.domain === 'strategic' && chosen.kind === 'choose_route_node') projected = { node: [chosen.x, chosen.y] };
    else if (action.candidate.domain === 'strategic' && chosen.kind === 'choose_event_option') projected = { event_id: chosen.event, option_index: chosen.option_index };
    else if (action.candidate.domain === 'strategic' && chosen.kind === 'take_card') projected = { card: cardFaceView({ card_id: chosen.card, upgrades: chosen.upgrades }, definitions) };
    else if (action.candidate.domain === 'strategic' && ['open_card_reward', 'skip_card_reward'].includes(chosen.kind)) projected = {};
    else if (action.candidate.domain === 'strategic' && chosen.kind === 'singing_bowl') projected = { max_hp_gain: boundary(entry.after).run.max_hp - state.run.max_hp };
    else if (action.candidate.domain === 'strategic' && chosen.kind === 'proceed' && chosen.site === 'reward') projected = {};
    else throw new Error('Unbound recorded action');
    actions.push({ kind: chosen.kind, ...projected, command: entry.action,
      source_snapshot_id: previous.public_snapshot_id, result_snapshot_id: entry.after.position.public_snapshot_id });
    snapshots.push(snapshot(entry.after, definitions, maps));
  }
  const view = first.position.observation;
  const context = view.domain === 'strategic' ? view.observation.run : view.run_context;
  return { id, kind: 'sequence', provenance: { operator, source_revision: entries[0].source_revision, session_id: first.session_id },
    encounter_id: context.encounter_id, turn: snapshots[0].turn, floor: context.floor,
    actions, snapshots, maps: maps.map(({ id, graph }) => ({ id, ...graph })), indices: [] };
}

export function sequenceView(scene, cursor) {
  if (!Number.isInteger(cursor) || cursor < 0 || cursor >= scene.snapshots.length) throw new Error('Invalid action cursor');
  const state = scene.snapshots[cursor], before = cursor ? scene.snapshots[cursor - 1] : null;
  const added = before ? cardGroups(state.hand ?? []).flatMap(group => {
    const previous = cardGroups(before.hand ?? []).find(card => key(card) === key(group))?.count ?? 0;
    return group.count > previous ? [{ ...group, count: group.count - previous }] : [];
  }) : [];
  return { state, action: cursor ? scene.actions[cursor - 1] : null, completed: scene.actions.slice(0, cursor), added,
    deltas: before ? { energy: Number.isFinite(state.energy) && Number.isFinite(before.energy) ? state.energy - before.energy : 0,
      block: Number.isFinite(state.block) && Number.isFinite(before.block) ? state.block - before.block : 0,
      hp: state.hp - before.hp, enemy_hp: state.enemies && before.enemies ? state.enemies.reduce((sum, enemy) => sum + enemy.hp, 0) - before.enemies.reduce((sum, enemy) => sum + enemy.hp, 0) : 0 } : {} };
}

// Slot continuity for the supported card plays: remove the recorded source
// slot, preserve the remaining public hand order, append any newly drawn cards.
// If that layout does not match, omit movement rather than invent a pairing.
export function handTransition(before, after, playedIndex) {
  if (!Number.isInteger(playedIndex) || playedIndex < 0 || playedIndex >= before.length) return null;
  const retained = before.map((card, from) => ({ card, from })).filter(item => item.from !== playedIndex);
  if (after.length < retained.length || retained.some((item, to) => key(item.card) !== key(after[to]))) return null;
  return { played: playedIndex, retained: retained.map((item, to) => ({ from: item.from, to })),
    added: after.slice(retained.length).map((_card, index) => retained.length + index) };
}
