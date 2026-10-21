import assert from 'node:assert/strict';
import { mkdtemp, writeFile, rm, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname, basename } from 'node:path';
import test from 'node:test';
import { inspect } from 'node:util';
import { DecisionOperationError, openDecisionWorkspace, openLiveSession } from './decision_client.mjs';

// Protocol fixture only: tests transport lifetime and source binding, never
// game legality, workspace persistence or Java parity. The real Rust owner
// tests and analyst tool-use acceptance cover those separately.
const fixtureSource = `
const fs = require('node:fs');
const fixtureChoices = JSON.parse(fs.readFileSync('choices.json', 'utf8'));
const lines = require('node:readline').createInterface({input:process.stdin});
fs.closeSync(fs.openSync('writer.lock', 'wx'));
let revision = 0, selected = 0;
lines.on('line', line => {
  const request = JSON.parse(line);
  const stale = !['observe','close'].includes(request.operation.op) && request.view !== 'V'+revision;
  const forced = request.operation.op === 'adopt' && ['partial','stopped'].includes(request.operation.proposal)
    ? request.operation.proposal : null;
  if (forced === 'partial') { selected++; revision++; }
  if (!stale && ['choose','choose_turn','undo'].includes(request.operation.op)) {
    selected += request.operation.op === 'undo' ? -1 : 1;
    revision++;
  }
  setTimeout(() => {
    process.stdout.write(JSON.stringify({
      id:request.id, status:forced ?? (stale ? 'rejected' : 'ok'), workspace:'Wfixture',
      view:'V'+revision, selected_actions:selected, stop:'fixture',
      choices:{actions:fixtureChoices.map((action,index) => ({handle:'V'+revision+':'+index,action}))},
      submitted_reference:request.operation.selector?.reference,
      inspected_section:request.operation.section,
      operation:request.operation,
      research: request.operation.op === 'research_turns' ? {
        summary:'turn research fixture', total:1,
        after:request.operation.after ? 'selected prefix' : null,
        options:[{reference:'V'+revision+':turn:fixture:'+(request.operation.after ? '1' : '0'),
          label:'Apparition -> End turn',prefix_action_count:request.operation.start === 'before_end_turn' ? 0 : (request.operation.after ? 2 : 0)}]
      } : undefined,
      turn: request.operation.op === 'inspect_turn' ? {summary:'turn detail fixture'} : undefined,
      startup_arguments:process.argv.slice(2),
      choice_labels:Object.fromEntries([0,1].map(index => ['V'+revision+':'+index, 'Choose Strike'])),
      execution:{state:request.operation.op === 'close' ? 'parked' : 'ready',parity:'Exact'},
      diagnostic_log:'fixture diagnostic only',
      summary:'transport fixture', elapsed_micros:1
    })+'\\n');
    if(request.operation.op === 'close') {
      fs.unlinkSync('writer.lock');
      lines.close();
      process.stdin.destroy();
    }
  }, 30);
});


`;

async function fixture(t, actions = [0,1].map(() => ({kind:'submit_selection',choices:[{attributes:{card_id:'strike'}}]}))) {
  const temporaryRoot = await realpath(tmpdir());
  const repository = await mkdtemp(join(temporaryRoot, 'decision-transport-'));
  await writeFile(join(repository, 'choices.json'), JSON.stringify(actions));
  await writeFile(join(repository, 'workspace'), fixtureSource);
  await writeFile(join(repository, 'manual-resume'), fixtureSource);
  t.after(async () => {
    const resolved = await realpath(repository);
    assert.equal(dirname(resolved), temporaryRoot);
    assert.ok(basename(resolved).startsWith('decision-transport-'));
    await rm(resolved, {recursive:true});
  });
  return {repository, binary:process.execPath};
}

async function rejected(promise, status = 'rejected') {
  let receipt;
  await assert.rejects(promise, error => {
    assert.ok(error instanceof DecisionOperationError);
    assert.equal(error.status, status);
    receipt = error.view;
    return true;
  });
  return receipt;
}

