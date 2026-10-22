// Consumers of the typed decision contract:
// structured intents, adjusted numbers, potion slots, semantic
// action strings. Anything the workbench knows about the current
// decision beyond its rendered view comes from the simulator.
import { mapGraphFromObservation } from './render.mjs';
import { createPresentation } from '@sts-simulator/presentation';

// ---------------------------------------------------------------------------
// The board and potion skin consume the same typed shape as the executable's
// `show --decision-json`; neither parses the human debug rendering.

// Reuse the resident agent-shell's typed observation instead of reopening the
// Rust workspace through `show --decision-json` after each command.
export function decisionDocumentFromObservation(observation) {
  const position = observation?.position;
  if (position?.kind !== 'decision' || !position.public_snapshot_id
      || !position.observation || !Array.isArray(position.actions)) {
    throw new Error('Run-play observation has no current typed decision');
  }
  return {
    decision: {
      observation: position.observation,
      information: { snapshot_id: position.public_snapshot_id },
      candidates: position.actions.map(action => action.candidate),
    },
    semantic_actions: position.actions.map(action => action.semantic_action),
  };
}

export function decisionObservation(doc) {
  return doc?.decision?.observation?.observation ?? null;
}

export function assertCurrentDecision(reading, doc, presentation = createPresentation()) {
  // Ordinary combat readings currently have no source/choice objects. Their
  // numbers and hints come from a fresh decision-json read; Rust resolves the
  // copied command again at execution. Choice pages must carry a snapshot.
  if (!reading.raw?.reading?.source && reading.control === 'combat'
      && !reading.options?.length && !reading.selections?.length) return;
  if (!reading.raw?.reading?.source?.public_snapshot_id
      || reading.raw.reading.source.public_snapshot_id !== doc?.decision?.information?.snapshot_id) {
    throw new Error(presentation.text('workbench:command.decisionChanged'));
  }
}

export function isRewardReading(reading) {
  return (reading.raw?.reading?.options ?? []).some(option => {
    const action = option.candidate?.action;
    return action?.kind === 'claim_reward' || action?.kind === 'open_card_reward'
      || (action?.kind === 'proceed' && action.site === 'reward');
  });
}

// The displayed rows may be reordered or filtered. Their original choices,
// from this exact observation, bind them to Rust's semantic action strings.
export function optionSemantics(reading, doc, presentation = createPresentation()) {
  assertCurrentDecision(reading, doc, presentation);
  const semantics = doc.semantic_actions ?? [];
  if (semantics.length !== doc.decision?.candidates?.length) {
    throw new Error('choice contract drifted: candidate and semantic action counts differ');
  }
  return (reading.raw.reading.options ?? []).map(option => {
    const path = option.choice.ordinal_path;
    return path.length === 1 ? semantics[path[0]] ?? null : null;
  });
}

