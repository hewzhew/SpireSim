import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile, readFile, readdir, rm, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, sep } from 'node:path';
import { openRunPlay, getRunPlay, RunPlayCommandError } from './client.mjs';

// Transport fixture only. Simulator semantics are exercised by Rust tests.
const server = `
const lines = require('node:readline').createInterface({ input: process.stdin });
let revision = 0;
lines.on('line', line => {
  const r = JSON.parse(line);
  if (r.details?.includes('map')) return;
  if (r.details?.includes('state')) { process.stdout.write('\\x1b]0;bad\\x07{}\\n'); return; }
  let status = 'ok', result = {};
  if (r.command === 'choose') {
    if (r.selection.source.revision !== revision) status = 'error';
    else revision++;
  }
  if (r.command === 'commit_combat_plan') revision++;
  if (r.command === 'combat_research') result = { report_id:'test-report', view: 'c0: HP 70/75\\n', candidates: [{candidate_ref:'c0',plan_id:'c0@4bf5ca407322'}] };
  else if (r.command === 'combat_review') result = {view:'Turn '+(r.turn ?? 1)+' | Strike @M0\\n'};
  else result = {reading:{view:'Decision '+revision+'\\n', summary:'当前位置 '+revision+'\\n', control:'strategic', source:{revision},
    context:{deck:'retained-deck'}, options:[{label:'Pick',choice:{ordinal_path:[3]}}],
    selections:[{label:'Discard',choice:{ordinal_path:[7]},min:0,max:2,
      members:[{ordinal:1,label:'Strike'},{ordinal:4,label:'Strike'}]}]},
    source:r.selection?.source, choice:r.selection?.choice, known:r.known_context,
    stdoutIsTTY:Boolean(process.stdout.isTTY)};
  setTimeout(() => process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',
    request_id:r.request_id,status,result,error:{message:'Decision changed'}})+'\\n'), 10);
});
`;

async function fixture(t, timeoutMs = 5000, extra = {}) {
  const base = await realpath(tmpdir());
  const root = await mkdtemp(join(base, 'run-play-pipe-'));
  const config = { repository: root, session: root, recordDirectory: join(root, 'wire'), binary: process.execPath, timeoutMs, closeTimeoutMs: 200, locale: extra.locale ?? 'en' };
  let client;
  t.after(async () => {
    try {
      if (client) await client.close();
    } catch (error) {
      if (!extra.expectedCloseError) throw error;
    } finally {
      const target = await realpath(root);
      assert.ok(target.startsWith(base + sep) && target.includes('run-play-pipe-'));
      await rm(target, { recursive: true });
    }
  });
  await writeFile(join(root, 'agent-shell'), extra.server ?? server);
  client = await openRunPlay(config);
  return { client, root, config };
}

test('direct pipes retain source objects, concise views and exact evidence', async t => {
  const {client, root} = await fixture(t);
  const first = await client.decision();
  assert.equal(first.raw.stdoutIsTTY, false);
  assert.equal(first.view, 'Decision 0\n');
  assert.equal(first.summary, '当前位置 0\n');
  const next = await first.options[0].choose({retainContext:true});
  assert.deepEqual(next.raw.source, {revision:0});
  assert.deepEqual(next.raw.choice, {ordinal_path:[3]});
  assert.equal(next.raw.known.deck, 'retained-deck');
  await assert.rejects(first.options[0].choose(), /Decision changed/);
  const read = client.decision();
  await assert.rejects(client.decision(), /current operation/);
  await read;
  const files = await readdir(join(root, 'wire'));
  assert.equal(files.length, 4);
  const wire = await readFile(join(root, 'wire', files[0]), 'utf8');
  assert.ok(!wire.includes('\x1b'));
  assert.equal(wire.trim().split('\n').length, 2);
});

test('connection language reaches the native process and defaults definition queries without rewriting requests', async t => {
  const scripted = `
  require('node:readline').createInterface({ input:process.stdin }).on('line', line => {
    const r = JSON.parse(line);
    const result = { view:'original native text\\n', processLocale:process.argv[process.argv.indexOf('--locale')+1], queryLocale:r.locale };
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status:'ok',result})+'\\n');
  });`;
  const { client, root } = await fixture(t, 5000, { server: scripted, locale: 'zh-CN' });
  const chinese = await client.info('card Bash');
  assert.equal(chinese.raw.processLocale, 'zh-CN');
  assert.equal(chinese.raw.queryLocale, 'zh-CN');
  assert.equal(chinese.view, 'original native text\n');
  const english = await client.info('card Bash', { locale: 'en' });
  assert.equal(english.raw.processLocale, 'zh-CN');
  assert.equal(english.raw.queryLocale, 'en');
  const request = JSON.parse((await readFile(join(root, 'wire', 'exchange-000.jsonl'), 'utf8')).split('\n')[0]);
  assert.equal(request.query, 'card Bash');
  assert.equal(request.locale, 'zh-CN');
  await assert.rejects(fixture(t, 5000, { locale: 'fr' }), /Unsupported text locale/);
});

test('observe returns the resident typed observation without starting another process', async t => {
  const scripted = `
  require('node:readline').createInterface({input:process.stdin}).on('line', line => {
    const request=JSON.parse(line);
    const result={kind:'observation',observation:{position:{kind:'decision',public_snapshot_id:'snapshot-1',actions:[]}}};
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:request.request_id,status:'ok',result})+'\\n');
  });`;
  const {client,root}=await fixture(t,5000,{server:scripted});
  const observation=await client.observe();
  assert.equal(observation.position.public_snapshot_id,'snapshot-1');
  assert.equal(Object.isFrozen(observation.position),true);
  const request=JSON.parse((await readFile(join(root,'wire','exchange-000.jsonl'),'utf8')).split('\n')[0]);
  assert.equal(request.command,'observe');
});

