import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { catalogs, createPresentation, preferredLocale, resolveLocale, validateCatalogs } from './index.mjs';

test('shipped dictionaries have identical keys and named parameters', () => {
  assert.equal(validateCatalogs(), true);
  const missing = structuredClone(catalogs); delete missing['zh-CN'].replay.nav.previous;
  assert.throws(() => validateCatalogs(missing), /Translation keys differ/);
  const wrong = structuredClone(catalogs); wrong['zh-CN'].game.effect.damage = '伤害 {{amount}}';
  assert.throws(() => validateCatalogs(wrong), /Translation parameters differ/);
});

test('locale selection maps the host zh and browser tags without claiming Traditional Chinese', () => {
  assert.equal(resolveLocale('zh'), 'zh-CN');
  assert.equal(resolveLocale('zh-Hans-CN'), 'zh-CN');
  assert.equal(resolveLocale('en-US'), 'en');
  assert.equal(resolveLocale('zh-Hant'), 'en');
  assert.equal(preferredLocale(['fr-FR', 'zh-CN']), 'zh-CN');
  assert.equal(preferredLocale(['en-GB', 'zh-CN']), 'en');
});

test('names are keyed by exact typed identity; changing one view leaves other views alone', async () => {
  const first = await createPresentation({ locale: 'zh-CN' }), second = await createPresentation({ locale: 'en' });
  assert.equal(first.entity('card', 'ShrugItOff'), '耸肩无视');
  assert.equal(second.entity('card', 'ShrugItOff'), 'Shrug It Off');
  assert.equal(first.entity('card', 'Strike'), first.entity('card', 'StrikeG'));
  await first.setLocale('en');
  assert.equal(first.entity('card', 'StrikeG'), 'Strike');
  await first.setLocale('zh-CN');
  assert.equal(second.locale, 'en');
});

test('plural and number formatting uses the chosen locale; missing parameters fail', async () => {
  const display = await createPresentation({ locale: 'en' });
  assert.equal(display.text('game:effect.draw', { count: 1 }), 'Draw 1 card.');
  assert.equal(display.text('game:effect.draw', { count: 2 }), 'Draw 2 cards.');
  assert.equal(display.text('game:effect.damage', { damage: 9 }), 'Deal 9 damage.');
  assert.throws(() => display.text('game:effect.damage'), /Missing translation parameter/);
  assert.throws(() => display.text('replay:unknown'), /Unknown presentation message/);
  await display.setLocale('zh-CN');
  assert.equal(display.text('game:effect.draw', { count: 2 }), '抽 2 张牌');
  assert.equal(display.number(12345), new Intl.NumberFormat('zh-CN').format(12345));
});

test('missing entity translations fall back once, while unknown IDs stay recognizable', async () => {
  const resources = structuredClone(catalogs), gaps = [];
  delete resources['zh-CN'].game.card.Bash;
  const display = await createPresentation({ locale: 'zh-CN', resources, onMissing: gap => gaps.push(gap) });
  assert.equal(display.entity('card', 'Bash'), 'Bash');
  assert.equal(display.entity('card', 'Bash'), 'Bash');
  assert.equal(display.entity('card', 'UnknownFutureCard'), 'UnknownFutureCard');
  assert.deepEqual(gaps.map(gap => gap.fallback), ['en', 'id']);
});

test('entity keys match the existing simulator vocabularies', async () => {
  const owners = {
    card: '../../src/content/cards/mod.rs',
    encounter: '../../src/content/monsters/factory.rs',
    enemy: '../../src/content/monsters/mod.rs',
    relic: '../../src/content/relics/identity_v1.json',
    power: '../../src/content/powers/mod.rs',
    potion: '../../src/content/potions/mod.rs',
    potionRule: '../../src/content/potions/mod.rs',
    event: '../../src/state/events/mod.rs',
  };
  for (const [kind, owner] of Object.entries(owners)) {
    const source = await readFile(new URL(owner, import.meta.url), 'utf8');
    const ids = kind === 'relic' ? JSON.parse(source).entries.map(entry => entry.stable_id)
      : [...source.matchAll(/=>\s*\("([^\"]+)",\s*\d+\)/g)].map(match => match[1]);
    for (const id of Object.keys(catalogs.en.game[kind])) assert.ok(ids.includes(id), `${kind}.${id} missing from ${owner}`);
    if (kind === 'potion' || kind === 'potionRule') assert.deepEqual(Object.keys(catalogs.en.game[kind]).sort(), ids.sort(), 'all potion names and rules are required');
  }
});

test('potion names use the simulator identity when it differs from a displayed or original-game name', async () => {
  const display = await createPresentation({ locale: 'en' });
  assert.equal(display.entity('potion', 'SteroidPotion'), 'Flex Potion');
  assert.equal(display.entity('potion', 'WeakenPotion'), 'Weak Potion');
  assert.equal(display.entity('potion', 'Elixir'), 'Elixir');
  await display.setLocale('zh-CN');
  assert.equal(display.entity('potion', 'SteroidPotion'), '类固醇药水');
  assert.equal(display.entity('potion', 'WeakenPotion'), '虚弱药水');
  assert.equal(display.entity('potion', 'Elixir'), '万灵药水');
});