// Copyable strategic choices replace existing rows, not the compact hand.
export function conciseCardCommand(command) {
  return command?.replace(/\+0(?=[#@,]|$)/g, '') ?? null;
}

// One spelling for displayed options and consumers of the typed action list.
export function strategicOptionCommand(command, observation) {
  if (command?.startsWith('select:') && observation?.position?.observation?.domain === 'strategic') {
    return conciseCardCommand(`act ${command}`);
  }
  if (!command?.startsWith('route:')) return conciseCardCommand(command);
  const route = /^route:(-?\d+)\.(-?\d+)$/.exec(command);
  if (!route) throw new Error(`unrecognized route semantic: ${command}`);
  const coordinate = `${route[1]},${route[2]}`;
  const node = mapGraphFromObservation(observation).nodes.get(coordinate);
  if (!node) throw new Error(`route has no published node: ${coordinate}`);
  return `go ${coordinate}`;
}

const cardCommandPattern = /^(?<verb>take|choose|smith|select|remove|toke|buy|play):(?<name>[^+#@,]+)(?<upgrade>\+(?:\d+)?)?(?<occurrence>#\d+)?(?<target>@\d+)?$/;

function cardCommandParts(command) {
  const parts = cardCommandPattern.exec(command)?.groups;
  if (!parts) return null;
  return { ...parts, level: parts.upgrade === undefined ? null : parts.upgrade === '+' ? 1 : Number(parts.upgrade.slice(1)) };
}

export function readingOptionCommands(reading, doc, presentation = createPresentation()) {
  const commands = optionSemantics(reading, doc, presentation);
  return commands.map(command => strategicOptionCommand(command,
    { position: { kind: 'decision', observation: doc.decision.observation } }));
}

export function currentCommandHint(reading, doc, { preview = false, presentation = createPresentation() } = {}) {
  const commands = preview
    ? (reading.options?.length ? optionSemantics(reading, doc, presentation) : expandedCombatCommands(reading, doc, presentation))
      .filter(Boolean).map(action => `preview ${conciseCardCommand(action)}`)
    : reading.options?.length ? readingOptionCommands(reading, doc, presentation)
      : expandedCombatCommands(reading, doc, presentation).map(command => `act ${command}`);
  return commands.length
    ? presentation.text('workbench:command.completeCommand', { commands: commands.map(command => `  ${command}`).join('\n') })
    : presentation.text('workbench:command.inspectOptions');
}

export function expandedCombatCommands(reading, doc, presentation = createPresentation()) {
  if (reading.control !== 'combat' || reading.options?.length || reading.selections?.length) return [];
  assertCurrentDecision(reading, doc, presentation);
  const commands = doc.semantic_actions ?? [];
  if (commands.length !== doc.decision?.candidates?.length) {
    throw new Error('choice contract drifted: candidate and semantic action counts differ');
  }
  return commands.filter(command => typeof command === 'string' && command.length > 0)
    .map(conciseCardCommand);
}

// Show one executable command per equivalent card/target and every usable bottle.
// Repeated copies share a representative; options still exposes every occurrence
// and auxiliary actions such as discarding a potion.
// Bind each to the engine candidate: #occurrence differs from hand/potion slots.
export function distinctCombatCommands(reading, doc, presentation = createPresentation()) {
  if (reading.control !== 'combat' || reading.options?.length || reading.selections?.length) return [];
  expandedCombatCommands(reading, doc, presentation); // Validate snapshot and parallel arrays.
  const cards = decisionObservation(doc)?.cards?.hand?.cards ?? [];
  const seen = new Set();
  return doc.decision.candidates.flatMap((candidate, index) => {
    const semantic = doc.semantic_actions[index];
    if (semantic === 'end') return ['end'];
    if (candidate.action?.kind === 'use_potion') {
      return semantic
        ? [`${semantic}  |  P${candidate.action.potion_index}`] : [];
    }
    if (candidate.action?.kind !== 'play_card') return [];
    const card = cards[candidate.action.hand_index];
    if (!card || !semantic) return [];
    const key = JSON.stringify([card, candidate.action.target_monster_index ?? null]);
    if (seen.has(key)) return [];
    seen.add(key);
    return [`${conciseCardCommand(semantic)}  |  ${card.effective_cost}c`];
  });
}

const activeMonster = monster => monster.alive !== false && !monster.dying
  && !monster.escaped && !monster.half_dead;

// Damage is supplied by Rust, per hit and before block. The vector uses monster
// order; displayed M labels use slot. Candidate targets use vector indices too.
export function cardDamageLabel(card, monsters, { targetIndex = null, suffix = '', separator = ' ' } = {}) {
  const damage = card.damage_by_monster_order ?? [];
  if (damage.length === 0) return (card.current_damage ?? 0) > 0 ? `${card.current_damage}${suffix}` : '';
  if (damage.length !== monsters.length) throw new Error('damage contract drifted: monster and damage counts differ');
  const targets = monsters.flatMap((monster, index) => {
    if (!activeMonster(monster) || (targetIndex !== null && index !== targetIndex)) return [];
    if (!Number.isInteger(monster.slot) || !Number.isFinite(damage[index])) {
      throw new Error('damage contract drifted: missing monster slot or damage');
    }
    return [{ slot: monster.slot, damage: damage[index] }];
  });
  if (targets.length === 0) return '';
  if (targets.every(target => target.damage === targets[0].damage)) return `${targets[0].damage}${suffix}`;
  return targets.map(target => `M${target.slot}:${target.damage}${suffix}`).join(separator);
}

export function resolveOptionName(wanted, reading, doc, presentation = createPresentation()) {
  const aligned = optionSemantics(reading, doc, presentation);
  if (wanted.includes('\uFF0B')) {
    throw new Error(presentation.text('workbench:command.fullwidthPlus', { wanted }));
  }
  // Explicit selectors must never fall back to a substring: a missing #0
  // must not select #01, another object, or an unrelated label.
  if (/^[a-z][a-z-]*:/.test(wanted) || /#\d+/.test(wanted)) {
    const hits = aligned.flatMap((semantic, index) => semantic === wanted ? [index] : []);
    if (hits.length === 1) return hits[0];
    const requested = cardCommandParts(wanted);
    if (requested) {
      // Honor an explicit upgrade or prefer base/sole level before applying
      // an optional #occurrence, matching Rust's unique selector resolution.
      const family = aligned.flatMap((semantic, index) => {
        const card = cardCommandParts(semantic);
        return card && card.verb === requested.verb && card.name === requested.name
          && card.target === requested.target && card.level !== null ? [{ index, card }] : [];
      });
      const levels = new Set(family.map(({ card }) => card.level));
      const preferred = requested.level ?? (levels.has(0) ? 0 : levels.size === 1 ? [...levels][0] : null);
      if (preferred !== null) {
        const matches = family.filter(({ card }) => card.level === preferred
          && (requested.occurrence === undefined || card.occurrence === requested.occurrence));
        if (matches.length === 1) return matches[0].index;
      }
    }
    throw new Error(presentation.text('workbench:command.noExactOption', { wanted, hint: currentCommandHint(reading, doc, { presentation }) }));
  }
  const norm = value => (value ?? '').toLowerCase().replaceAll(/[^\p{L}\p{N}]/gu, '');
  const key = norm(wanted);
  const options = reading.options ?? [];
  const entries = aligned.map((semantic, index) => ({ index, keys: semantic === null
    ? [norm(options[index].label)]
    : [semantic, semantic.replace(/^[a-z-]+:/, ''),
      semantic.replace(/^[a-z-]+:/, '').replace(/[+#@]\S*$/, ''), options[index].label].map(norm),
  }));
  const exact = key ? entries.filter(entry => entry.keys.includes(key)) : [];
  if (exact.length === 1) return exact[0].index;
  const partial = entries.filter(entry => entry.keys.some(value => key.length > 0 && value.includes(key)));
  if (exact.length === 0 && partial.length === 1) return partial[0].index;
  throw new Error(presentation.text('workbench:command.noUniqueOption', { wanted, hint: currentCommandHint(reading, doc, { presentation }) }));
}

export function parseDecisionPotions(doc) {
  const slots = [];
  const strategic = doc?.decision?.observation?.domain === 'strategic';
  (decisionObservation(doc)?.potions ?? []).forEach((potion, index) => {
    if (potion === null || potion === undefined) return;
    if (strategic && potion.potion === null) return;
    const stableId = strategic ? potion.potion?.potion
      : typeof potion === 'string' ? potion : potion.potion_id;
    const slot = strategic ? potion.slot : index;
    if (typeof stableId !== 'string' || !stableId || !Number.isSafeInteger(slot) || slot < 0) {
      throw new Error('potion contract drifted: missing stable identity or slot');
    }
    slots.push({ index: slot, stableId, key: stableId.replaceAll(/[^A-Za-z]/g, '').toLowerCase() });
  });
  slots.sort((left, right) => left.index - right.index);
  // The executable's use-potion semantic strings carry #occurrence per
  // identical potion; map each form back to its slot so NAME resolves to
  // the ready-made string with no id guessing.
  const useForms = new Map();
  (doc.semantic_actions ?? []).forEach(semantic => {
    // Targeted forms carry an @slot tail (use-potion:X#0@0); it stays part
    // of the executable string, so capture occurrence and optional target
    // separately when mapping the form back to its potion slot.
    const match = /^use-potion:(.+?)#(\d+)(?:@\d+)?$/.exec(semantic ?? '');
    if (!match) return;
    const matching = slots.filter(slot => slot.stableId === match[1]);
    const slot = matching[Number(match[2])];
    if (slot && !useForms.has(slot.index)) useForms.set(slot.index, semantic);
  });
  return { slots, useForms };
}

export function summarizeDecisionJson(doc, presentation = createPresentation()) {
  const inner = decisionObservation(doc);
  if (inner === null) {
    throw new Error('decision JSON is missing its combat observation — contract drifted');
  }
  const candidates = doc.decision?.candidates ?? [];
  const semanticActions = doc.semantic_actions ?? null;
  if (semanticActions !== null && semanticActions.length !== candidates.length) {
    throw new Error(`decision JSON contract drifted: ${candidates.length} candidates but ${semanticActions.length} semantic actions`);
  }
  let incoming = 0;
  for (const monster of inner.monsters ?? []) {
    if (!activeMonster(monster)) continue;
    const perHit = monster?.intent?.preview_damage_per_hit;
    if (typeof perHit !== 'number' || perHit <= 0) continue;
    let hits = 1;
    for (const variant of Object.values(monster.intent?.intent ?? {})) {
      if (variant && typeof variant.hits === 'number') hits = variant.hits;
    }
    incoming += perHit * hits;
  }
  const handCards = inner.cards?.hand?.cards ?? [];
  const handBlock = handCards.reduce((sum, card) => sum + (card.current_block ?? 0), 0);
  const playableLines = [];
  candidates.forEach((candidate, index) => {
    const semantic = semanticActions ? semanticActions[index] : null;
    if (!semantic) return;
    if (candidate?.action?.kind === 'play_card') {
      const card = handCards[candidate.action.hand_index] ?? {};
      const bits = [presentation.text('workbench:board.cost', { cost: card.effective_cost ?? '?' })];
      const damage = cardDamageLabel(card, inner.monsters ?? [], {
        targetIndex: candidate.action.target_monster_index ?? null, separator: ',',
      });
      if (damage) bits.push(presentation.text('workbench:board.damage', { damage }));
      if ((card.current_block ?? 0) > 0) bits.push(presentation.text('workbench:board.block', { block: card.current_block }));
      playableLines.push(`${semantic} ${bits.join(' ')}`);
    } else if (/^(?:end$|use-potion:)/.test(semantic)) {
      playableLines.push(semantic);
    }
  });
  return { incoming, currentBlock: inner.player?.block ?? null, handBlock, playableLines };
}

// Skin-layer spelling for potions: `potion:Essence` resolves against the
// decision JSON contract — stable ids in the observation's potion slots
// and the ready-made use-potion semantic strings. The name ladder matches
// pick's: exact, then de-spaced contains, then a unique substring; real
// ambiguity is refused with the candidate list. Identical potions in
// several slots are not ambiguous (same effect).
export function resolvePotionToken(token, doc, presentation = createPresentation()) {
  const match = /^potion:(.+)$/.exec(token);
  if (!match) return null;
  const [, wantedRaw] = match;
  const wanted = wantedRaw.replaceAll(/\s+/g, '').toLowerCase();
  const { slots, useForms } = parseDecisionPotions(doc);
  if (slots.length === 0) {
    throw new Error(presentation.text('workbench:command.noPotion'));
  }
  const byExact = slots.filter(s => s.key === wanted);
  const hits = byExact.length > 0 ? byExact : slots.filter(s => s.key.includes(wanted));
  if (hits.length === 0) {
    throw new Error(presentation.text('workbench:command.noMatchingPotion', { wanted: wantedRaw, available: slots.map(s => s.stableId).join(', ') }));
  }
  if (new Set(hits.map(s => s.key)).size > 1) {
    throw new Error(presentation.text('workbench:command.ambiguousPotion', { wanted: wantedRaw, matches: hits.map(s => s.stableId).join(', ') }));
  }
  const chosen = hits[0];
  const action = useForms.get(chosen.index);
  if (!action) {
    throw new Error(presentation.text('workbench:command.potionUnavailable', { potion: chosen.stableId }));
  }
  return { action, name: chosen.stableId };
}

// Resolve names only. The engine evaluates draws, hand limits and the other
// effects after all preceding actions in the line have resolved.
export function applyPotionSkin(tokens, doc, presentation = createPresentation()) {
  return tokens.map(token => resolvePotionToken(token, doc, presentation)?.action ?? token);
}