test('command errors preserve the server code and leave the connection usable', async t => {
  const scripted = `
  require('node:readline').createInterface({input:process.stdin}).on('line', line => {
    const r=JSON.parse(line);
    const response=r.command==='act'
      ? {status:'error',error:{code:'invalid_action',message:'No legal match for play:Bash'}}
      : {status:'ok',result:{reading:{view:'Still at root\\n',control:'combat',options:[]}}};
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,...response})+'\\n');
  });`;
  const {client,root}=await fixture(t,5000,{server:scripted});
  await assert.rejects(client.act(['play:Bash']), error => {
    assert.ok(error instanceof RunPlayCommandError);
    assert.equal(error.code,'invalid_action');
    assert.equal(error.message,'No legal match for play:Bash');
    assert.equal(error.command,'act');
    assert.equal(error.requestId,'exchange-000');
    assert.equal(error.recordDirectory,join(root,'wire'));
    return true;
  });
  assert.equal((await client.decision()).view,'Still at root\n');
});

test('game facts reuse the resident connection and preserve typed effects', async t => {
  const scripted = `
  require('node:readline').createInterface({input:process.stdin}).on('line', line => {
    const r=JSON.parse(line);
    const result=r.command==='info'
      ? {kind:'info',view:'Dropkick: conditional Energy and draw\\n'}
      : {kind:'game_facts',facts:r.entities.map(entity=>({...entity,draw_cards:0,exhausts:false}))};
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status:'ok',result})+'\\n');
  });`;
  const {client,root}=await fixture(t,5000,{server:scripted});
  const entities=[{kind:'card',card:'Dropkick'},{kind:'potion',potion:'FirePotion'}];
  const facts=await client.gameFacts(entities);
  assert.equal(facts[0].draw_cards,0);
  assert.equal(facts[0].exhausts,false);
  assert.ok(Object.isFrozen(facts[0]));
  const request=JSON.parse((await readFile(join(root,'wire','exchange-000.jsonl'),'utf8')).split('\n')[0]);
  assert.equal(request.command,'game_facts');
  assert.deepEqual(request.entities,entities);
  await assert.rejects(client.gameFacts([]),/1..128/);
  assert.equal((await readdir(join(root,'wire'))).length,1);
  const reading = await client.info('card Dropkick');
  assert.equal(reading.view,'Dropkick: conditional Energy and draw\n');
  assert.equal(reading.options,undefined);
  const infoRequest = JSON.parse((await readFile(join(root,'wire','exchange-001.jsonl'),'utf8')).split('\n')[0]);
  assert.equal(infoRequest.locale, 'en');
  await assert.rejects(client.info(' '),/mechanics query/);
  await assert.rejects(client.info('potion FirePotion', { locale: 'fr' }), /Unsupported text locale/);
  assert.equal((await readdir(join(root,'wire'))).length,2);
  await client.info('potion FirePotion', { locale: 'zh-CN' });
  const localized = JSON.parse((await readFile(join(root,'wire','exchange-002.jsonl'),'utf8')).split('\n')[0]);
  assert.equal(localized.locale, 'zh-CN');
  assert.equal(localized.query, 'potion FirePotion');
});

test('process failure includes the Rust diagnostic and preserves full stderr', async t => {
  const diagnostic = 'error: run-play workbench is stale: rebuild before running\n';
  const scripted = `
  require('node:readline').createInterface({input:process.stdin}).once('line', () => {
    process.stderr.write(${JSON.stringify(diagnostic)}, () => process.exit(1));
  });`;
  const {client,root}=await fixture(t,5000,{server:scripted});
  await assert.rejects(client.decision(), error => {
    assert.equal(error.code,'RUN_PLAY_CONNECTION_ERROR');
    assert.equal(error.exitCode,1);
    assert.equal(error.outcomeUncertain,true);
    assert.equal(error.stderrTail,diagnostic);
    assert.match(error.message,/workbench is stale: rebuild before running/);
    return true;
  });
  await client.close();
  assert.equal(await readFile(join(root,'wire','stderr.txt'),'utf8'),diagnostic);
});

test('long process diagnostics are complete on disk and bounded in the error', async t => {
  const diagnostic = 'first diagnostic\n' + 'detail\n'.repeat(2000) + 'last diagnostic\n';
  const scripted = `
  require('node:readline').createInterface({input:process.stdin}).once('line', () => {
    process.stderr.write(${JSON.stringify(diagnostic)}, () => process.exit(1));
  });`;
  const {client,root}=await fixture(t,5000,{server:scripted});
  await assert.rejects(client.decision(), error => {
    assert.equal(error.stderrTail,diagnostic.slice(-8192));
    assert.match(error.message,/last diagnostic/);
    assert.ok(error.message.length < 1200);
    return true;
  });
  await client.close();
  assert.equal(await readFile(join(root,'wire','stderr.txt'),'utf8'),diagnostic);
});

test('candidate methods keep the search effort and expire after adoption', async t => {
  const {client} = await fixture(t);
  const old = await client.research({effort:'expanded'});
  const plans = await client.research();
  await assert.rejects(old.candidates[0].commit(), /expired/);
  const candidate = plans.candidates[0];
  const review = candidate.review({turn:2});
  await assert.rejects(client.research(), /current operation/);
  assert.match((await review).view, /Turn 2/);
  const rewards = await candidate.commit();
  assert.equal(rewards.view, 'Decision 1\n');
  await assert.rejects(candidate.commit(), /expired/);
  assert.equal((await rewards.options[0].choose()).view, 'Decision 2\n');
});

