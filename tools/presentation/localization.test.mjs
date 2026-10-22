import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { catalogs, createPresentation, preferredLocale, resolveLocale, validateCatalogs } from './index.mjs';

test('local formatters are ready synchronously and remain independent across awaited consumers', async () => {
  const zh = createPresentation({ locale: 'zh-CN' }), en = createPresentation();
  assert.equal(zh.then, undefined);
  assert.match(zh.text('workbench:command.decisionChanged'), /局面已改变/);
  assert.equal(await zh, zh);
  assert.match(en.text('workbench:command.decisionChanged'), /decision changed/);
  await zh.setLocale('en');
  assert.equal(zh.text('workbench:command.decisionChanged'), en.text('workbench:command.decisionChanged'));
});

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

test('definition text resolves exact entity references without interpreting substituted names', async () => {
  const resources = structuredClone(catalogs);
  resources.en.game.card.HeavyBlade = '{{literal}} $t(replay:nav.previous)';
  const display = await createPresentation({locale:'en',resources});
  assert.ok(display.definitionText('game:powerRule.Strength.effect').includes('{{literal}} $t(replay:nav.previous)'));
  await display.setLocale('zh-CN');
  assert.ok(display.definitionText('game:powerRule.Strength.effect').includes('重刃'));
  assert.throws(() => display.definitionText('powerRule.Strength.effect'), /namespaced key/);
  assert.throws(() => display.definitionText('game:effect.damage'), /Invalid definition reference/);
  assert.throws(() => display.definitionText('game:powerRule.Unknown.effect'), /Unknown presentation message/);
  for (const [template, error] of [
    ['{{entity_card_UnknownCard}}', /Unknown definition entity/],
    ['{{entity_effect_damage}}', /Invalid definition reference/],
    ['{{entity_card_Strike', /Invalid definition template/],
    ['{{entity card Strike}}', /Invalid definition reference/],
  ]) {
    const bad = structuredClone(catalogs);
    bad.en.game.powerRule.Strength.effect = template;
    const view = await createPresentation({locale:'en',resources:bad});
    assert.throws(() => view.definitionText('game:powerRule.Strength.effect'), error);
  }
  const broken = structuredClone(catalogs);
  for (const locale of ['en','zh-CN']) broken[locale].game.powerRule.Strength.effect = '{{entity_card_UnknownCard}}';
  assert.throws(() => validateCatalogs(broken), /Unknown definition entity/);
});

test('every shipped power definition has complete bilingual fields and resolves all references', async () => {
  for (const locale of ['en','zh-CN']) {
    const display = await createPresentation({locale});
    for (const [id,rule] of Object.entries(catalogs[locale].game.powerRule)) {
      assert.deepEqual(Object.keys(rule).sort(), ['amountUnit','effect','timing']);
      for (const field of Object.keys(rule)) {
        const text = display.definitionText(`game:powerRule.${id}.${field}`);
        assert.ok(text.length && !text.includes('{{'), `${locale}:${id}:${field}`);
        if (locale === 'zh-CN') assert.match(text, /[\u4e00-\u9fff]/);
      }
    }
  }
});

test('all reviewed card rules share stable identities and resolve official cross references', () => {
  for (const locale of ['en', 'zh-CN']) {
    const display = createPresentation({ locale });
    assert.equal(Object.keys(catalogs[locale].game.cardRule).length, 136);
    for (const id of Object.keys(catalogs[locale].game.cardRule)) {
      assert.equal(typeof catalogs[locale].game.card[id], 'string', id);
      const rule = display.definitionText(`game:cardRule.${id}`);
      assert.ok(rule && !rule.includes('{{'), `${locale}:${id}`);
      if (locale === 'zh-CN') assert.match(rule, /[\u4e00-\u9fff]/);
    }
    const rule = display.definitionText('game:cardRule.Reaper');
    assert.ok(rule.includes(display.entity('relic', 'MagicFlower')));
    assert.ok(rule.includes(display.entity('power', 'Invincible')));
  }
});

test('all reviewed relic definitions use official cross references in both languages', () => {
  for (const locale of ['en', 'zh-CN']) {
    const display = createPresentation({ locale });
    assert.equal(Object.keys(catalogs[locale].game.relicRule).length, 150);
    for (const id of Object.keys(catalogs[locale].game.relicRule)) {
      assert.equal(typeof catalogs[locale].game.relic[id], 'string', id);
      const text = display.definitionText(`game:relicRule.${id}`);
      assert.ok(text && !text.includes('{{'), `${locale}:${id}`);
      if (locale === 'zh-CN') assert.match(text, /[\u4e00-\u9fff]/);
    }
    const flower = display.definitionText('game:relicRule.MagicFlower');
    assert.ok(flower.includes(display.entity('card', 'Reaper')));
    assert.ok(flower.includes(display.entity('relic', 'EternalFeather')));
  }
});

test('enemy rules combine original move identities with literal owner-provided values', () => {
  for (const locale of ['en', 'zh-CN']) {
    const display = createPresentation({ locale });
    assert.equal(Object.keys(catalogs[locale].game.enemyMove).length, 61);
    assert.equal(display.definitionText('game:enemyRule.lagavulin.siphonSoul'), display.entity('enemyMove', 'Lagavulin_SiphonSoul'));
    if (locale === 'zh-CN') assert.notEqual(display.entity('enemyMove', 'Looter_Mug'), display.entity('enemyMove', 'Mugger_Mug'));
    const value = '{{literal}} $t(game:power.Strength)';
    const rule = display.definitionText('game:enemyRule.exploder.explosionRule', { count: value, damage: 30 });
    assert.ok(rule.includes(value) && rule.includes('30'));
    assert.throws(() => display.definitionText('game:enemyRule.exploder.explosionRule', { count: 3 }), /Invalid definition reference/);
  }
});

