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
  return labelRuns(label);
}

export function neowStructuredRuns(option, locale) {
  const { effects, action, transition } = option;
  const { REWARD_TEXT: text, UNIQUE_REWARDS: unique, OPTIONS: options } = original.events[resolveLocale(locale)].Neow;
  if (action === 'Leave' && transition?.kind === 'complete' && effects.length === 0) return labelRuns(options[3]);
  const shape = effects.map(effect => effect.effect).sort().join(',');
  const amount = kind => {
    const value = effects.find(effect => effect.effect === kind)?.amount;
    if (!Number.isSafeInteger(value) || value < 0) throw new Error('Missing public Neow amount');
    return String(value);
  };
  let label;
  switch (shape) {
    case 'offer_cards':
      if (effects[0].pool.kind !== 'random_class_common_or_uncommon') throw new Error('Unbound Neow card pool');
      label = text[0]; break;
    case 'gain_gold': label = text[8] + amount('gain_gold') + text[9]; break;
    case 'lose_max_hp,offer_cards':
      if (effects.find(effect => effect.effect === 'offer_cards').pool.kind !== 'random_class_rare') throw new Error('Unbound Neow rare pool');
      label = text[17] + amount('lose_max_hp') + text[18] + text[12]; break;
    case 'lose_starter_relic,obtain_relic': label = unique[0]; break;
    default: throw new Error('Unbound structured Neow offer: ' + shape);
  }
  return labelRuns(label);
}

function labelRuns(label) {
  return label.trim().split(/(\s+)/).filter(Boolean).map(token => {
    if (token.startsWith('#g')) return { text: token.slice(2), tone: 'positive' };
    if (token.startsWith('#r')) return { text: token.slice(2), tone: 'negative' };
    return { text: token, tone: 'plain' };
  });
}