test('search budget is explicit, bounded and leaves existing handles intact on invalid input', async t => {
  const {client,root}=await fixture(t);
  const plans=await client.research({effort:'expanded',maxSearchMs:750});
  const count=(await readdir(join(root,'wire'))).length;
  for (const maxSearchMs of [0,-1,20001,1.5,'1000',Infinity]) {
    await assert.rejects(client.research({maxSearchMs}),/maxSearchMs/);
  }
  assert.equal((await readdir(join(root,'wire'))).length,count);
  await plans.candidates[0].review();
  const request=JSON.parse((await readFile(join(root,'wire','exchange-000.jsonl'),'utf8')).split('\n')[0]);
  assert.equal(request.max_search_ms,750);
  assert.equal(request.effort,'expanded');
});

test('work budgets are sent unchanged and invalid requests preserve existing handles', async t => {
  const { client, root } = await fixture(t);
  const plans = await client.research({ maxSearchMs: 15000, maxGenerationWork: 100000 });
  const count = (await readdir(join(root, 'wire'))).length;
  for (const maxGenerationWork of [0, -1, 1000001, 1.5, '1000', Infinity]) {
    await assert.rejects(client.research({ maxGenerationWork }), /maxGenerationWork/);
  }
  await assert.rejects(client.research({ effort: 'expanded', maxGenerationWork: 100000 }), /requires whole_combat/);
  assert.equal((await readdir(join(root, 'wire'))).length, count);
  await plans.candidates[0].review();
  const request = JSON.parse((await readFile(join(root, 'wire', 'exchange-000.jsonl'), 'utf8')).split('\n')[0]);
  assert.equal(request.max_search_ms, 15000);
  assert.equal(request.max_generation_work, 100000);
  assert.equal(request.effort, 'whole_combat');
});

test('potion budgets are sent per request and invalid budgets preserve handles', async t => {
  const { client, root } = await fixture(t);
  const plans = await client.research({ maxPotionsUsed: 0 });
  const count = (await readdir(join(root, 'wire'))).length;
  for (const maxPotionsUsed of [-1, 1.5, '0', null, Infinity, 4294967296]) {
    await assert.rejects(client.research({ maxPotionsUsed }), /maxPotionsUsed/);
  }
  assert.equal((await readdir(join(root, 'wire'))).length, count);
  await plans.candidates[0].review();
  const first = JSON.parse((await readFile(join(root, 'wire', 'exchange-000.jsonl'), 'utf8')).split('\n')[0]);
  assert.equal(first.max_potions_used, 0);
  await client.research({ maxPotionsUsed: 2 });
  await client.research();
  const exchanges = await Promise.all((await readdir(join(root, 'wire'))).filter(n => n.endsWith('.jsonl'))
    .sort().map(n => readFile(join(root, 'wire', n), 'utf8')));
  const requests = exchanges.flatMap(s => s.trim().split('\n').map(line => JSON.parse(line)))
    .filter(r => r.command === 'combat_research');
  assert.deepEqual(requests.map(r => r.max_potions_used), [0, 2, undefined]);
});

test('saved report identity and effort survive binding; preview sends only probe', async t => {
  const scripted = `
  const lines = require('node:readline').createInterface({input:process.stdin});
  lines.on('line', line => {
    const r=JSON.parse(line);
    let result={view:'Reviewed',received:r};
    if (r.command==='combat_plans') result={view:'Saved plans',report_id:'saved-report',effort:'expanded',
      candidates:[{candidate_ref:'c0',plan_id:'c0@79128061adc3',plan_steps:2}]};
    if (r.command==='probe') result={durable_head_unchanged:true,
      lines:[{view:'HP 70/75',endpoint:{kind:'decision'},stop:{kind:'complete'}}]};
    if (r.command==='combat_research' || r.command==='act') throw new Error('unexpected execution');
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status:'ok',result})+'\\n');
  });`;
  const {client,root}=await fixture(t,5000,{server:scripted});
  const plans=await client.plans();
  assert.equal(plans.candidates[0].planId, 'c0@79128061adc3');
  const line=['play:Bash+@0','play:Strike@0','end','play:Defend','play:Strike@1','play:Strike@1','end','play:PerfectedStrike@1','play:Anger@1'];
  const preview=await client.preview(line);
  assert.equal(preview.view,'HP 70/75');
  assert.equal(preview.preview,true);
  assert.deepEqual(preview.stop,{kind:'complete'});
  const probe=JSON.parse((await readFile(join(root,'wire','exchange-001.jsonl'),'utf8')).split('\n')[0]);
  assert.equal(probe.command,'probe');
  assert.deepEqual(probe.lines,[line]);
  const reviewed=await plans.candidates[0].review();
  assert.equal(reviewed.raw.received.report_id,'saved-report');
  assert.equal(reviewed.raw.received.effort,'expanded');
  await plans.candidates[0].commit();
  const request=JSON.parse((await readFile(join(root,'wire','exchange-003.jsonl'),'utf8')).split('\n')[0]);
  assert.equal(request.command,'commit_combat_plan');
  assert.equal(request.report_id,'saved-report');
  assert.equal(request.effort,'expanded');
  await assert.rejects(plans.candidates[0].commit(),/expired/);
});