test('turn trials keep read-only summaries compact and candidate callbacks source-bound', async t => {
  const options = await fixture(t);
  const connection = openDecisionWorkspace(options);
  t.after(() => connection.close().catch(() => {}));
  const initial = await connection.observe();
  for (const invalid of [{steps:2}, {outcome:'win'}, {outcome:[]}, {outcome:0}, {outcome:''},
    {minimumHp:2**40}, {minimumBlock:-1}, {minimumBlock:2**40}, {minimumBlock:'30'},
    {enemyHpAtMost:{slot:0,hp:10,extra:1}}, {potionSlots:[0,0]},
    {playerPowers:null}, {playerPowers:[{}]}, {playerPowers:[{power:'Weak',present:1}]},
    {playerPowers:[{power:'Weak',minimumAmount:2**40}]}, {playerPowers:[{power:'Weak',present:false,minimumAmount:0}]},
    {playerPowers:[{power:'Weak',amount:1}]}, {playerPowers:Array(9).fill({power:'Weak'})}]) {
    assert.throws(() => initial.researchTurns(invalid));
  }
  assert.equal(connection.statistics().requestsSent, 1);
  const researched = await initial.researchTurns();
  assert.equal(researched.diagnostics().operation.request.limit, 0);
  assert.equal(researched.research.total, 1);
  assert.equal(JSON.stringify(researched.research), '"turn research fixture"');
  assert.ok(!inspect(researched.turns).includes(':turn:fixture'));
  const candidate = researched.turns[0];
  const future = await candidate.researchNextTurn({work:0,minimumHp:35,minimumBlock:12,outcome:'victory'});
  assert.equal(future.selectedActions, 0);
  assert.equal(future.diagnostics().operation.after, candidate.reference);
  assert.equal(future.diagnostics().operation.request.work, 0);
  assert.equal(future.diagnostics().operation.request.limit, 0);
  assert.equal(future.diagnostics().operation.request.outcome, 'victory');
  assert.equal(future.diagnostics().operation.request.minimum_block, 12);
  const expandedFuture = await candidate.researchNextTurn({work:0,limit:2});
  assert.equal(expandedFuture.diagnostics().operation.request.limit, 2);
  assert.equal(future.turns[0].researchNextTurn, undefined);
  assert.equal(future.turns[0].researchBeforeEndTurn, undefined);
  const remainder = await candidate.researchBeforeEndTurn({work:0,minimumBlock:20,limit:1});
  assert.equal(remainder.selectedActions, 0);
  assert.equal(remainder.diagnostics().operation.after, candidate.reference);
  assert.equal(remainder.diagnostics().operation.start, 'before_end_turn');
  assert.equal(remainder.diagnostics().operation.request.minimum_block, 20);
  assert.equal(remainder.turns[0].prefix_action_count, 0);
  assert.equal(remainder.turns[0].researchNextTurn, undefined);
  assert.equal(remainder.turns[0].researchBeforeEndTurn, undefined);
  assert.notEqual(future.turns[0].reference, candidate.reference);
  assert.throws(() => candidate.researchNextTurn({playerPowers:null}), /unknown|invalid/);
  const detail = await candidate.inspect();
  assert.equal(detail.diagnostics().operation.reference, candidate.reference);
  assert.equal(detail.selectedActions, 0);
  assert.equal(JSON.stringify(detail.turn), '"turn detail fixture"');
  const filtered = await researched.researchTurns({work:0,minimumHp:40,enemyHpAtMost:{slot:2,hp:10}});
  assert.deepEqual(filtered.diagnostics().operation.request.enemy_hp_at_most, {slot:2,hp:10});
  assert.equal(filtered.diagnostics().operation.request.limit, 6);
  const block = await researched.researchTurns({work:0,minimumBlock:30});
  assert.equal(block.diagnostics().operation.request.minimum_block, 30);
  assert.equal(block.diagnostics().operation.request.limit, 6);
  for (const outcome of ['next_turn', 'victory', 'defeat', 'escape']) {
    const outcomes = await researched.researchTurns({work:0,outcome});
    assert.equal(outcomes.diagnostics().operation.request.outcome, outcome);
    assert.equal(outcomes.diagnostics().operation.request.limit, 6);
    assert.equal(outcomes.selectedActions, 0);
  }
  const powers = await researched.researchTurns({work:0,playerPowers:[
    {power:'Dark Embrace'}, {power:'Weak',present:false}, {power:'Dexterity',minimumAmount:2},
  ]});
  assert.equal(powers.diagnostics().operation.request.limit, 6);
  assert.deepEqual(powers.diagnostics().operation.request.player_powers, [
    {power:'Dark Embrace',present:true,minimum_amount:null},
    {power:'Weak',present:false,minimum_amount:null},
    {power:'Dexterity',present:true,minimum_amount:2},
  ]);
  const child = future.turns[0];
  assert.equal((await child.inspect()).diagnostics().operation.reference, child.reference);
  const chosen = await child.choose();
  assert.equal(chosen.selectedActions, 1);
  assert.equal(chosen.diagnostics().operation.reference, child.reference);
  await rejected(candidate.inspect());
  await rejected(candidate.researchNextTurn());
  await rejected(candidate.researchBeforeEndTurn());
  await rejected(child.choose());
  await rejected(candidate.choose());
  await rejected(researched.researchTurns());
  await connection.close();
  const live = openLiveSession(options);
  t.after(() => live.close().catch(() => {}));
  assert.equal((await live.observe()).researchTurns, undefined);
  await live.close();
});

