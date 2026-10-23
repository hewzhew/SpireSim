// Project recorded public observations. No game execution or private state.
const supportedCards = new Set(['Strike', 'Defend', 'Bash', 'ShrugItOff', 'AscendersBane']);
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

function combat(frame) {
  const view = frame.position?.observation;
  if (typeof frame.session_id !== 'string' || !frame.session_id || !Number.isSafeInteger(frame.head_revision)
      || typeof frame.position?.public_snapshot_id !== 'string' || !frame.position.public_snapshot_id) throw new Error('Missing recorded boundary identity');
  if (frame.position?.kind !== 'decision' || view?.domain !== 'combat') throw new Error('A sequence boundary must be a recorded public combat decision');
  if (view.observation.information_access !== 'public') throw new Error('A sequence requires public information');
  return view.observation;
}

function cardView(card, definitions) {
  if (!supportedCards.has(card.card_id)) throw new Error(`Unbound sequence card: ${card.card_id}`);
  const definition = definitions.find(fact => fact.kind === 'card' && fact.definition.current.card === card.card_id)?.definition;
  const facts = card.upgrades === 0 ? definition?.current : card.upgrades === 1 ? definition?.upgraded : null;
  if (!facts) throw new Error(`Missing captured card definition: ${card.card_id}`);
  const effects = [];
  if (card.effective_cost === -2) effects.push({ kind: 'unplayable' });
  if (facts.ethereal) effects.push({ kind: 'ethereal' });
  if (facts.damage?.hit_count > 0) effects.push({ kind: 'damage', damage: card.damage_by_monster_order[0] ?? '?' });
  if (facts.block > 0) effects.push({ kind: 'block', block: card.current_block });
  if (facts.vulnerable > 0) effects.push({ kind: 'apply', power_id: 'Vulnerable', amount: facts.vulnerable });
  if (facts.draw_cards > 0) effects.push({ kind: 'draw', count: facts.draw_cards });
  return { card_id: card.card_id, upgrades: card.upgrades, type: facts.card_type.toLowerCase(),
    cost: card.effective_cost < 0 ? null : card.effective_cost, effects };
}

function snapshot(frame, definitions) {
  const state = combat(frame);
  if (state.monsters.length !== 1 || state.monsters[0].enemy.status !== 'known') throw new Error('This sequence view requires one publicly identified enemy');
  const monster = state.monsters[0];
  let intent = { kind: 'unknown' };
  if (monster.intent.intent) {
    const token = monster.intent.intent;
    const kind = typeof token === 'string' ? token : Object.keys(token)[0];
    const payload = typeof token === 'string' ? {} : token[kind];
    intent = { kind: snake(kind) };
    if (intent.kind.startsWith('attack')) Object.assign(intent, { damage: monster.intent.preview_damage_per_hit, hits: payload.hits });
    if (intent.kind === 'heal' && payload.amount !== null) intent.amount = payload.amount;
  }
  const piles = {};
  for (const zone of ['draw', 'discard', 'exhaust']) {
    const pile = state.cards[zone];
    if (!['public_ordered_collection', 'public_unordered_collection'].includes(pile.evidence)) throw new Error(`Unobservable pile: ${zone}`);
    const ordered = pile.evidence === 'public_ordered_collection';
    piles[zone] = { ordered, count: pile.cards.length,
      cards: ordered ? pile.cards.map(card => ({ card_id: card.card_id, upgrades: card.upgrades, count: 1 })) : cardGroups(pile.cards) };
  }
  return {
    source: { session_id: frame.session_id, head_revision: frame.head_revision, public_snapshot_id: frame.position.public_snapshot_id },
    hp: state.player.hp, max_hp: state.player.max_hp, gold: state.player.gold, block: state.player.block,
    relic_ids: state.player.relics.map(relic => relic.id),
    turn: state.turn.turn_count + 1, energy: state.turn.energy,
    hand: state.cards.hand.cards.map(card => cardView(card, definitions)), piles,
    powers: state.player.powers.map(power => ({ power_id: power.power, amount: power.amount })),
    enemy: { enemy_id: monster.enemy.enemy_id, hp: monster.hp, max_hp: monster.max_hp, block: monster.block, intent,
      powers: monster.powers.map(power => ({ power_id: power.power, amount: power.amount })) },
  };
}

export function recordedSequence(entries, definitions, id) {
  if (!entries.length) throw new Error('A sequence requires actual recorded actions');
  if (typeof entries[0].source_revision !== 'string' || !entries[0].source_revision) throw new Error('Missing producer source revision');
  const first = entries[0].before;
  const snapshots = [snapshot(first, definitions)], actions = [];
  for (const entry of entries) {
    const previous = snapshots.at(-1).source;
    if (entry.source_revision !== entries[0].source_revision || entry.before.session_id !== first.session_id || entry.after.session_id !== first.session_id
        || entry.before.position.public_snapshot_id !== previous.public_snapshot_id
        || entry.before.head_revision !== previous.head_revision) throw new Error('Disconnected sequence boundaries');
    const action = entry.before.position.actions.find(action => action.semantic_action === entry.action);
    if (action?.candidate.domain !== 'combat_atomic' || action.candidate.action.kind !== 'play_card') throw new Error('This sequence requires recorded card-play choices');
    const state = combat(entry.before);
    const card = state.cards.hand.cards[action.candidate.action.hand_index];
    if (!card) throw new Error('Recorded action has no corresponding hand card');
    actions.push({ card: cardView(card, definitions), command: entry.action, hand_index: action.candidate.action.hand_index, target: action.candidate.action.target_monster_index,
      source_snapshot_id: previous.public_snapshot_id, result_snapshot_id: entry.after.position.public_snapshot_id });
    snapshots.push(snapshot(entry.after, definitions));
  }
  const context = first.position.observation.run_context;
  return { id, kind: 'sequence', provenance: { operator: 'developer', source_revision: entries[0].source_revision, session_id: first.session_id },
    encounter_id: context.encounter_id, turn: snapshots[0].turn, floor: context.floor,
    actions, snapshots, indices: [] };
}

export function sequenceView(scene, cursor) {
  if (!Number.isInteger(cursor) || cursor < 0 || cursor >= scene.snapshots.length) throw new Error('Invalid action cursor');
  const state = scene.snapshots[cursor], before = cursor ? scene.snapshots[cursor - 1] : null;
  const added = before ? cardGroups(state.hand).flatMap(group => {
    const previous = cardGroups(before.hand).find(card => key(card) === key(group))?.count ?? 0;
    return group.count > previous ? [{ ...group, count: group.count - previous }] : [];
  }) : [];
  return { state, action: cursor ? scene.actions[cursor - 1] : null, completed: scene.actions.slice(0, cursor), added,
    deltas: before ? { energy: state.energy - before.energy, block: state.block - before.block,
      hp: state.hp - before.hp, enemy_hp: state.enemy.hp - before.enemy.hp } : {} };
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