test('full plans reads the same saved report without research and validates before expiring handles', async t => {
  const scripted = `
  require('node:readline').createInterface({input:process.stdin}).on('line', line => {
    const r=JSON.parse(line);
    if (!['combat_plans','combat_review'].includes(r.command)) throw new Error('unexpected execution');
    const result=r.command==='combat_plans'
      ? {view:r.diagnostics?'Full plans':'Plans',report_id:'saved-report',effort:'whole_combat',
        candidates:[{candidate_ref:'c0',plan_id:'c0@79128061adc3',plan_steps:2}]}
      : {view:'Reviewed',received:r};
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status:'ok',result})+'\\n');
  });`;
  const {client,root}=await fixture(t,5000,{server:scripted});
  const brief=await client.plans();
  const full=await client.plans({diagnostics:true});
  assert.equal(full.view,'Full plans');
  assert.equal(full.raw.report_id,brief.raw.report_id);
  assert.deepEqual(full.raw.candidates,brief.raw.candidates);
  const count=(await readdir(join(root,'wire'))).length;
  for (const diagnostics of ['full',1,null]) await assert.rejects(client.plans({diagnostics}), /diagnostics/);
  assert.equal((await readdir(join(root,'wire'))).length,count);
  const reviewed=await full.candidates[0].review();
  assert.equal(reviewed.raw.received.report_id,'saved-report');
  for (const [index,diagnostics] of [[0,undefined],[1,true]]) {
    const request=JSON.parse((await readFile(join(root,'wire',`exchange-00${index}.jsonl`),'utf8')).split('\n')[0]);
    assert.equal(request.command,'combat_plans');
    assert.equal(request.diagnostics,diagnostics);
  }
});

test('missing report identity and malformed preview do not grant executable plans', async t => {
  const scripted = `
  require('node:readline').createInterface({input:process.stdin}).on('line', line => {
    const r=JSON.parse(line);
    const result=r.command==='probe'
      ? {durable_head_unchanged:false,lines:[{view:'Untrusted'}]}
      : {view:'Unbound plans',candidates:[{candidate_ref:'c0',plan_id:'c0@4bf5ca407322'}]};
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status:'ok',result})+'\\n');
  });`;
  const {client}=await fixture(t,5000,{server:scripted});
  await assert.rejects(client.research(), /report identity/);
  await assert.rejects(client.plans(), /report identity/);
  await assert.rejects(client.preview(['end']), /non-publishing preview/);
});

test('plans require a complete source-owned plan ID matching the wire candidate', async t => {
  const scripted = `
  const ids = [null, 'c1@4bf5ca407322', 'c0', 'c0@ABCDEF012345'];
  require('node:readline').createInterface({input:process.stdin}).on('line', line => {
    const r=JSON.parse(line);
    if (r.command!=='combat_plans') throw new Error('unexpected operation');
    const result={report_id:'test-report',view:'已保存方案',effort:'whole_combat',
      candidates:[{candidate_ref:'c0',plan_id:ids.shift()}]};
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status:'ok',result})+'\\n');
  });`;
  const {client}=await fixture(t,5000,{server:scripted});
  for (let i=0; i<4; i++) await assert.rejects(client.plans(), /matching report-bound plan ID/);
});

test('combat outcomes support local filtering and one source-bound review of selected plans', async t => {
  const combatServer = `
  const lines = require('node:readline').createInterface({input:process.stdin});
  lines.on('line', line => {
    const r=JSON.parse(line);
    let result={view:'Reviewed\\n',received:r};
    if (r.command==='combat_research') result={report_id:'test-report',view:'Combat plans\\n',candidates:[
      {candidate_ref:'c0',plan_id:'c0@4bf5ca407322',plan_steps:12,minimum_observed_hp:12,next_incense_turn:2,meets_requested_satisfaction:false,outcome:{final_hp:60,
        total_potions_consumed:0,consumed_entry_potion_slots:[],pending_rewards:[{kind:'gold',amount:20}]}},
      {candidate_ref:'c3',plan_id:'c3@4bf5ca407322',plan_steps:9,meets_requested_satisfaction:true,outcome:{final_hp:72,
        total_potions_consumed:1,consumed_entry_potion_slots:[1],generated_potions:[{slot:0,potion_id:'FirePotion'}]}},
    ]};
    if (r.command==='commit_combat_plan') result={reading:{view:'Rewards\\n',control:'strategic',options:[]}};
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status:'ok',result})+'\\n');
  });`;
  const {client,root}=await fixture(t,5000,{server:combatServer});
  const old=await client.research({effort:'expanded'});
  const plans=await client.research({effort:'expanded'});
  const [preserve,spend]=plans.candidates;
  const before=(await readdir(join(root,'wire'))).length;
  assert.deepEqual(plans.candidates.filter(c=>c.outcome.total_potions_consumed===0),[preserve]);
  assert.strictEqual(spend.outcome,plans.raw.candidates[1].outcome);
  assert.equal(spend.actionCount,9);
  assert.equal(preserve.minimumObservedHp,12);
  assert.equal(preserve.nextIncenseTurn,2);
  assert.equal(spend.minimumObservedHp,undefined);
  assert.equal(spend.nextIncenseTurn,undefined);
  assert.equal(preserve.meetsRequestedSatisfaction,false);
  assert.equal(spend.meetsRequestedSatisfaction,true);
  assert.throws(()=>{spend.outcome.consumed_entry_potion_slots.push(0);},TypeError);
  assert.throws(()=>{preserve.outcome.pending_rewards[0].amount=99;},TypeError);
  await assert.rejects(plans.review([]),/1..8/);
  await assert.rejects(plans.review(Array(9).fill(preserve)),/1..8/);
  await assert.rejects(plans.review(Array(1)),/another reading/);
  await assert.rejects(plans.review([preserve,preserve]),/repeats/);
  await assert.rejects(plans.review([old.candidates[0]]),/another reading/);
  await assert.rejects(plans.review([{...preserve}]),/another reading/);
  await assert.rejects(old.review([old.candidates[0]]),/expired/);
  assert.equal((await readdir(join(root,'wire'))).length,before,'filtering and invalid selections send nothing');
  const compared=await plans.review([spend,preserve],{turn:2,includeSteps:true});
  assert.equal((await readdir(join(root,'wire'))).length,before+1);
  assert.deepEqual(compared.raw.received.candidate_refs,['c3','c0']);
  assert.equal(compared.raw.received.effort,'expanded');
  assert.equal(compared.raw.received.turn,2);
  assert.equal(compared.raw.received.include_steps,true);
  const singleReview=preserve.review;
  assert.deepEqual((await singleReview()).raw.received.candidate_refs,['c0']);
  await spend.commit();
  await assert.rejects(plans.review([preserve]),/expired/);
  await assert.rejects(singleReview(),/expired/);
});