test('views retain their scope, only one request runs, and close releases the writer', async t => {
  const options = await fixture(t);
  const connection = openDecisionWorkspace(options);
  t.after(() => connection.close().catch(() => {}));
  const first = await connection.observe();
  assert.throws(() => first.choose({reward:'typo'}));
  assert.throws(() => first.choose({card:12}));
  assert.throws(() => first.previewRewards('gold'));
  assert.equal(connection.statistics().requestsSent, 1);
  const mutation = first.choose({reward:'potion'});
  await assert.rejects(connection.observe(), /still pending/);
  const current = await mutation;
  assert.equal(current.selectedActions, 1);
  const stale = await rejected(first.choose({reward:'gold'}));
  assert.equal(stale.status, 'rejected');
  assert.equal(stale.selectedActions, 1);
  await rejected(first.choices[0].choose());
  assert.throws(() => first.switchTo('unbound node'));
  assert.equal((await stale.undo()).selectedActions, 0);
  await connection.close();
  await assert.rejects(current.undo(), /connection ended/);
  const reopened = openDecisionWorkspace(options);
  t.after(() => reopened.close().catch(() => {}));
  assert.equal((await reopened.observe()).status, 'ok');
  await reopened.close();
});

test('map inspection is source-bound and leaves the same choices valid', async t => {
  for (const open of [openDecisionWorkspace, openLiveSession]) {
    const connection = open(await fixture(t));
    t.after(() => connection.close().catch(() => {}));
    const initial = await connection.observe();
    assert.throws(() => initial.inspect('unknown'));
    assert.equal(connection.statistics().requestsSent, 1);
    const map = await initial.inspect('map');
    assert.equal(map.diagnostics().inspected_section, 'map');
    assert.equal((await initial.choices[0].choose()).status, 'ok');
    await rejected(map.inspect('map'));
    await connection.close();
  }
});

test('fixed match cells encode a bounded sequence and keep live choices source-bound', async t => {
  const action = {kind:'select',surface:'event',choice:{kind:'match_card',id:'R3C4',occurrence:0,attributes:{}}};
  const options = await fixture(t, [action]);
  const connection = openDecisionWorkspace(options);
  t.after(() => connection.close().catch(() => {}));
  let view = await connection.observe();
  assert.equal(view.choices[0].cell, 'R3C4');
  for (const cells of [[], ['R0C1'], ['R1C5'], ['r1c1'], Array(11).fill('R1C1')]) {
    assert.throws(() => view.flip(cells));
  }
  assert.equal(connection.statistics().requestsSent, 1);
  view = await view.flip(['R1C1', 'R3C4']);
  assert.deepEqual(view.diagnostics().operation, {op:'flip_match_cells',cells:[0,11]});
  view = await view.choose({cell:'R2C3'});
  assert.deepEqual(view.diagnostics().operation.selector, {kind:'match_cell',cell:6});
  assert.equal((await view.inspect('board')).diagnostics().inspected_section, 'board');
  await connection.close();
  const live = openLiveSession(options);
  t.after(() => live.close().catch(() => {}));
  const original = await live.observe();
  assert.equal(original.flip, undefined);
  assert.equal(original.choose, undefined);
  assert.equal((await original.choices[0].choose()).diagnostics().submitted_reference, original.choices[0].reference);
  await rejected(original.choices[0].choose());
  await live.close();
});