test('generated name sections match their recorded original-resource import', async () => {
  const source = JSON.parse(await readFile(new URL('./original-names.json', import.meta.url), 'utf8'));
  assert.equal(source.schema, 'sts-original-localization-names-v1');
  assert.equal(Object.keys(source.entries_sha256).length, 18);
  for (const locale of ['en', 'zh-CN']) {
    const names = {};
    for (const [kind, count] of Object.entries(source.name_counts[locale])) {
      const entries = Object.entries(catalogs[locale].game[kind]).sort(([left], [right]) => left < right ? -1 : left > right ? 1 : 0);
      assert.equal(entries.length, count, `${locale}:${kind}`);
      names[kind] = Object.fromEntries(entries);
    }
    const sha = createHash('sha256').update(JSON.stringify(names, null, 2) + '\n').digest('hex');
    assert.equal(sha, source.generated_names_sha256[locale], locale);
  }
  assert.equal(catalogs['zh-CN'].game.card.SwordBoomerang, '飞剑回旋镖');
  assert.equal(catalogs['zh-CN'].game.encounter.TheChamp, '第一勇士');
  assert.equal(catalogs['zh-CN'].game.encounter.Automaton, '铜制机械人偶');
  assert.equal(catalogs['zh-CN'].game.encounter.Cultist, '邪教徒');
  assert.equal(catalogs['zh-CN'].game.encounter.ThreeSentries, '三哨卫');
  assert.equal(catalogs['zh-CN'].game.encounter.ShieldAndSpear, '矛与盾');
  assert.equal(source.name_counts.en.encounter, 65);
  assert.equal(catalogs.en.game.enemy.LouseNormal, catalogs.en.game.enemy.LouseDefensive);
  assert.ok(source.omitted['card.Impulse']);
  assert.equal(catalogs.en.game.card.Impulse, undefined);
  assert.equal(source.name_counts.en.event, 53);
  assert.equal(catalogs['zh-CN'].game.event.Ssssserpent, '蛇～');
  assert.equal(catalogs.en.game.event.DrugDealer, 'Augmenter');
  assert.equal(catalogs.en.game.event.BonfireSpirits, undefined);
});

test('consumed original event scenes and button fragments retain source provenance', async () => {
  const original = JSON.parse(await readFile(new URL('./original-event-text.json', import.meta.url), 'utf8'));
  const names = JSON.parse(await readFile(new URL('./original-names.json', import.meta.url), 'utf8'));
  assert.equal(original.schema, 'sts-original-event-text-v1');
  for (const [entry, sha] of Object.entries(original.entries_sha256)) assert.equal(names.entries_sha256[entry], sha);
  for (const locale of ['en', 'zh-CN']) {
    assert.equal(Object.keys(original.events[locale]).length, 31);
    assert.equal(original.events[locale].FaceTrader.DESCRIPTIONS.length, 5);
    assert.ok(original.events[locale].FaceTrader.DESCRIPTIONS[0].includes('NL'));
    assert.equal(original.events[locale].FaceTrader.OPTIONS.length, 6);
    assert.equal(original.events[locale].Lab.OPTIONS.length, 1);
    assert.equal(original.events[locale].BigFish.DESCRIPTIONS, undefined);
    assert.equal(Object.values(original.events[locale]).reduce((sum,event) => sum + event.OPTIONS.length,0), 162);
    assert.equal(original.events[locale].Addict.OPTIONS[6], '');
    assert.equal(original.events[locale].Ghosts.OPTIONS[4], '');
    assert.equal(original.events[locale].Neow.REWARD_TEXT.length, 32);
    assert.equal(original.events[locale].Neow.UNIQUE_REWARDS.length, 1);
  }
  assert.notEqual(original.events.en.FaceTrader.DESCRIPTIONS[0], original.events['zh-CN'].FaceTrader.DESCRIPTIONS[0]);
});

test('original card templates retain their source fields without entering interface messages', async () => {
  const source = JSON.parse(await readFile(new URL('./original-card-text.json', import.meta.url), 'utf8'));
  assert.equal(source.schema, 'sts-original-card-text-v1');
  for (const locale of ['en', 'zh-CN']) {
    assert.equal(Object.keys(source.cards[locale]).length, 370);
    assert.deepEqual(Object.keys(source.cards[locale]).sort(), Object.keys(catalogs[locale].game.card).sort());
    assert.equal(source.cards[locale].Impulse, undefined);
  }
  assert.equal(source.cards.en.BodySlam.UPGRADE_DESCRIPTION, ' NL (Deals !D! damage.)');
  assert.ok(source.cards['zh-CN'].ShrugItOff.DESCRIPTION.includes('!B!'));
  assert.equal(catalogs.en.mechanics.card.energy, '[Energy]');
  assert.equal(catalogs['zh-CN'].mechanics.card.energy, '[能量]');
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
  assert.equal(first.cardName('SearingBlow', 1), '灼热攻击+1');
  assert.equal(first.cardName('StrikeG', 1), '打击+');
  assert.equal(second.cardName('SearingBlow', 1), 'Searing Blow+1');
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
    stance: '../../src/runtime/combat/orbs.rs',
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