test('reward collection sends one ordered source-bound request and stops at the returned card choice', async t => {
  const rewardServer = `
  const lines = require('node:readline').createInterface({input:process.stdin});
  let revision=0;
  lines.on('line', line => {
    const r=JSON.parse(line);
    const status=r.command==='choose' && r.selection.source.revision!==revision?'error':'ok';
    if (r.command==='choose' && status==='ok') revision++;
    const result={received:r,reading:{view:revision?'Choose a card\\n':'Rewards\\n',control:'strategic',
      source:{revision},context:{deck:'retained'},can_collect_rewards:revision===0,selections:[],
      options:revision?[{label:'Take Headbutt',choice:{ordinal_path:[0]}}]:[
        {label:'Gold',choice:{ordinal_path:[2]}},{label:'Potion',choice:{ordinal_path:[7]}},{label:'Cards',choice:{ordinal_path:[9]}}]}};
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status,result,error:{message:'Decision changed'}})+'\\n');
  });`;
  const {client,root}=await fixture(t,5000,{server:rewardServer});
  const reading=await client.decision();
  const other=await client.decision();
  const count=(await readdir(join(root,'wire'))).length;
  await assert.rejects(reading.collectRewards([]), /1..8/);
  await assert.rejects(reading.collectRewards([reading.options[0],reading.options[0]]), /repeats/);
  await assert.rejects(reading.collectRewards([other.options[0],reading.options[2]]), /another reading/);
  assert.equal((await readdir(join(root,'wire'))).length,count);
  const cards=await reading.collectRewards([reading.options[1],reading.options[0],reading.options[2]],{retainContext:true});
  assert.equal((await readdir(join(root,'wire'))).length,count+1);
  assert.deepEqual(cards.raw.received.selection,{source:{revision:0},choice:{ordinal_path:[7]}});
  assert.deepEqual(cards.raw.received.following_rewards,[{ordinal_path:[2]},{ordinal_path:[9]}]);
  assert.deepEqual(cards.raw.received.known_context,{deck:'retained'});
  assert.equal(cards.collectRewards,undefined);
  assert.equal(cards.options[0].label,'Take Headbutt');
  await assert.rejects(reading.collectRewards(reading.options), /Decision changed/);
  assert.equal((await readdir(join(root,'wire'))).length,count+2);
  assert.equal((await client.decision()).raw.reading.source.revision,1);
});

test('terminal escapes are rejected and preserved, never silently filtered', async t => {
  const {client, root} = await fixture(t);
  await assert.rejects(client.decision({details:['state']}), /JSON/);
  const wire = await readFile(join(root, 'wire', 'exchange-000.jsonl'), 'utf8');
  assert.ok(wire.includes('\x1b]0;bad\x07'));
});

test('selection objects preserve occurrence order, reject mixed frames and invalidate plans', async t => {
  const {client, root} = await fixture(t);
  const first = await client.decision();
  const current = await client.decision();
  const selection = current.selections[0];
  const plans = await client.research();
  await assert.rejects(selection.select([first.selections[0].members[0]]), /another reading/);
  await assert.rejects(selection.select([selection.members[0],selection.members[0]]), /duplicate/);
  assert.equal((await readdir(join(root,'wire'))).length, 3, 'invalid objects never reach the wire');
  const result = await selection.select([selection.members[1],selection.members[0]]);
  assert.deepEqual(result.raw.choice.ordinal_path, [7,4,1]);
  assert.deepEqual(result.raw.source, {revision:0});
  await assert.rejects(plans.candidates[0].commit(), /expired/);
  await assert.rejects(selection.select([]), /Decision changed/);
  const empty = await result.selections[0].select([]);
  assert.deepEqual(empty.raw.choice.ordinal_path, [7]);
});

test('timeout ends the transport without resending an uncertain operation', async t => {
  const {client, root} = await fixture(t, 500);
  await assert.rejects(client.decision({details:['map']}), error => {
    assert.equal(error.code,'RUN_PLAY_TIMEOUT');
    assert.equal(error.outcomeUncertain,true);
    assert.equal(error.transportFailed,true);
    assert.equal(error.command,'decision');
    assert.equal(error.requestId,'exchange-000');
    return /outcome uncertain/.test(error.message);
  });
  await assert.rejects(client.decision(), /outcome uncertain/);
  await client.close();
  assert.deepEqual(await readdir(join(root, 'wire')), ['exchange-000.jsonl', 'stderr.txt']);
  const wire = await readFile(join(root, 'wire', 'exchange-000.jsonl'), 'utf8');
  assert.equal(wire.trim().split('\n').length, 1);
});

