import enGame from '@sts-simulator/presentation/locales/en.json' with { type: 'json' };
import zhGame from '@sts-simulator/presentation/locales/zh-CN.json' with { type: 'json' };
import en from '../locale/en.json' with { type: 'json' };
import zh from '../locale/zh.json' with { type: 'json' };

export const mapNamespace = 'sts.map';
export const gameNamespace = 'sts.game';
export const workbenchNamespace = 'sts.workbench';
export const mapDictionaries = { en: en.map, zh: zh.map };
export const workbenchDictionaries = { en: en.workbench, zh: zh.workbench };
// Names are plain text. Effect templates stay with their own formatter;
// dsh UI messages use the host's native {name} syntax.
export const gameDictionaries = Object.fromEntries(Object.entries({ en: enGame, zh: zhGame }).map(([locale, catalog]) => [locale,
  Object.fromEntries(['encounter', 'room', 'potion'].flatMap(kind => Object.entries(catalog.game[kind]).map(([id, name]) => [`${kind}.${id}`, name]))),
]));

export function validateHostDictionaries(resources, namespace) {
  const keys = Object.keys(resources.en).sort();
  const params = text => [...new Set([...text.matchAll(/\{(\w+)\}/g)].map(match => match[1]))].sort();
  for (const locale of ['en', 'zh']) {
    const dictionary = resources[locale];
    if (JSON.stringify(Object.keys(dictionary).sort()) !== JSON.stringify(keys)) throw new Error(`${namespace} translation keys differ: ${locale}`);
    for (const key of keys) {
      const value = dictionary[key];
      if (typeof value !== 'string' || !value.trim() || /[{}]/.test(value.replace(/\{\w+\}/g, ''))) throw new Error(`Invalid host translation: ${locale}:${key}`);
      if (JSON.stringify(params(value)) !== JSON.stringify(params(resources.en[key]))) throw new Error(`${namespace} translation parameters differ: ${locale}:${key}`);
    }
  }
  for (const [locale, names] of Object.entries(gameDictionaries)) for (const [key, value] of Object.entries(names)) {
    if (/[{}]/.test(value)) throw new Error(`Game name must be plain text: ${locale}:${key}`);
  }
}

export function codeText(t, group, code) {
  const key = `${group}.${code}`, value = t(key);
  return value === key ? code : value;
}

export function returnOutcomeText(outcome, t) {
  if (outcome.status === 'success') return codeText(t, 'terminal.success', outcome.game_outcome);
  if (outcome.status === 'paused') return t('terminal.paused');
  if (outcome.status === 'failure') {
    const reason = outcome.reason;
    return reason.kind === 'mechanics_or_protocol_error'
      ? t('terminal.failure.mechanics_or_protocol_error', { message: reason.message })
      : codeText(t, 'terminal.failure', reason.kind);
  }
  return JSON.stringify(outcome);
}

export function mapTokenTitle(token, boss, t, gameText) {
  if (token.kind === 'edge') return `(${token.from}) -> (${token.to})`;
  if (token.kind !== 'room') return undefined;
  const current = token.state === 'current';
  if (token.room === 'boss') {
    const name = boss ? codeText(gameText, 'encounter', boss) : t('undisclosedBoss');
    return current ? t('currentBoss', { boss: name }) : name;
  }
  const key = token.emerald ? (current ? 'currentKeyRoom' : 'keyRoom') : (current ? 'currentRoom' : 'room');
  return t(key, {
    room: gameText(`room.${token.room}`), position: token.coordinate,
  });
}
