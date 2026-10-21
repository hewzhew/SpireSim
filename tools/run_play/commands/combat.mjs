// Act publishes a validated line; preview runs it on a private engine branch.
// Board presents the current combat without executing a line.
import { printReading, writeText, previewInventoryChanges } from '../render.mjs';
import { summarizeDecisionJson, applyPotionSkin, assertCurrentDecision, decisionDocumentFromObservation } from '../contract.mjs';
import { expandRepeatSugar, normalizeActionTokens, actTrimmingEndedCombat } from '../tokens.mjs';

export async function actOrPreview(ctx, command, parameters) {
  const { game, usage, printCurrentReading, decisionDocument } = ctx;
  const write = ctx.write ?? writeText;
  if (parameters.length < 1) usage();
  let tokens = normalizeActionTokens(expandRepeatSugar(parameters));
  const before = await game.decision();
  if (tokens.some(token => /^\d+$/.test(token) || token.startsWith('ordinal:'))) {
    throw new Error(`${command} requires semantic actions, not row numbers or ordinal addressing.\n${await ctx.commandHint(before, { preview: command === 'preview' })}`);
  }
  const previewSource = command === 'preview' && before.control === 'strategic'
    ? await decisionDocument(before) : null;
  if (previewSource) assertCurrentDecision(before, previewSource);
  // The engine resolves the whole line, including draws and energy changes.
  // Potion skin tokens resolve against the typed decision contract
  // (one extra round trip, only when asked).
  if (tokens.some(t => /^potion:/.test(t))) {
    tokens = applyPotionSkin(tokens, await decisionDocument(before));
  }
  tokens = tokens.map(t => (t === 'end!' ? 'end' : t));
  const executor = command === 'preview' ? { act: line => game.preview(line) } : game;
  let acceptedTokens = tokens;
  let reading;
  try {
    reading = await actTrimmingEndedCombat(executor, tokens,
      { write, onAccepted: line => { acceptedTokens = line; } });
  } catch (error) {
    if (/^exact run-play (?:prepared line|probe) could not resolve step \d+:/.test(error.message)) {
      error.message += command === 'act'
        ? '\nPreparation failed; this act line was not applied.'
        : '\nPreview preparation failed; actual position unchanged.';
    }
    throw error;
  }
  if (command === 'preview') {
    write('Preview (current position unchanged)\n\n');
    const actual = before.view.split('\n').filter(line => /^(Act \d|Turn \d|Hand:)/.test(line));
    if (actual.length) write(`Actual position:\n${actual.join('\n')}\n\n`);
    if (reading.stop?.kind === 'unresolved') {
      write('Private boundary reached by preview; repair the failed action and suffix before act:\n');
    } else if (reading.stop?.kind === 'terminal') {
      write('Private terminal reached by preview:\n');
    } else {
      write('Private result of preview:\n');
    }
    printReading(reading, { write });
    write(previewInventoryChanges(previewSource?.decision.observation, reading.endpoint));
    return;
  }
  ctx.recordAction?.(acceptedTokens.join(' '));
  await (ctx.showSuccessor ?? printCurrentReading)(reading);
}

export async function board(ctx) {
  const { game, printCurrentReading, decisionDocument } = ctx;
  const write = ctx.write ?? writeText;
  // A static thinking surface between reading and acting: the compact
  // reading plus each playable card's ADJUSTED numbers (strength, frail,
  // weak already applied — straight from the typed decision contract)
  // and the total incoming damage next enemy turn. No mutation, no
  // pressure: look until the plan is worth committing.
  const reading = await game.decision({ details: ['brief'] });
  const doc = reading.control === 'combat' ? await decisionDocument(reading) : null;
  await printCurrentReading(reading);
  // The combat summary only exists mid-fight; outside combat the plain
  // reading already says everything.
  if (reading.control === 'combat') {
    assertCurrentDecision(reading, doc);
    const { incoming, handBlock, playableLines } = summarizeDecisionJson(doc);
    const blockNow = /Block (\d+)/.exec(reading.view)?.[1] ?? '?';
    if (incoming > 0) {
      write(`incoming next enemy turn: ${incoming} | current block ${blockNow}\n`);
      write(`hand block sum: ${handBlock} (individual projections; ignores cost, order and play limits)\n`);
    }
    if (playableLines.length > 0) {
      write(`playable (damage per hit before block; adjusted for the listed target):\n`);
      for (const line of playableLines) write(`  ${line}\n`);
    }
  }
}

// The ordinary game exposes pile contents, without revealing hidden draw order.
// Use only the typed public observation; never call the exact-pile endpoint.
export async function publicPiles(ctx) {
  const state = decisionDocumentFromObservation(await ctx.game.observe()).decision.observation;
  if (state.domain !== 'combat') throw new Error('piles requires an active combat');
  const lines = ['Combat piles (unordered; current combat, not the permanent deck)'];
  for (const [key, label] of [['draw', 'Draw'], ['discard', 'Discard'], ['exhaust', 'Exhaust']]) {
    const pile = state.observation.cards[key];
    if (!['public_unordered_collection', 'public_ordered_collection'].includes(pile?.evidence)
        || !Array.isArray(pile.cards)) throw new Error(`Public ${key} pile contents are unavailable`);
    const counts = new Map();
    for (const card of pile.cards) {
      const name = card.card_id + (card.upgrades === 1 ? '+' : card.upgrades > 1 ? `+${card.upgrades}` : '');
      counts.set(name, (counts.get(name) ?? 0) + 1);
    }
    const cards = [...counts].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
      .map(([name, count]) => count > 1 ? `${count}x ${name}` : name);
    lines.push(`${label} [${pile.cards.length}]: ${cards.join(', ') || 'empty'}`);
  }
  ctx.write(lines.join('\n') + '\n');
}