test('research allows its requested compute time plus normal response grace', async t => {
  const delayedResearch = `
    const lines = require('node:readline').createInterface({input:process.stdin});
    lines.on('line', line => {
      const r = JSON.parse(line);
      setTimeout(() => process.stdout.write(JSON.stringify({
        schema:'sts-run-play-agent-response-v1', request_id:r.request_id, status:'ok',
        result:{report_id:'delayed-report', view:'Computed and replayed plans\\n', candidates:[]}
      })+'\\n'), 800);
    });`;
  const { client } = await fixture(t, 500, { server: delayedResearch });
  const plans = await client.research({ maxSearchMs: 1000, maxGenerationWork: 100000 });
  assert.equal(plans.view, 'Computed and replayed plans\n');
});

test('draft objects bind sparse options, undo invalidates restored frames and only wins commit', async t => {
  const draftServer = `
  const lines = require('node:readline').createInterface({input:process.stdin});
  let started = false, depth = 0, won = false;
  lines.on('line', line => {
    const r = JSON.parse(line);
    let result;
    if (r.command === 'combat_draft_inspect') result = {view:'Checked '+r.option_ref+'\\n'};
    else if (r.command === 'combat_research') result = { report_id:'test-report',view:'Search\\n', candidates:[{candidate_ref:'c0',plan_id:'c0@4bf5ca407322'}]};
    else if (r.command === 'commit_combat_plan') result = {reading:{view:'Rewards\\n',control:'strategic',options:[]},effort:r.effort};
    else {
      if (r.command === 'combat_draft_advance') started = true;
      if (r.command === 'combat_draft_choose') { depth++; won = r.option_ref === 'T9'; }
      if (r.command === 'combat_draft_undo') { depth = 0; won = false; }
      result = {view:(won?'Victory':'Draft')+'\\n', draft_present:started,
        ...(started?{root_head_revision:0}:{}),
        ...(!won && started?{frame_ref:'F'+depth}:{}),
        ...(won?{commit_candidate_ref:'c0'}:{}),
        options:started&&!won?[{option_ref:'T3',label:'T3: Defend'},{option_ref:'T9',label:'T9: Strike'}].map(o=>({...o,
          action_count:2,successor:{hp:o.option_ref==='T9'?70:60,max_hp:75,block:0,
            monsters:[{slot:2,hp:0,max_hp:30,block:0,alive:false,escaped:false,half_dead:false}],
            potions:[null,'FirePotion']}})):[]};
    }
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status:'ok',result})+'\\n');
  });`;
  const {client, root} = await fixture(t, 5000, {server:draftServer});
  const empty = await client.draft();
  assert.equal(empty.commit, undefined);
  await assert.rejects(empty.advance({generationWork:-1}), /budget/);
  const parent = await empty.advance({generationWork:64,engineSteps:1024});
  assert.equal(parent.commit, undefined);
  const count = (await readdir(join(root,'wire'))).length;
  const kept = parent.options.filter(o => o.successor.hp >= 70 && o.successor.potions[1] === 'FirePotion');
  assert.equal(kept.length, 1);
  assert.equal(kept[0].ref, 'T9');
  assert.equal(kept[0].actionCount, 2);
  assert.equal(kept[0].successor.monsters[0].slot, 2);
  assert.throws(() => { kept[0].successor.potions[1] = null; }, TypeError);
  assert.throws(() => { kept[0].successor.monsters[0].hp = 30; }, TypeError);
  assert.throws(() => { parent.raw.options[1].successor.potions[1] = null; }, TypeError);
  assert.equal(kept[0].successor.potions[1], 'FirePotion');
  assert.equal((await readdir(join(root,'wire'))).length, count);
  assert.equal((await parent.options[1].review()).view, 'Checked T9\n');
  const child = await parent.options[0].choose();
  await assert.rejects(parent.options[0].choose(), /expired/);
  const restored = await child.undo();
  assert.equal(restored.raw.frame_ref, parent.raw.frame_ref);
  await assert.rejects(parent.options[1].review(), /expired/);
  const won = await restored.options[1].choose();
  assert.equal(won.advance, undefined);
  assert.equal(typeof won.commit, 'function');
  const plans = await client.research();
  await assert.rejects(won.commit(), /expired/);
  const terminal = await client.draft();
  const restoredAgain = await terminal.undo();
  await assert.rejects(plans.candidates[0].commit(), /expired/);
  const wonAgain = await restoredAgain.options[1].choose();
  const refreshed = await client.draft();
  assert.equal((await refreshed.commit()).view, 'Rewards\n');
  await assert.rejects(wonAgain.commit(), /expired/);
  const wireFiles = (await readdir(join(root,'wire'))).filter(f => f.endsWith('.jsonl')).sort();
  const requests = await Promise.all(wireFiles.map(async f => JSON.parse((await readFile(join(root,'wire',f),'utf8')).split('\n')[0])));
  assert.deepEqual(requests.filter(r=>r.command==='combat_draft_choose').map(r=>[r.frame_ref,r.option_ref]), [['F0','T3'],['F0','T9'],['F0','T9']]);
  assert.equal(requests.at(-1).effort, 'standard');
  assert.equal(requests.filter(r=>r.command==='combat_draft_advance').length, 1);
});

test('an existing connection is recoverable and duplicate opens do not spawn', async t => {
  const {client, root, config} = await fixture(t);
  const pending = client.decision();
  await assert.rejects(client.close(), /current operation/);
  assert.equal(await getRunPlay(root), client);
  await pending;
  const spelling = process.platform === 'win32' ? root.toUpperCase() : root;
  assert.equal(await getRunPlay(spelling), client);
  const duplicateRecords = join(root, 'duplicate');
  await assert.rejects(openRunPlay({...config, session: spelling, recordDirectory: duplicateRecords}), /already has an open/);
  assert.ok(!(await readdir(root)).includes('duplicate'));
  const reloaded = await import('./client.mjs?registry-test');
  assert.equal(await reloaded.getRunPlay(root), client);
  await reloaded.getRunPlay(root).then(c => c.close());
  await assert.rejects(getRunPlay(root), /No open/);
  const reopened = await openRunPlay({...config, recordDirectory: join(root,'reopened')});
  try {
    assert.notEqual(reopened, client);
    await assert.rejects(client.decision(), /closing/);
    await reopened.decision();
  } finally { await reopened.close(); }
});

