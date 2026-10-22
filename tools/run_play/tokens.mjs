// The act-line token language: repeat sugar and combat-end trimming.
// Rust resolves names, copies, targets and legality; errors do not authorize
// the client to guess a different occurrence, upgrade or target.
import { writeText } from './render.mjs';
import { createPresentation } from '@sts-simulator/presentation';

// `play:Strike+0@1*3` becomes three equivalent transactional Strike plays;
// the server resolves each copy against its own successor.
export function normalizeActionTokens(tokens, presentation = createPresentation()) {
  if (tokens[0] === 'select') {
    const cards = tokens.slice(1);
    if (cards.length === 0 || cards.some(card => !card || /[:,]/.test(card)
      || ['end', 'proceed', 'cancel', 'leave'].includes(card))) {
      throw new Error(presentation.text('workbench:command.selectSyntax'));
    }
    return [`select:${cards.join(',')}`];
  }
  const actions = [];
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i] !== 'go') { actions.push(tokens[i]); continue; }
    const coordinates = /^(\d+),(\d+)$/.exec(tokens[++i] ?? '');
    if (!coordinates) throw new Error(presentation.text('workbench:command.lineCoordinates'));
    actions.push(`route:${coordinates[1]}.${coordinates[2]}`);
  }
  return actions;
}

export function expandRepeatSugar(tokens, presentation = createPresentation()) {
  const expanded = [];
  for (const token of tokens) {
    const match = /^(.*[^*])\*(\d+)$/.exec(token);
    if (!match) { expanded.push(token); continue; }
    const [, base, countText] = match;
    const count = Number.parseInt(countText, 10);
    if (count < 1 || count > 12) {
      throw new Error(presentation.text('workbench:command.repeatRange', { token }));
    }
    if (!/^play:/.test(base)) {
      throw new Error(presentation.text('workbench:command.repeatPlayOnly', { token }));
    }
    expanded.push(...Array.from({ length: count }, () => base));
  }
  return expanded;
}

// A lethal play line legally ends the combat before its trailing actions
// resolve, and the transactional server then rejects the whole line. Retry
// once without the tail when the failure says the combat already ended and
// the remaining steps are only plays or `end`; anything else stays an error.
export async function actTrimmingEndedCombat(game, tokens, { onAccepted, write = writeText, presentation = createPresentation() } = {}) {
  try {
    const reading = await game.act(tokens);
    onAccepted?.(tokens);
    return reading;
  } catch (error) {
    const message = String(error.message);
    const match = /step (\d+):/.exec(message);
    const failing = match ? Number.parseInt(match[1], 10) - 1 : -1;
    const combatEnded = /requires a combat decision|no legal match at (Reward|Map|Campfire|Card reward|Shop|Event|Neow|Boss relic|Treasure)/.test(message);
    const tail = tokens.slice(failing);
    if (failing >= 1 && failing < tokens.length && combatEnded
      && tail.every(token => token === 'end' || /^play:/.test(token))) {
      write(`${presentation.text('workbench:command.combatEndedTail', { step: failing, tail: tail.join(' ') })}\n`);
      const prefix = tokens.slice(0, failing);
      const reading = await game.act(prefix);
      onAccepted?.(prefix);
      return reading;
    }
    throw error;
  }
}

// A row index selects one option from the current reading.
export function parseChoiceToken(value, presentation = createPresentation()) {
  const match = /^(\d+)$/.exec(value);
  if (!match) {
    throw new Error(presentation.text('workbench:command.choiceToken', { value }));
  }
  return { index: Number.parseInt(match[1], 10) };
}
