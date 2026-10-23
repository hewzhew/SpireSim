import original from '../original-event-text.json' with { type: 'json' };
import { resolveLocale } from '../catalogs.mjs';

// This replay has four curated offer shapes. Numbers come from its captured
// public effects; wording and color markers come from the original resources.
// Keep this display binding separate from game choices and execution.
export function neowOptionRuns(effects, locale) {
  const { REWARD_TEXT: text, UNIQUE_REWARDS: unique } = original.events[resolveLocale(locale)].Neow;
  const shape = effects.map(effect => effect.kind).sort().join(',');
  const amount = kind => {
    const value = effects.find(effect => effect.kind === kind)?.amount;
    if (!Number.isSafeInteger(value) || value < 0) throw new Error(`Missing recorded Neow amount: ${kind}`);
    return String(value);
  };
  let label;
  switch (shape) {
    case 'remove_card': label = text[2]; break;
    case 'max_hp': label = text[7] + amount('max_hp') + ' ]'; break;
    case 'lose_hp,max_hp':
      label = text[21] + amount('lose_hp') + text[29] + ' ' + text[16] + amount('max_hp') + ' ]';
      break;
    case 'boss_relic,lose_relic': label = unique[0]; break;
    default: throw new Error(`Unbound recorded Neow offer: ${shape}`);
  }
  return label.trim().split(/(\s+)/).filter(Boolean).map(token => {
    if (token.startsWith('#g')) return { text: token.slice(2), tone: 'positive' };
    if (token.startsWith('#r')) return { text: token.slice(2), tone: 'negative' };
    return { text: token, tone: 'plain' };
  });
}