test('candidate draft opens preserve source and effort; rejected replacement leaves handles usable', async t => {
  const draftServer = `
  const lines = require('node:readline').createInterface({input:process.stdin});
  lines.on('line', line => {
    const r=JSON.parse(line);
    let status='ok', result={view:'Draft turn 1\\n',frame_ref:'F1',root_head_revision:0,options:[{option_ref:'T0',label:'Strike'}]};
    if (r.command==='combat_research') result={report_id:'test-report',view:'Plans\\n',candidates:[{candidate_ref:'c3',plan_id:'c3@4bf5ca407322'}]};
    if (r.command==='combat_draft_open') {
      if (!r.replace) status='error';
      else result={view:'Draft turn '+r.turn+'\\n',frame_ref:'F2',root_head_revision:0,options:[],received:r};
    }
    if (r.command==='combat_draft_inspect') result={view:'Checked turn 1\\n'};
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status,result,error:{message:'Draft exists'}})+'\\n');
  });`;
  const {client,root}=await fixture(t,5000,{server:draftServer});
  const plans=await client.research({effort:'expanded'});
  const old=await client.draft();
  const candidate=plans.candidates[0];
  const count=(await readdir(join(root,'wire'))).length;
  for (const options of [{}, {turn:0}, {turn:1.1}, {turn:'2'}, {turn:2,replace:'yes'}]) {
    await assert.rejects(candidate.draft(options),/observed turn/);
  }
  assert.equal((await readdir(join(root,'wire'))).length,count);
  await assert.rejects(candidate.draft({turn:2}),/Draft exists/);
  assert.equal((await old.options[0].review()).view,'Checked turn 1\n');
  const opened=await candidate.draft({turn:2,replace:true});
  assert.deepEqual([opened.raw.received.candidate_ref,opened.raw.received.effort,opened.raw.received.turn],['c3','expanded',2]);
  assert.equal(opened.commit,undefined);
  assert.equal(typeof opened.advance,'function');
  await assert.rejects(old.options[0].review(),/expired/);
  await assert.rejects(candidate.draft({turn:1,replace:true}),/expired/);
});

test('failed startup releases the reservation and retains stderr', async t => {
  const {client, root, config} = await fixture(t);
  await client.close();
  await assert.rejects(openRunPlay({...config, recordDirectory: join(root,'missing-parent','wire')}));
  await assert.rejects(getRunPlay(root), /No open/);
  const failed = await openRunPlay({...config, binary: join(root,'missing-binary'), recordDirectory: join(root,'failed')});
  await assert.rejects(failed.decision());
  await failed.close();
  await assert.rejects(getRunPlay(root), /No open/);
  assert.equal(await readFile(join(root,'failed','stderr.txt'),'utf8'), '');
});

test('concurrent opens reserve a session before creating a transport', async t => {
  const {client, root, config} = await fixture(t);
  await client.close();
  const attempts = await Promise.allSettled(['first','second'].map(name => openRunPlay({...config, recordDirectory: join(root,name)})));
  const opened = attempts.filter(r => r.status === 'fulfilled');
  assert.equal(opened.length, 1);
  try {
    assert.equal(attempts.filter(r => r.status === 'rejected').length, 1);
    assert.equal((await readdir(root)).filter(n => ['first','second'].includes(n)).length, 1);
    assert.equal(await getRunPlay(root), opened[0].value);
  } finally { await opened[0].value.close(); }
});

test('EOF shutdown is bounded, records stderr and releases the connection', async t => {
  const {client, root} = await fixture(t, 5000, {
    server: server + `\nprocess.stderr.write('shutdown fixture'); setInterval(() => {}, 1000);`,
    expectedCloseError: true,
  });
  await client.decision();
  await assert.rejects(client.close(), /did not exit after EOF/);
  assert.equal(await readFile(join(root,'wire','stderr.txt'),'utf8'), 'shutdown fixture');
  await assert.rejects(getRunPlay(root), /No open/);
  await assert.rejects(client.decision(), /closed/);
});

