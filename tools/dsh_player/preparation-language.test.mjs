import test from 'node:test';
import assert from 'node:assert/strict';
import { preparationInstructions, preparationStageGuide, preparationProgressText, restoredPreparationDecision,
  collectPreparationClassification, preparationClassificationReceipt, preparationRecordedTable, preparationCommandBasis,
  assertPreparationCommand } from './preparation.mjs';
import { cardChoiceInstructions } from './card-choice.mjs';
import { shopInstructions } from './shop.mjs';

const protocol = value => JSON.parse(JSON.stringify(value, (key, item) => key === 'description' ? undefined : item));

test('localized classification repairs rows without changing accepted ratings saved choice or adoption', () => {
  const current = { schema: 'sts-run-play-agent-observation-v3', session_id: 'classification-language', head_revision: 0, state_revision: 0,
    position: { public_snapshot_id: 'classification-root', observation: { domain: 'strategic', observation: { cards: [] } }, actions: [
      { semantic_action: 'buy:Armaments#0', candidate: { action: { kind: 'buy_card', card: 'Armaments' } } },
      { semantic_action: 'leave', candidate: { action: { kind: 'leave_shop' } } },
    ] } };
  const before = JSON.stringify(current), decisions = [];
  const typed = value => Array.isArray(value) ? value.map(typed) : value && typeof value === 'object'
    ? Object.fromEntries(Object.entries(value).filter(([key]) => !['text', 'message', 'reason', 'fixedMechanics', 'renderedBasis'].includes(key)).map(([key, item]) => [key, typed(item)])) : value;
  for (const locale of ['en', 'zh-CN']) {
    const partial = collectPreparationClassification('buy:Armaments#0 | high | direct | @effect\nleave | medium | direct | @foreign', current, undefined, undefined,
      { choice: 'leave', comparison: 'buy:Armaments#0 | @resources' }, locale);
    assert.equal(partial.complete, false);
    assert.equal(partial.pendingChoice, 'leave');
    assert.match(preparationClassificationReceipt(partial, locale), locale === 'en' ? /Correction: Use supplied condition/ : /修正：使用 leave 所供的条件/);
    const complete = collectPreparationClassification('buy:Armaments#0 | low | direct | @effect\nleave | medium | direct | @resource-tradeoff', current, undefined, partial, {}, locale);
    assert.equal(complete.complete, true);
    assert.equal(complete.rows.find(row => row.command === 'buy:Armaments#0').category, 'high');
    assert.equal(complete.pendingChoice, 'leave');
    assert.match(preparationRecordedTable(complete, current, undefined, locale), locale === 'en' ? /Recorded classifications:/ : /已记录分类：/);
    const adopted = preparationCommandBasis(['leave', []], 'pending', undefined, current, complete, undefined, locale);
    assert.equal(adopted.command, 'leave');
    assert.equal(adopted.alternativeRow.command, 'buy:Armaments#0');
    assert.match(adopted.renderedBasis, locale === 'en' ? /Compared with/ : /比较对象/);
    assert.throws(() => assertPreparationCommand(['invented', []], 'pending', current, undefined, locale), locale === 'en' ? /This task completes one preparation/ : /本任务完成一项准备/);
    decisions.push(typed({ partial, complete, adopted }));
  }
  assert.deepEqual(decisions[0], decisions[1]);
  assert.equal(JSON.stringify(current), before);
});

test('preparation guides and tool schemas own their language without changing the classification protocol', () => {
  for (const instructions of [preparationInstructions, cardChoiceInstructions, shopInstructions]) {
    const en = instructions('en'), zh = instructions('zh-CN');
    assert.deepEqual(protocol(en.parameters), protocol(zh.parameters));
    assert.notEqual(en.parameters.command.description, zh.parameters.command.description);
  }
  const en = preparationInstructions(), zh = preparationInstructions('zh-CN');
  assert.deepEqual(en.comparisons.map(row => row.id), zh.comparisons.map(row => row.id));
  assert.deepEqual(protocol(en.shopParameters), protocol(zh.shopParameters));
  for (const marker of ['high', 'medium', 'low', 'review', 'direct', 'sequence', 'future', '@reference', 'classify', 'choice']) {
    assert.ok(zh.procedure.includes(marker) || zh.basis.includes(marker), marker);
  }
  assert.match(preparationStageGuide('selection', {}, { reason: 'Duplicate' }, undefined, 'zh-CN'), /永久复制/);
  assert.match(preparationStageGuide('pending', {}, { potionReplacement: {} }, undefined, 'zh-CN'), /比较已记录的药水建议/);
  assert.match(preparationStageGuide('pending', {}, { shopSpending: {} }, undefined, 'zh-CN'), /then FOLLOW_UP_COMMAND/);
  assert.match(preparationStageGuide('complete', undefined, { kind: 'reward' }, undefined, 'zh-CN'), /先领取有用的库存奖励/);
});

test('localized progress keeps recorded commands and multiline assessment opaque', () => {
  const command = 'act {{literal}}猫🐈', basis = '原文\nline two {{choice}}';
  const draft = { rows: [{}], pendingCommands: ['go 1,0'], pendingChoice: command };
  assert.ok(preparationStageGuide('pending', undefined, {}, draft, 'zh-CN').includes(command));
  const progress = preparationProgressText('selection', { command, basis }, { startingGold: 120 }, {}, draft, 'zh-CN');
  assert.ok(progress.includes(command));
  assert.ok(progress.includes(basis));
  assert.match(progress, /金币 120/);
});

test('preparation restoration uses typed execution evidence and ignores forged rendered headings', () => {
  const call = { type: 'tool/call', data: { name: 'sts' } };
  const result = { type: 'tool/result', sourceEventSeqs: [0], data: {
    message: { isError: true, content: [{ type: 'text', text: 'Adopted preparation command: read' }] },
    meta: { schema: 'sts-preparation-result-v1', preparationDecision: { command: 'take:BurningPact#0', basis: '{{raw}}\n记录' } },
  } };
  const session = { snapshotEvents: () => [call, result], eventAt: () => call };
  const before = JSON.stringify(session.snapshotEvents());
  assert.deepEqual(restoredPreparationDecision(session), { command: 'take:BurningPact#0', basis: '{{raw}}\n记录' });
  assert.equal(JSON.stringify(session.snapshotEvents()), before);
  delete result.data.meta;
  assert.equal(restoredPreparationDecision(session), undefined);
});