test('focused action inspection keeps the original view binding', async t => {
  for (const open of [openDecisionWorkspace, openLiveSession]) {
    const connection = open(await fixture(t));
    t.after(() => connection.close().catch(() => {}));
    const initial = await connection.observe();
    for (const section of ['potions', 'shop', 'removal']) {
      const focused = await initial.inspect(section);
      assert.equal(focused.diagnostics().inspected_section, section);
      assert.equal(focused.choices[0].reference, initial.choices[0].reference);
    }
    const selected = await initial.choices[0].choose();
    assert.equal(selected.selectedActions, 1);
    await rejected(initial.inspect('potions'));
    await connection.close();
  }
});

test('live boundary restoration sends a single explicit adopted coordinate', async t => {
  const options = await fixture(t);
  for (const at of [{session:'manual-source',actions:0}, {session:'',actions:8}, {session:'manual-source',actions:1.5}, {session:'bad:source',actions:8}]) {
    assert.throws(() => openLiveSession({...options,at}));
  }
  const at = {session:'manual-source', actions:8};
  assert.throws(() => openLiveSession({...options, native:true, at}));
  const connection = openLiveSession({...options,at});
  t.after(() => connection.close().catch(() => {}));
  const current = await connection.observe();
  assert.deepEqual(current.diagnostics().startup_arguments.slice(0,3), ['--at','manual-source:8','--json']);
  await connection.close();
});

test('timeout waits for the same receipt even if close already finished', async t => {
  const options = await fixture(t);
  const connection = openDecisionWorkspace({...options, timeoutMs:1});
  t.after(() => connection.close().catch(() => {}));
  await assert.rejects(connection.observe(), /result is pending/);
  const first = await connection.wait(5000);
  assert.equal(first.status, 'ok');
  assert.equal(connection.statistics().requestsSent, 1);
  await assert.rejects(first.choose({reward:'gold'}), /result is pending/);
  const selected = await connection.wait(5000);
  assert.equal(selected.selectedActions, 1);
  assert.equal(connection.statistics().requestsSent, 2);
  await assert.rejects(connection.close(), /result is pending/);
  const closed = await connection.wait(5000);
  assert.equal(closed.status, 'ok');
  assert.equal((await connection.wait(5000)).selectedActions, 1);
  assert.equal(connection.statistics().requestsSent, 3);
});

test('display separates blocks and hides diagnostics while duplicate choices keep their identity', async t => {
  const options = await fixture(t);
  const output = ['preceding output'];
  const connection = openDecisionWorkspace({...options, display: text => output.push(text)});
  t.after(() => connection.close().catch(() => {}));
  const view = await connection.observe();
  view.show();
  view.show();
  assert.ok(output[1].startsWith('\n') && output[1].endsWith('\n'));
  assert.equal(output[1].trim(), view.summary);
  assert.equal(output[2], output[1]);
  assert.equal(inspect(view).trim(), view.summary);
  assert.equal(JSON.parse(JSON.stringify(view)).trim(), view.summary);
  assert.deepEqual(Object.keys(view), ['summary']);
  assert.equal(view.diagnostics().elapsed_micros, 1);
  const duplicates = view.choices.filter(c => c.card === 'strike');
  assert.equal(duplicates.length, 2);
  assert.deepEqual(Object.keys(duplicates[0]), ['label']);
  assert.equal(inspect(duplicates[0]), 'Choose Strike');
  assert.equal(JSON.parse(JSON.stringify(duplicates[0])), 'Choose Strike');
  assert.notEqual(duplicates[0].reference, duplicates[1].reference);
  const result = await duplicates[1].choose();
  assert.equal(result.diagnostics().submitted_reference, duplicates[1].reference);
  await connection.close();
});