test('nested draft selections bind sources and ordered members through edits, paging and undo', async t => {
  const selectionServer = `
  const lines = require('node:readline').createInterface({input:process.stdin});
  let preferences = [], depth = 0;
  lines.on('line', line => {
    const r = JSON.parse(line);
    let result;
    if (r.command === 'combat_draft_selection') {
      const source = r.edit?.source_ref ?? r.source_ref;
      if (r.edit) preferences = r.edit.preferences;
      result = {view:source?'Discard from hand\\n':'Draft selections\\n', frame_ref:'F'+depth,
        sources:source?[]:[{source_ref:r.offset===8?'S12':'S4',label:'Use Brew',action_count:1}],
        families:source?[{family:2,label:'Discard',min:0,max:2,members:[
          {ordinal:1,label:'Strike'},{ordinal:4,label:'Strike'},{ordinal:6,label:'Defend'}]}]:[],
        preferences,
        outcomes:source?[{option_ref:'T7',label:'T7: Strike (1)',boundary:'next_player_turn',preferred:preferences.length>0,equivalence:'gambling_chip_cancel',
          action_count:3,successor:{hp:61,max_hp:75,block:0,monsters:[],potions:[null]}}]:[],
        ...(source?{source_ref:source}:{}), ...(r.offset?{}:{next_offset:8}), received:r};
    } else if (r.command === 'combat_draft_inspect') result = {view:'Checked\\n', received:r};
    else {
      if (r.command === 'combat_draft_choose') depth++;
      if (r.command === 'combat_draft_undo') depth=0;
      result = {view:'Draft\\n', frame_ref:'F'+depth, root_head_revision:0, options:[], received:r};
    }
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status:'ok',result})+'\\n');
  });`;
  const {client, root} = await fixture(t, 5000, {server:selectionServer});
  const draft = await client.draft();
  const index = await draft.selections();
  const page = await index.next();
  assert.equal(page.raw.received.offset, 8);
  assert.equal(page.raw.received.source_ref, undefined);
  let focus = await page.sources[0].open();
  assert.equal(focus.raw.received.source_ref, 'S12');
  const other = await index.sources[0].open();
  const family = focus.families[0];
  const count = (await readdir(join(root,'wire'))).length;
  await assert.rejects(family.prefer([other.families[0].members[0]]), /another reading/);
  await assert.rejects(family.prefer([family.members[0],family.members[0]]), /duplicate/);
  await assert.rejects(family.prefer(family.members), /bounds/);
  await assert.rejects(focus.advance({engineSteps:-1}), /budget/);
  assert.equal((await readdir(join(root,'wire'))).length, count);
  focus = await family.prefer([family.members[1],family.members[0]]);
  assert.deepEqual(focus.raw.received.edit, {source_ref:'S12',preferences:[{family:2,choices:[4,1]}]});
  await assert.rejects(family.prefer([]), /expired/);
  await assert.rejects(index.sources[0].open(), /expired/);
  await assert.rejects(draft.advance(), /expired/);
  focus = await focus.families[0].prefer([]);
  assert.deepEqual(focus.raw.received.edit.preferences, [{family:2,choices:[]}]);
  focus = await focus.clearPreferences();
  assert.deepEqual(focus.raw.received.edit.preferences, []);
  const paged = await focus.next();
  assert.equal(paged.raw.received.source_ref, 'S12');
  assert.equal(paged.raw.received.offset, 8);
  const old = focus;
  focus = await paged.advance({generationWork:1,engineSteps:500});
  assert.equal(focus.raw.received.source_ref, 'S12');
  assert.equal(focus.raw.received.offset, 0);
  await assert.rejects(old.outcomes[0].choose(), /expired/);
  assert.equal(focus.outcomes[0].equivalence, 'gambling_chip_cancel');
  assert.equal(focus.outcomes[0].successor.hp, 61);
  assert.deepEqual(focus.outcomes[0].successor.potions, [null]);
  assert.equal(focus.outcomes[0].actionCount, 3);
  assert.deepEqual((await focus.outcomes[0].review()).raw.received.frame_ref, 'F0');
  const next = await focus.outcomes[0].choose();
  assert.equal(next.raw.received.option_ref, 'T7');
  const restored = await next.undo();
  assert.equal(restored.raw.frame_ref, 'F0');
  await assert.rejects(focus.clearPreferences(), /expired/);
  await restored.selections();
  const files = (await readdir(join(root,'wire'))).filter(f=>f.endsWith('.jsonl')).sort();
  const requests = await Promise.all(files.map(async f=>JSON.parse((await readFile(join(root,'wire',f),'utf8')).split('\n')[0])));
  assert.deepEqual(requests.filter(r=>r.command==='combat_draft_advance').map(r=>[r.generation_work,r.engine_steps]), [[1,500]]);
  assert.equal(requests.filter(r=>r.command==='combat_draft_choose').length, 1);
  assert.equal(requests.filter(r=>['choose','commit_combat_plan'].includes(r.command)).length, 0);
});

test('act sends one transactional semantic line and validates input locally', async t => {
  const actServer = `
  const lines = require('node:readline').createInterface({input:process.stdin});
  let lastAct = null;
  lines.on('line', line => {
    const r=JSON.parse(line);
    let status='ok', result={};
    if (r.command==='act') {
      lastAct = r.actions.join(',')+(r.advance?'@'+r.advance:'');
      result={kind:'acted', observation:{schema:'x'}, reading:{view:'After '+lastAct+'\\n', control:'strategic', options:[]}};
    } else {
      result={reading:{view:(lastAct?'After '+lastAct:'Idle')+'\\n',
        control:'strategic', source:{revision:0}, context:{}, options:[]}};
    }
    process.stdout.write(JSON.stringify({schema:'sts-run-play-agent-response-v1',request_id:r.request_id,status,result,error:{message:'no'}})+'\\n');
  });`;
  const {client, root} = await fixture(t, 5000, {server: actServer});
  const before = (await readdir(join(root, 'wire'))).length;
  for (const bad of [[], ['play:Strike', ''], [42], 'play:Strike']) {
    await assert.rejects(client.act(bad), /non-empty semantic action/);
  }
  await assert.rejects(client.act(['end'], {advance: 3}), /advance must be a string/);
  assert.equal((await readdir(join(root, 'wire'))).length, before);
  const next = await client.act(['play:Strike+0#0@0', 'end']);
  assert.equal(next.view, 'After play:Strike+0#0@0,end@one_public_action\n');
  const advanced = await client.act(['take:Anger'], {advance: 'next_strategic_decision'});
  assert.equal(advanced.view, 'After take:Anger@next_strategic_decision\n');
  assert.equal((await readdir(join(root, 'wire'))).length, before + 2, 'each act returns its successor in one exchange');
});
