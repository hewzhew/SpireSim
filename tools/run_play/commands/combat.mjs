// Act publishes a validated line; preview runs it on a private engine branch.
// Board presents the current combat without executing a line.
import { printReading, writeText, previewInventoryChanges, presentationFor } from '../render.mjs';
import { summarizeDecisionJson, applyPotionSkin, assertCurrentDecision, decisionDocumentFromObservation } from '../contract.mjs';
import { expandRepeatSugar, normalizeActionTokens, actTrimmingEndedCombat } from '../tokens.mjs';

export async function actOrPreview(ctx, command, parameters) {
  const { game, usage, printCurrentReading, decisionDocument } = ctx;
  const write = ctx.write ?? writeText;
  if (parameters.length < 1) await usage();
  const presentation = await presentationFor(ctx);
  const text = (key, params = {}) => presentation.text(`workbench:command.${key}`, params);
  let tokens = normalizeActionTokens(expandRepeatSugar(parameters, presentation), presentation);
  const before = await game.decision();
  if (command === 'preview' && typeof before.summary !== 'string') {
    throw new Error(text('compatibilitySummaryRequired'));
  }
  if (tokens.some(token => /^\d+$/.test(token) || token.startsWith('ordinal:'))) {
    throw new Error(text('semanticActions',{command,hint:await ctx.commandHint(before,{preview:command==='preview'})}));
  }
  const previewSource = command === 'preview' && before.control === 'strategic'
    ? await decisionDocument(before) : null;
  if (previewSource) assertCurrentDecision(before, previewSource, presentation);
  // The engine resolves the whole line, including draws and energy changes.
  // Potion skin tokens resolve against the typed decision contract
  // (one extra round trip, only when asked).
  if (tokens.some(t => /^potion:/.test(t))) {
    tokens = applyPotionSkin(tokens, await decisionDocument(before), presentation);
  }
  tokens = tokens.map(t => (t === 'end!' ? 'end' : t));
  const executor = command === 'preview' ? { act: line => game.preview(line) } : game;
  let acceptedTokens = tokens;
  let reading;
  try {
    reading = await actTrimmingEndedCombat(executor, tokens,
      { write, presentation, onAccepted: line => { acceptedTokens = line; } });
  } catch (error) {
    if (/^exact run-play (?:prepared line|probe) could not resolve step \d+:/.test(error.message)) {
      error.message += command === 'act'
        ? text('actPreparationFailed')
        : text('previewPreparationFailed');
    }
    throw error;
  }
  if (command === 'preview') {
    const presentation = await presentationFor(ctx);
    write(`${presentation.text('workbench:preview.heading')}\n\n`);
    write(`${presentation.text('workbench:preview.actual', { summary: before.summary.trimEnd() })}\n\n`);
    if (reading.stop?.kind === 'unresolved') {
      write(`${presentation.text('workbench:preview.unresolved')}\n`);
    } else if (reading.stop?.kind === 'terminal') {
      write(`${presentation.text('workbench:preview.terminal')}\n`);
    } else {
      write(`${presentation.text('workbench:preview.result')}\n`);
    }
    await printReading(reading, { write, presentation });
    write(await previewInventoryChanges(previewSource?.decision.observation, reading.endpoint, { presentation }));
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
  await printCurrentReading(reading, { document: doc });
  // The combat summary only exists mid-fight; outside combat the plain
  // reading already says everything.
  if (reading.control === 'combat') {
    const presentation = await presentationFor(ctx);
    assertCurrentDecision(reading, doc, presentation);
    const { incoming, currentBlock, handBlock, playableLines } = summarizeDecisionJson(doc, presentation);
    if (incoming > 0) {
      write(`${presentation.text('workbench:board.incoming', { incoming, block: currentBlock ?? '?' })}\n`);
      write(`${presentation.text('workbench:board.handBlock', { block: handBlock })}\n`);
    }
    if (playableLines.length > 0) {
      write(`${presentation.text('workbench:board.playable')}\n`);
      for (const line of playableLines) write(`  ${line}\n`);
    }
  }
}

// The ordinary game exposes pile contents, without revealing hidden draw order.
// Use only the typed public observation; never call the exact-pile endpoint.
export async function publicPiles(ctx) {
  const presentation = await presentationFor(ctx);
  const state = decisionDocumentFromObservation(await ctx.game.observe()).decision.observation;
  if (state.domain !== 'combat') throw new Error(presentation.text('workbench:command.pilesCombat'));
  const lines = [presentation.text('workbench:piles.heading')];
  for (const key of ['draw', 'discard', 'exhaust']) {
    const pile = state.observation.cards[key];
    if (!['public_unordered_collection', 'public_ordered_collection'].includes(pile?.evidence)
        || !Array.isArray(pile.cards)) throw new Error(presentation.text('workbench:command.pileUnavailable', { pile: presentation.text(`workbench:piles.${key}`) }));
    const counts = new Map();
    const nameIds = new Map();
    for (const card of pile.cards) {
      const displayName = presentation.entity('card', card.card_id);
      const ids = nameIds.get(displayName) ?? new Set();
      ids.add(card.card_id);
      nameIds.set(displayName, ids);
      const identity = `${card.card_id}+${card.upgrades}`;
      const group = counts.get(identity) ?? { id: card.card_id, upgrades: card.upgrades, count: 0 };
      group.count++;
      counts.set(identity, group);
    }
    const cards = [...counts].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
      .map(([, { id, upgrades, count }]) => {
        const disambiguation = nameIds.get(presentation.entity('card', id)).size > 1 ? ` [${id}]` : '';
        const name = presentation.cardName(id, upgrades) + disambiguation;
        return count > 1 ? `${count}x ${name}` : name;
      });
    lines.push(presentation.text('workbench:piles.row', {
      label: presentation.text(`workbench:piles.${key}`), count: pile.cards.length,
      cards: cards.join(', ') || presentation.text('workbench:piles.empty'),
    }));
  }
  ctx.write(lines.join('\n') + '\n');
}