test('live session uses bound objects, keeps execution status separate, and waits for writer close', async t => {
  const options = await fixture(t);
  const output = [];
  const connection = openLiveSession({...options, display:text => output.push(text)});
  t.after(() => connection.close().catch(() => {}));
  const view = await connection.observe();
  assert.equal(view.undo, undefined);
  assert.equal(view.choose, undefined);
  assert.equal(typeof view.adopt, 'function');
  view.show();
  assert.ok(!output[0].includes('Exact'));
  view.showStatus();
  assert.ok(output[1].includes('Exact'));
  assert.equal(view.diagnostics().diagnostic_log, 'fixture diagnostic only');
  assert.equal((await view.choices[1].choose()).status, 'ok');
  await rejected(view.choices[0].choose());
  await rejected(view.adopt('D-saved'));
  const closed = await connection.close();
  assert.equal(closed.execution.state, 'parked');
  const reopened = openLiveSession(options);
  t.after(() => reopened.close().catch(() => {}));
  assert.equal((await reopened.observe()).status, 'ok');
  await reopened.close();
});

test('card access covers upgrades, shop cards and removals without replacing instance identity', async t => {
  const cases = [
    [{kind:'select',surface:'campfire',choice:{kind:'deck_card',id:'reward/room/0',attributes:{card_id:'corruption'}}}, 'corruption'],
    [{kind:'select',surface:'shop',choice:{kind:'deck_card',id:'starter/2',attributes:{card_id:'strike'}}}, 'strike'],
    [{kind:'select',surface:'shop',choice:{kind:'deck_card',id:'starter/3',attributes:{card_id:'strike'}}}, 'strike'],
    [{kind:'select',surface:'shop',choice:{kind:'shop_card',id:'powerthrough'}}, 'powerthrough'],
    [{kind:'select',choice:{kind:'reward_card',id:'offering'}}, 'offering'],
    [{kind:'select',choice:{kind:'discovery_card',id:'feelnopain'}}, 'feelnopain'],
    [{kind:'select',choice:{kind:'deck_card',id:'opaque-instance'}}, undefined],
    [{kind:'select',choice:{kind:'event_option',id:'unrelated',attributes:{card_id:'strike'}}}, undefined],
    [{kind:'submit_selection',choices:[{attributes:{card_id:'strike'}},{attributes:{card_id:'defend'}}]}, undefined],
  ];
  for (const open of [openLiveSession, openDecisionWorkspace]) {
    const connection = open(await fixture(t, cases.map(([action]) => action)));
    t.after(() => connection.close().catch(() => {}));
    const view = await connection.observe();
    assert.deepEqual(view.choices.map(choice => choice.card), cases.map(([,card]) => card));
    const duplicates = view.choices.filter(choice => choice.card === 'strike');
    assert.notEqual(duplicates[0].reference, duplicates[1].reference);
    const result = await duplicates[1].choose();
    assert.equal(result.diagnostics().submitted_reference, duplicates[1].reference);
    await rejected(duplicates[0].choose());
    await connection.close();
  }
});

test('failed operations interrupt a sequence and retain the actual receipt without resending', async t => {
  const connection = openLiveSession(await fixture(t));
  t.after(() => connection.close().catch(() => {}));
  const first = await connection.observe();
  const current = await first.choices[0].choose();
  let continued = false;
  const stoppedSequence = rejected((async () => {
    await first.choices[1].choose();
    continued = true;
    await current.choices[1].choose();
  })());
  const failed = await stoppedSequence;
  assert.equal(continued, false);
  assert.equal(failed.selectedActions, 1);
  const requests = connection.statistics().requestsSent;
  assert.equal((await rejected(connection.wait())).selectedActions, 1);
  assert.equal(connection.statistics().requestsSent, requests);
  for (const status of ['partial', 'stopped']) {
    const view = await connection.observe();
    const result = await rejected(view.adopt(status), status);
    assert.equal(result.selectedActions, 2);
    assert.equal((await connection.observe()).selectedActions, 2);
  }
  await connection.close();
});
