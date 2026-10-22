import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, realpath, rm, readFile } from 'node:fs/promises';
import { readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, sep, basename } from 'node:path';
import { openWorkbench, navigationArguments, createWorkbenchContext, executeWorkbenchCommands } from './workbench.mjs';
import { commandTokens, commandGroups } from './session.mjs';
import { info } from './commands/travel.mjs';
import { runEngine } from './commands/navigation.mjs';

test('strategic reads preserve source-rendered localized choices without querying another observation', async () => {
  const view = '选择牌\n./p take:Armaments+0#0  |  效果：武装\n  Input: ./p act select:<tokens>\n0: claim reward[4] is descriptive text\n';
  let output = '';
  const ctx = createWorkbenchContext({ write(text) { output += text; },
    game: new Proxy({}, { get() { assert.fail('source-rendered strategic text needs no extra query'); } }),
  });
  await ctx.printCurrentReading({ control: 'strategic', view, options: [{ label: '武装' }] });
  assert.equal(output, view);
});

test('printing a reading does not query plans based on an English or localized heading', async () => {
  for (const heading of ['Saved combat wins', '已保存的战斗方案']) {
    let text = '';
    const view = `当前局面\n${heading}\nc0@4bf5ca407322 | review c0@4bf5ca407322 4\n`;
    const ctx = createWorkbenchContext({ write(chunk) { text += chunk; },
      game: new Proxy({}, { get() { assert.fail('rendering must not request another reading'); } }),
    });
    await ctx.printCurrentReading({ view });
    assert.equal(text, view);
  }
});

test('execution records only the final displayed actual reading and keeps previews separate', async t => {
  let revision = 0;
  const current = () => ({ control: 'combat', summary: `当前局面 ${revision}\n`,
    view: `当前局面 ${revision}\n手牌：防御\n./p act end\n`,
    raw: { observation: { position: { kind: 'decision', public_snapshot_id: `s${revision}`,
      actions: [], observation: { domain: 'combat', observation: {} } } } },
  });
  const wb = await fixture(t, async () => ({
    async decision() { return current(); },
    async act() { revision++; return current(); },
    async preview() { return { view: '假设后继\nTerminal | Failure\n' }; },
    async observe() { assert.fail('reading metadata must not add a query'); },
    async close() {},
  }), undefined, { commandPrefix: '' });
  const read = await wb.execute('read');
  assert.deepEqual(read.currentReading, { control: 'combat', summary: '当前局面 0\n',
    text: '当前局面 0\n手牌：防御\nact end\n' });
  assert.ok(read.text.includes(read.currentReading.text));
  const preview = await wb.execute('preview end');
  assert.equal(preview.currentReading, undefined, 'earlier calls cannot supply metadata for a preview');
  const mixed = await wb.execute('read, preview end');
  assert.deepEqual(mixed.currentReading, read.currentReading);
  assert.ok(mixed.text.includes('假设后继'));
  assert.ok(!mixed.currentReading.text.includes('假设'));
  const acted = await wb.execute('act end, act end');
  assert.equal(acted.currentReading.summary, '当前局面 2\n');
  assert.ok(acted.text.includes(acted.currentReading.text));
  assert.doesNotMatch(acted.text, /当前局面 1/);
});

test('board reads one typed observation for both commands and its numeric summary', async () => {
  let observations = 0, text = '';
  const ctx = createWorkbenchContext({ write(chunk) { text += chunk; }, game: {
    async decision() { return { control: 'combat', view: '战斗\n当前格挡 8\n' }; },
    async observe() {
      observations++;
      return { position: { kind: 'decision', public_snapshot_id: 'board', actions: [],
        observation: { domain: 'combat', observation: {
          player: { block: 8 }, cards: { hand: { cards: [] } },
          monsters: [{ alive: true, intent: { preview_damage_per_hit: 13 } }],
        } },
      } };
    },
  } });
  const { dispatchWorkbench } = await import('./workbench.mjs');
  await dispatchWorkbench(ctx, 'board', []);
  assert.equal(observations, 1);
  assert.match(text, /当前格挡 8/);
  assert.match(text, /incoming next enemy turn: 13 \| current block 8/);
});

for (const prefix of [false, true]) test(`cancelling a running diagnosis ${prefix ? 'after an applied action' : 'without a prefix'} waits for its child and preserves serialization`, { timeout: 8000 }, async t => {
  let plays = 0, observed = false, launches = 0;
  const controller = new AbortController();
  t.after(() => controller.abort());
  const wb = await fixture(t, async () => ({
    async decision() { return { view: 'HP 70/75\n' }; },
    async act() { plays++; return { view: 'HP 70/75\n' }; },
    async observe() { observed = true; return { hp: 70 }; },
    async close() {},
  }), (_binary, args, options) => {
    launches++;
    assert.equal(args[0], 'deck-lab');
    assert.equal(options.signal, controller.signal);
    return runEngine(process.execPath, ['-e',
      `require('node:fs').writeFileSync(${JSON.stringify(join(wb.recordDirectory, 'child-ready'))}, String(process.pid)); setInterval(() => {}, 1000);`], options);
  });
  const execution = wb.execute(`${prefix ? 'act end, ' : ''}deck-lab TheHeart, act end`, { signal: controller.signal });
  const rejected = assert.rejects(execution, error => {
    assert.match(error.message, /abort/i);
    if (prefix) assert.match(error.message, /Applied before error:.*end.*HP 70\/75/s);
    return true;
  });
  // A real child announces readiness. Reading it must remain possible while
  // the diagnosis is active; a synchronous subprocess would block this loop.
  let pid;
  const deadline = Date.now() + 3000;
  while (!pid && Date.now() < deadline) {
    try { pid = Number(await readFile(join(wb.recordDirectory, 'child-ready'), 'utf8')); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (!pid) await new Promise(resolve => setTimeout(resolve, 10));
  }
  assert.ok(pid, 'the diagnostic child must announce readiness');
  const observation = wb.observe();
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(observed, false, 'game reads wait for the active operation');
  controller.abort();
  await rejected;
  assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
  assert.deepEqual(await observation, { hp: 70 });
  assert.equal(launches, 1);
  assert.equal(plays, Number(prefix), 'neither the applied action nor a command after cancellation is replayed');
  assert.equal((await wb.execute('act end')).text, 'HP 70/75\n');
  assert.equal(plays, Number(prefix) + 1);
});

async function fixture(t, connect, run = () => assert.fail('unexpected executable'), options = {}) {
  const base = await realpath(tmpdir());
  const root = await mkdtemp(join(base, 'run-play-workbench-'));
  const workbench = await openWorkbench({ repository: root, session: root,
    recordDirectory: join(root, 'records'), commandPrefix: '', ...options }, { connect, run });
  t.after(async () => {
    try { await workbench.close(); }
    finally {
      const target = await realpath(root);
      assert.ok(target.startsWith(base + sep) && basename(target).startsWith('run-play-workbench-'));
      await rm(target, { recursive: true });
    }
  });
  return workbench;
}

test('manual workbench blocks assistance before grouped actions and retains the mode across returns', async t => {
  let plays = 0, connections = 0;
  const wb = await fixture(t, async options => {
    assert.equal(options.manual, true); connections++;
    return { async decision() { return { view: 'Turn 1 | HP 70/75\n' }; },
      async act() { plays++; return { view: 'Turn 2 | HP 65/75\n' }; }, async close() {} };
  }, () => ({ status: 0, stdout: 'restored' }), { manual: true });
  for (const command of ['preview end', 'research', 'plans', 'review c0', 'commit c0', 'deck-lab list', 'piles sl']) {
    await assert.rejects(wb.execute(`act end; ${command}`), /Public manual play disables/);
  }
  await assert.rejects(wb.preview(['end']), /Public manual play disables/);
  assert.equal(plays, 0);
  await wb.execute('act end');
  const returned = await wb.execute('back 1');
  assert.equal(returned.branchChanged, true);
  assert.equal(connections, 2);
  assert.equal(plays, 1);
  assert.equal(JSON.parse(await readFile(join(wb.recordDirectory, 'workbench.json'), 'utf8')).manual, true);
  const events = (await readFile(join(wb.recordDirectory, 'commands.jsonl'), 'utf8')).trim().split('\n').map(JSON.parse);
  assert.equal(events.filter(event => event.phase === 'started').length, 10);
  assert.equal(events.filter(event => event.phase === 'failed').length, 8);
  const last = events.at(-1);
  assert.equal(last.phase, 'completed');
  assert.equal(last.result.branchChanged, true);
  assert.match(last.result.text, /Turn 1/);
  assert.ok(last.elapsedMs >= 0);
});

test('agent text language belongs to the workbench and survives reconnects without changing a query', async t => {
  const queries = [];
  const wb = await fixture(t, async () => ({
    async info(query, options) { queries.push({ query, ...options }); return { view: '药水机制原文\n' }; },
    async decision() { return { view: 'Reopened game\n', control: 'terminal' }; },
    async close() {},
  }), () => ({ status: 0, stdout: 'restored' }), { agentLocale: 'zh-CN' });
  assert.equal((await wb.execute('info potion SteroidPotion')).text, '药水机制原文\n');
  await wb.execute('back 1');
  await wb.execute('info potion SteroidPotion');
  assert.deepEqual(queries, [
    { query: 'potion SteroidPotion', locale: 'zh-CN' },
    { query: 'potion SteroidPotion', locale: 'zh-CN' },
  ]);
  const metadata = JSON.parse(await readFile(join(wb.recordDirectory, 'workbench.json'), 'utf8'));
  assert.equal(metadata.agentLocale, 'zh-CN');
});

test('text tokens preserve card selectors and literal shell syntax without evaluation', () => {
  assert.deepEqual(commandTokens('info "card Dropkick" , act play:Strike#0@1*2 end'),
    ['info', 'card Dropkick', ',', 'act', 'play:Strike#0@1*2', 'end']);
  assert.deepEqual(commandTokens('info "$(not a command)"'), ['info', '$(not a command)']);
  assert.deepEqual(commandTokens('act select:Strike#0,Defend#0'), ['act', 'select:Strike#0,Defend#0']);
  assert.deepEqual(commandTokens('claim:Gold#0, claim:SwiftPotion#0, open-card#0'),
    ['claim:Gold#0', ',', 'claim:SwiftPotion#0', ',', 'open-card#0']);
  assert.deepEqual(commandTokens('go 5,10, info "literal, text;"; read'),
    ['go', '5,10', ',', 'info', 'literal, text;', ';', 'read']);
  assert.deepEqual(commandTokens("info card Ascender's Bane"), ['info', 'card', "Ascender's", 'Bane']);
  assert.deepEqual(commandTokens('act play:"Twin Strike"@0'), ['act', 'play:Twin Strike@0']);
  assert.throws(() => commandTokens('info "unfinished'), /unclosed quote/);
});

test('comma action continuations retain their explicit execution or preview scope', () => {
  const parse = line => { const [command, ...args] = commandTokens(line); return commandGroups(command, args); };
  assert.deepEqual(parse('preview event:CursedTome:0, event:CursedTome:0, info'), [
    ['preview', ['event:CursedTome:0', 'event:CursedTome:0']], ['info', []],
  ]);
  assert.deepEqual(parse('act play:Anger#0, play:Bash+#0@0, end, inventory'), [
    ['act', ['play:Anger#0', 'play:Bash+#0@0', 'end']], ['inventory', []],
  ]);
  assert.deepEqual(parse('preview play:Inflame#0, play:ShrugItOff#0, act play:Inflame#0, options'), [
    ['preview', ['play:Inflame#0', 'play:ShrugItOff#0']], ['act', ['play:Inflame#0']], ['options', []],
  ]);
  assert.deepEqual(parse('preview event:CursedTome:0, Continue'), [['preview', ['event:CursedTome:0', 'Continue']]]);
  assert.deepEqual(parse('act select Strike#0, Defend#0'), [['act', ['select', 'Strike#0', 'Defend#0']]]);
  assert.deepEqual(parse('preview end; play:Strike#0'), [['preview', ['end']], ['play:Strike#0', []]]);
  assert.deepEqual(parse('claim:Gold#0, open-card#0'), [['claim:Gold#0', []], ['open-card#0', []]]);
  assert.deepEqual(parse('go 5,10, info M0'), [['go', ['5,10']], ['info', ['M0']]]);
  assert.deepEqual(parse('act smith:Bash#0, deck-lab Hexaghost'), [['act', ['smith:Bash#0']], ['deck-lab', ['Hexaghost']]]);
  assert.deepEqual(parse('preview go 1,0, go 2,1, info'), [['preview', ['go', '1,0', 'go', '2,1']], ['info', []]]);
  assert.throws(() => parse('act play:Anger#0, back 1'), /navigation.*separate/);
});

test('typed private previews use the resident connection and share its serialized lifecycle', async t => {
  const calls = [];
  const wb = await fixture(t, async () => ({
    async preview(actions) { calls.push(actions); return { preview: true, endpoint: { kind: 'decision' } }; },
    async observe() { calls.push('observe'); return {}; }, async close() { calls.push('close'); },
  }));
  const [preview] = await Promise.all([wb.preview(['buy:MembershipCard#0']), wb.observe()]);
  assert.equal(preview.preview, true);
  assert.deepEqual(calls, [['buy:MembershipCard#0'], 'observe']);
  const controller = new AbortController(); controller.abort();
  await assert.rejects(wb.preview(['buy:ShrugItOff#0'], { signal: controller.signal }), /abort/i);
  await wb.close();
  await assert.rejects(wb.preview(['leave']), /closed/);
});

test('SL pile inspection requires an explicit information level and stays on the resident connection', async t => {
  let reads = 0;
  const wb = await fixture(t, async () => ({
    async slPiles() { reads++; return {view: 'SL internal combat piles\nDraw (next -> last): Shrug It Off+\n'}; },
    async close() {},
  }));
  for (const command of ['piles public', 'piles sl extra']) {
    await assert.rejects(wb.execute(command), /Use piles for public unordered/);
  }
  assert.equal(reads, 0);
  const result = await wb.execute('piles sl');
  assert.match(result.text, /^SL internal combat piles/);
  assert.equal(reads, 1);
});

test('manual public piles use only public card sets and reveal neither pile order nor private fields', async t => {
  const card = (card_id, upgrades = 0) => ({ card_id, upgrades, private_marker: 'must-not-leak' });
  const pile = cards => ({ evidence: 'public_unordered_collection', cards });
  const cards = { draw: pile([card('Strike'), card('BodySlam', 1), card('Strike')]),
    discard: pile([card('PommelStrike', 1)]), exhaust: pile([card('Impervious'), card('Defend')]) };
  const wb = await fixture(t, async () => ({
    async observe() { return { position: { kind: 'decision', public_snapshot_id: 'public-1', actions: [],
      observation: { domain: 'combat', observation: { cards } } } }; },
    async slPiles() { assert.fail('public inspection must never request exact piles'); }, async close() {},
  }), undefined, { manual: true });
  const first = (await wb.execute('piles')).text;
  cards.draw.cards.reverse(); cards.exhaust.cards.reverse();
  assert.equal((await wb.execute('piles')).text, first);
  assert.match(first, /Draw \[3\]: BodySlam\+, 2x Strike/);
  assert.match(first, /Discard \[1\]: PommelStrike\+/);
  assert.match(first, /Exhaust \[2\]: Defend, Impervious/);
  assert.doesNotMatch(first, /must-not-leak|next ->|shuffle input/);
  cards.draw.evidence = 'hidden';
  await assert.rejects(wb.execute('piles'), /Public draw pile contents are unavailable/);
});

test('literal act/preview notation gives the actual command choices without attempting a game selection', async t => {
  const wb = await fixture(t, async () => ({
    async decision() { assert.fail('malformed command must not attempt to select a game option'); },
    async close() {},
  }));
  await assert.rejects(wb.execute('act/preview play:FeelNoPain+'), /Use preview.*or act.*No action was submitted/);
});

test('an unsupported deck query cannot fall through to selecting an option with deck in its label', async t => {
  const wb = await fixture(t, async () => ({
    async decision() { assert.fail('deck must not resolve or select a game option'); },
    async close() {},
  }));
  await assert.rejects(wb.execute('deck'), /^Error: deck is not a command\. Use inventory/);
});

test('routine readings request brief and full context remains explicit', async t => {
  const calls = [];
  const wb = await fixture(t, async () => ({
    async decision(options) { calls.push(options); return { control: 'strategic', view: 'HP 61/75\n' }; },
    async close() {},
  }));
  for (const command of ['read', 'read brief', 'board', 'read full', 'options', '']) {
    assert.equal((await wb.execute(command)).text, 'HP 61/75\n');
  }
  assert.deepEqual(calls, [{ details: ['brief'] }, { details: ['brief'] }, { details: ['brief'] },
    { details: [] }, { details: ['brief', 'options'] }, { details: ['brief'] }]);
  for (const command of ['read detailed', 'read brief extra', 'read full extra']) {
    await assert.rejects(wb.execute(command), /read \[brief\|full\]/);
  }
  assert.equal(calls.length, 6);
});

test('a bare play after a query requires act and preserves prior query output', async t => {
  const wb = await fixture(t, async () => ({
    async decision() { assert.fail('missing act must not attempt an option-name lookup'); },
    async info() { return { view: 'Current mechanics\n' }; },
    async close() {},
  }));
  await assert.rejects(wb.execute('info card Strike, play:PommelStrike+#0@0'), error => {
    assert.match(error.message, /Current mechanics/);
    assert.match(error.message, /Use act play:PommelStrike\+#0@0/);
    assert.match(error.message, /Start the action line with act or preview/);
    return true;
  });
});

test('inventory and route analysis expose existing Rust owners without changing branches', async t => {
  const calls = [];
  const wb = await fixture(t, async () => ({
    async decision() { assert.fail('native inventory/routes must not substitute a basic reading'); },
    async observe() { assert.fail('route analysis must not fall back to the grid'); },
    async close() {},
  }), (_binary, args) => {
    calls.push(args.filter((_, i) => i !== 2)); // Omit the fixture session path.
    return {status: 0, stdout: `owner: ${args[0]} ${args.slice(3).join(' ')}\n`};
  });
  for (const command of ['inventory', 'inventory effects', 'map', 'map elites', 'map route 2 4']) {
    const result = await wb.execute(command);
    assert.match(result.text, /^owner:/);
    assert.equal(result.branchChanged, undefined);
  }
  assert.deepEqual(calls, [
    ['inventory', '--session', '--locale', 'en'], ['inventory', '--session', '--locale', 'en', '--effects'],
    ['map', '--session'], ['map', '--session', 'elites'], ['map', '--session', 'route', '2', '4'],
  ]);
  for (const command of ['inventory unknown', 'map route', 'map route 2 4 extra', 'map elites extra', 'map unknown']) {
    await assert.rejects(wb.execute(command), /inventory \[effects\]|map \[/);
  }
  assert.equal(calls.length, 5, 'invalid parameters must not execute or silently display the grid');
});

test('inventory forwards the bound agent language to the native renderer', async t => {
  const calls = [];
  const wb = await fixture(t, async () => ({ async close() {} }), (_binary, args) => {
    calls.push(args.filter((arg, index) => index !== 2));
    return { status: 0, stdout: '永久卡组（11）：\n' };
  }, { agentLocale: 'zh-CN' });
  for (const command of ['inventory', 'inventory effects']) {
    const result = await wb.execute(command);
    assert.equal(result.text, '永久卡组（11）：\n');
    assert.equal(result.branchChanged, undefined);
  }
  assert.deepEqual(calls, [
    ['inventory', '--session', '--locale', 'zh-CN'],
    ['inventory', '--session', '--locale', 'zh-CN', '--effects'],
  ]);
});

test('resident commands serialize, isolate outputs and recover after a query failure', async t => {
  let inFlight = 0;
  const wb = await fixture(t, async () => ({
    async info(query) {
      assert.equal(inFlight++, 0);
      await new Promise(resolve => setTimeout(resolve, 5));
      inFlight--;
      if (query === 'missing') throw new Error('exact query missing');
      return { view: `${query}\n` };
    }, async close() { assert.equal(inFlight, 0); },
  }));
  const results = await Promise.allSettled([
    wb.execute('info first'), wb.execute('info missing'), wb.execute('info last'),
  ]);
  assert.deepEqual(results.map(x => x.status), ['fulfilled', 'rejected', 'fulfilled']);
  assert.equal(results[0].value.text, 'first\n');
  assert.equal(results[1].reason.message, 'exact query missing');
  assert.equal(results[2].value.text, 'last\n');
});

test('host observations serialize with game commands and preserve terminal positions', async t => {
  const calls = [];
  let inFlight = 0;
  const position = { kind: 'terminal', outcome: { kind: 'combat_defeat' } };
  const wb = await fixture(t, async () => ({
    async info(query) {
      assert.equal(inFlight++, 0);
      await new Promise(resolve => setTimeout(resolve, 5));
      calls.push(query);
      inFlight--;
      return { view: `${query}\n` };
    },
    async observe() { assert.equal(inFlight, 0); calls.push('observe'); return { position }; },
    async close() { assert.equal(inFlight, 0); },
  }));
  const controller = new AbortController();
  controller.abort();
  await assert.rejects(wb.observe({ signal: controller.signal }), /abort/i);
  const results = await Promise.all([wb.execute('info first'), wb.observe(), wb.execute('info last')]);
  assert.deepEqual(calls, ['first', 'observe', 'last']);
  assert.deepEqual(results[1], { position });
  await wb.close();
  await assert.rejects(wb.observe(), /closed/);
});

test('deck-lab routes current resources to the shared Rust owner with fresh diagnostic artifacts', async t => {
  const calls = [];
  const wb = await fixture(t, async () => ({
    async decision() { assert.fail('deck diagnosis must not advance a game decision'); },
    async close() {},
  }), (_binary, args) => { calls.push(args); return {status: 0, stdout: `diagnostic only\nFull cases: ${args.at(-1)}\n`}; });
  const catalog = await wb.execute('deck-lab list');
  assert.match(catalog.text, /Run: deck-lab ENCOUNTER/);
  assert.match(catalog.text, /Append after.*displayed preparation command.*removal.*privately/);
  assert.match(catalog.text, /game stays unchanged/);
  assert.deepEqual(calls[0].filter((_, i) => i !== 2), ['deck-lab', '--session', '--list']);
  for (let i = 0; i < 2; i++) {
    const result = await wb.execute('deck-lab TheHeart scenarios 3 work 200000 ms 8000 potions 2 full-hp');
    assert.match(result.text, /^diagnostic only\nFull cases:/);
    assert.equal(result.branchChanged, undefined);
  }
  const first = calls[1], second = calls[2];
  assert.deepEqual(first.slice(3, -2), ['--encounter', 'TheHeart', '--scenarios', '3',
    '--work', '200000', '--ms', '8000', '--max-potion-uses', '2', '--full-hp']);
  assert.equal(first.at(-2), '--output');
  assert.ok(first.at(-1).startsWith(join(wb.recordDirectory, 'deck-lab-')));
  assert.notEqual(first.at(-1), second.at(-1));
  await wb.execute('deck-lab TheHeart details after smith:PommelStrike#0');
  assert.deepEqual(calls.at(-1).slice(3, -2),
    ['--encounter', 'TheHeart', '--details', '--after', 'smith:PommelStrike#0']);
  const validCallCount = calls.length;
  for (const invalid of ['deck-lab', 'deck-lab list extra', 'deck-lab --session other',
    'deck-lab TheHeart --output other', 'deck-lab TheHeart work 0', 'deck-lab TheHeart work 4 work 5',
    'deck-lab TheHeart full-hp full-hp', 'deck-lab TheHeart details details',
    'deck-lab TheHeart full_hp', 'deck-lab TheHeart ms -1', 'deck-lab TheHeart potions']) {
    await assert.rejects(wb.execute(invalid), /deck-lab list/);
  }
  assert.equal(calls.length, validCallCount, 'malformed input cannot execute or override the bound session/output');
});

test('deck-lab preparation is passed to Rust as one private semantic line', async t => {
  const calls = [];
  const wb = await fixture(t, async () => ({
    async decision() { assert.fail('candidate diagnosis must not adopt a choice'); },
    async close() {},
  }), (_binary, args) => { calls.push(args); return { status: 0, stdout: 'Preparation (private)\n' }; });
  await wb.execute('deck-lab Collector scenarios 3 after smith:PommelStrike#0');
  assert.deepEqual(calls[0].slice(3, -2), ['--encounter', 'Collector', '--scenarios', '3',
    '--after', 'smith:PommelStrike#0']);
  await wb.execute('deck-lab TheHeart potions 1 after buy:MembershipCard#0 buy:"Pommel Strike"#0');
  assert.deepEqual(calls[1].slice(3, -2), ['--encounter', 'TheHeart', '--max-potion-uses', '1',
    '--after', 'buy:MembershipCard#0/buy:Pommel Strike#0']);
  for (const command of ['deck-lab TheHeart after', 'deck-lab TheHeart after --session other',
    'deck-lab TheHeart after rest after rest', 'deck-lab list after rest']) {
    await assert.rejects(wb.execute(command), /deck-lab list/);
  }
  assert.equal(calls.length, 2);
});

test('a later failure preserves committed receipts and never repeats the earlier action', async t => {
  let plays = 0, hp = 75;
  const wb = await fixture(t, async () => ({
    async decision() { return { view: `HP ${hp}/75\n` }; },
    async act(tokens) { assert.deepEqual(tokens, ['end']); plays++; hp = 70; return { view: 'HP 70/75\n' }; },
    async info() { throw new Error('missing mechanics'); }, async close() {},
  }));
  await assert.rejects(wb.execute('act end, info missing'), error => {
    assert.match(error.message, /Applied before error:.*end.*HP 70\/75/s);
    assert.match(error.message, /^missing mechanics\nApplied before error:/);
    return true;
  });
  assert.equal(plays, 1);
  assert.equal((await wb.execute('read')).text, 'HP 70/75\n');
});

test('a successful batch replaces mutation pages while preserving requested queries', async () => {
  let hp = 75;
  const ctx = createWorkbenchContext({
    write() { assert.fail('batch owns its output until all commands settle'); },
    game: {
      async decision() { return { view: `HP ${hp}/75\n` }; },
      async act() { hp--; return { view: `HP ${hp}/75\n` }; },
      async info() { return { view: 'requested mechanics\n' }; },
    },
  });
  const text = await executeWorkbenchCommands(ctx, [['act', ['end']], ['info', ['card Strike']], ['act', ['end']]]);
  assert.equal(text, 'requested mechanics\nHP 73/75\n');
  assert.equal(hp, 73);
});

test('failure during successor rendering still identifies the applied action', async () => {
  const ctx = { write() {}, async printCurrentReading() { throw new Error('reading failed'); } };
  await assert.rejects(executeWorkbenchCommands(ctx, [['claim', []]], async ctx => {
    ctx.recordAction('claim potion');
    await ctx.showSuccessor({});
  }), /reading failed.*Applied before error:.*claim potion/s);
});

test('a route-choice pause stops the batch before later search or mutations', async () => {
  const called = [];
  const ctx = { write() {}, async printCurrentReading(reading) { this.write(reading.view); } };
  const output = await executeWorkbenchCommands(ctx,
    [['loot', []], ['go', []], ['research', []], ['act', ['end']]], async (ctx, command) => {
      called.push(command);
      await ctx.showSuccessor({ view: command === 'go' ? 'Choose next route\n' : 'Rewards\n' });
      return command === 'go' ? { needsChoice: true } : undefined;
    });
  assert.equal(output, 'Choose next route\n');
  assert.deepEqual(called, ['loot', 'go']);
});

test('revealed transform results survive later successors and explicit page queries', async () => {
  const ctx = { write() {}, async printCurrentReading(reading) { this.write(reading.view); } };
  const output = await executeWorkbenchCommands(ctx,
    [['select', []], ['leave', []], ['read', []]], async (ctx, command) => {
      if (command === 'read') { ctx.write('Current map\n'); return; }
      await ctx.showSuccessor({ view: `${command} page\n`, raw: command === 'select'
        ? { revealed_changes: 'Deck change: -Strike, +Havoc\n' } : {} });
    });
  assert.equal(output, 'Deck change: -Strike, +Havoc\nCurrent map\n');
});

test('current-page queries replace the automatic page, survive later actions, and preserve it if the query fails', async () => {
  for (const command of ['read', 'options', 'board']) {
    let hp = 75, fail = false;
    const ctx = createWorkbenchContext({ write() {}, game: {
      async act() { hp--; return { view: `automatic HP ${hp}\n` }; },
      async decision() {
        if (fail && hp === 71) throw new Error('query failed');
        return { view: `requested HP ${hp}\n` };
      },
    } });
    assert.equal(await executeWorkbenchCommands(ctx, [['act', ['end']], [command, []]]), 'requested HP 74\n');
    assert.equal(await executeWorkbenchCommands(ctx, [['act', ['end']], [command, []], ['act', ['end']]]),
      'requested HP 73\nautomatic HP 72\n');
    fail = true;
    await assert.rejects(executeWorkbenchCommands(ctx, [['act', ['end']], [command, []]]),
      /query failed.*Applied before error:.*end.*automatic HP 71/s);
  }
});

test('explicit map queries replace automatic route pages only after success', async () => {
  for (const args of [[], ['full'], ['elites'], ['route', '2', '4']]) {
    const ctx = { write() {}, async printCurrentReading(reading) { this.write(reading.view); } };
    const dispatch = async (ctx, command) => {
      if (command === 'leave') {
        ctx.recordAction('leave shop');
        return ctx.showSuccessor({ view: 'automatic map and routes\n', raw: { reading: {
          options: [{ candidate: { action: { kind: 'choose_route_node' } } }],
        } } });
      }
      ctx.write('requested map\n');
    };
    assert.equal(await executeWorkbenchCommands(ctx, [['leave', []], ['map', args]], dispatch),
      'requested map\n');
    assert.equal(await executeWorkbenchCommands(ctx, [['leave', []], ['map', args], ['leave', []]], dispatch),
      'requested map\nautomatic map and routes\n');
    await assert.rejects(executeWorkbenchCommands(ctx, [['leave', []], ['map', args]],
      (ctx, command) => command === 'map' ? Promise.reject(new Error('map failed')) : dispatch(ctx, command)),
    /map failed.*Applied before error:.*leave shop.*automatic map and routes/s);
  }
});

test('map queries preserve newly opened card rewards and other non-map successors', async () => {
  for (const page of ['Card reward: take Wild Strike / skip', 'Shop: buy / leave', 'Combat: Hand and targets']) {
    const ctx = { write() {}, async printCurrentReading(reading) { this.write(reading.view); } };
    const dispatch = async (ctx, command) => {
      if (command === 'open-card') return ctx.showSuccessor({ view: page + '\n' });
      ctx.write('Next room: Boss Corrupt Heart\n');
    };
    const result = await executeWorkbenchCommands(ctx, [['open-card', []], ['map', []]], dispatch);
    assert.equal(result, page + '\nNext room: Boss Corrupt Heart\n');
    await assert.rejects(executeWorkbenchCommands(ctx, [['open-card', []], ['map', []]],
      (ctx, command) => command === 'map' ? Promise.reject(new Error('map failed')) : dispatch(ctx, command)),
    error => error.message.includes(page) && error.message.includes('map failed'));
  }
});

test('bound sessions reject process and focus controls before any prefix action', async t => {
  const wb = await fixture(t, async () => ({
    async info() { assert.fail('prefix must not execute'); }, async close() {},
  }));
  for (const command of ['exe status', 'use other', 'new other 1', 'refresh other', 'map set flip']) {
    await assert.rejects(wb.execute('info first, ' + command), /not available|shared preferences/);
  }
  await assert.rejects(wb.execute('info first, back'), /separate command/);
});

test('a broken transport releases its connection and read observes real state without replaying actions', async t => {
  let connections = 0, plays = 0, hp = 75;
  const closed = [];
  const wb = await fixture(t, async () => {
    const connection = ++connections;
    return {
      async info() { return { view: 'Known mechanics\n' }; },
      async decision() { return { view: `HP ${hp}/75\n` }; },
      async act() {
        plays++; hp = 70;
        throw Object.assign(new Error('response lost'), { transportFailed: true,
          outcomeUncertain: true, code: 'RUN_PLAY_TIMEOUT', command: 'act',
          requestId: 'exchange-002', recordDirectory: 'original-records' });
      },
      async close() { closed.push(connection); },
    };
  });
  await assert.rejects(wb.execute('info powers , act end'), error => {
    assert.match(error.message, /^response lost.*Known mechanics/s);
    assert.match(error.message, /Use read to reconnect/);
    assert.match(error.message, /no game command was retried/);
    assert.equal(error.transportFailed, true);
    assert.equal(error.outcomeUncertain, true);
    assert.equal(error.code, 'RUN_PLAY_TIMEOUT');
    assert.equal(error.command, 'act');
    assert.equal(error.requestId, 'exchange-002');
    assert.equal(error.recordDirectory, 'original-records');
    return true;
  });
  assert.deepEqual(closed, [1]);
  assert.equal((await wb.execute('read')).text, 'HP 70/75\n');
  assert.equal(connections, 2);
  assert.equal(plays, 1);
});

test('navigation closes the resident first and reconnects after a failed restore', async t => {
  const events = [];
  let connection = 0;
  const wb = await fixture(t, async options => {
    const id = ++connection;
    assert.match(options.recordDirectory, new RegExp(`connection-${id - 1}$`));
    events.push('open ' + id);
    return { async close() { events.push('close ' + id); },
      async info() { return { view: 'connection ' + id }; } };
  }, (_binary, args) => {
    assert.equal(events.at(-1), 'close 1');
    assert.deepEqual(args.slice(3), ['--steps', '2']);
    events.push('back');
    return { status: 1, stdout: '', stderr: 'no retained decision' };
  });
  await assert.rejects(wb.execute('back 2'), error => {
    assert.match(error.message, /no retained decision/);
    assert.equal(error.branchChanged, undefined);
    return true;
  });
  assert.deepEqual(events, ['open 1', 'close 1', 'back', 'open 2']);
  assert.equal((await wb.execute('info any')).text, 'connection 2');
});

test('typed return points stay read-only and restore the issued request after closing the resident', async t => {
  const events = [], request = { schema: 'run-play-return-request-v1', source: { revision: 5 }, target: { revision: 1 } };
  let connection = 0, restored = false;
  const wb = await fixture(t, async () => {
    const id = ++connection; events.push('open ' + id);
    return { async close() { events.push('close ' + id); } };
  }, (_binary, args) => {
    if (args.includes('--list')) {
      assert.deepEqual(args.slice(3), ['--list', '--json']);
      events.push('list');
      return { status: 0, stdout: JSON.stringify({ schema: 'run-play-return-points-v2', restored }) };
    }
    assert.equal(events.at(-1), 'close 1');
    assert.equal(args[3], '--request');
    assert.deepEqual(JSON.parse(readFileSync(args[4], 'utf8')), request);
    restored = true; events.push('restore');
    return { status: 0, stdout: JSON.stringify({ to: { revision: 6 }, previous_endpoint: { revision: 5 } }) };
  });
  assert.equal((await wb.returnPoints()).restored, false);
  const result = await wb.restorePoint(request);
  assert.deepEqual(result.receipt.to, { revision: 6 });
  assert.equal(result.points.restored, true);
  assert.deepEqual(events, ['open 1', 'list', 'close 1', 'restore', 'open 2', 'list']);
});

test('a rejected typed return reconnects without retrying the restore', async t => {
  const events = []; let connection = 0, restores = 0;
  const wb = await fixture(t, async () => {
    const id = ++connection; events.push('open ' + id);
    return { async close() { events.push('close ' + id); } };
  }, (_binary, args) => {
    if (args.includes('--list')) return { status: 0, stdout: JSON.stringify({ schema: 'run-play-return-points-v2', current: 'unchanged' }) };
    assert.equal(events.at(-1), 'close 1'); restores++;
    return { status: 1, stderr: 'return request source expired' };
  });
  await assert.rejects(wb.restorePoint({ schema: 'run-play-return-request-v1' }), /source expired/);
  assert.deepEqual(events, ['open 1', 'close 1', 'open 2']);
  assert.equal(restores, 1);
  assert.equal((await wb.returnPoints()).current, 'unchanged');
});

test('return points reject an incompatible response before presentation', async t => {
  const wb = await fixture(t, async () => ({ async close() {} }),
    () => ({ status: 0, stdout: JSON.stringify({ schema: 'run-play-return-points-v1' }) }));
  await assert.rejects(wb.returnPoints(), /Unsupported return points schema/);
});

test('a committed typed return retains its receipt when reconnecting fails', async t => {
  let connections = 0;
  const receipt = { schema: 'run-play-return-receipt-v1', from: { revision: 3 }, to: { revision: 4 } };
  const wb = await fixture(t, async () => {
    if (++connections === 2) throw new Error('new connection failed');
    return { async close() {} };
  }, () => ({ status: 0, stdout: JSON.stringify(receipt) }));
  await assert.rejects(wb.restorePoint({ schema: 'run-play-return-request-v1' }), error => {
    assert.match(error.message, /new connection failed/);
    assert.deepEqual(error.receipt, receipt);
    assert.equal(error.branchChanged, true);
    return true;
  });
});

test('floor targets and marks use the Rust owner while listing leaves the resident open', async t => {
  const events = [], calls = [];
  let connection = 0;
  const wb = await fixture(t, async () => {
    const id = ++connection;
    events.push('open ' + id);
    return { async close() { events.push('close ' + id); },
      async decision() { return { view: 'Map at connection ' + id }; } };
  }, (_binary, args) => {
    calls.push(args);
    if (args.includes('--list') || args.length === 3) {
      assert.equal(events.at(-1), 'open 1');
      return { status: 0, stdout: 'available return points\n' };
    }
    assert.match(events.at(-1), /^close /);
    return { status: 0, stdout: 'owner receipt\n' };
  });
  const listed = await wb.execute('read, back list, mark, read');
  assert.equal(listed.branchChanged, undefined);
  assert.equal(listed.text,
    'Map at connection 1\navailable return points\navailable return points\nMap at connection 1\n');
  assert.equal(connection, 1);
  for (const command of ['mark "Before boss"', 'back floor 49', 'back mark "Before boss"', 'mark drop "Before boss"']) {
    const result = await wb.execute(command);
    assert.match(result.text, /^Map at connection/);
    assert.equal(result.branchChanged, command.startsWith('back ') ? true : undefined);
  }
  assert.deepEqual(calls.map(args => [args[0], ...args.slice(3)]), [
    ['back', '--list'], ['mark'], ['mark', '--name', 'Before boss'],
    ['back', '--floor', '49'], ['back', '--mark', 'Before boss'], ['mark', '--drop', 'Before boss'],
  ]);
  assert.equal(connection, 5);
});

test('a successful restore remains a branch change if reconnecting fails', async t => {
  let connection = 0;
  const wb = await fixture(t, async () => {
    if (++connection === 2) throw new Error('reconnect failed');
    return { async close() {}, async decision() { return { view: 'Restored branch' }; } };
  }, () => ({ status: 0, stdout: 'restored' }));
  await assert.rejects(wb.execute('back floor 36'), error => {
    assert.equal(error.branchChanged, true);
    assert.match(error.message, /^reconnect failed.*Applied before error:.*back floor 36/s);
    return true;
  });
  assert.equal((await wb.execute('read')).text, 'Restored branch\n');
});

test('malformed floor and mark arguments fail before navigation or grouped prefixes execute', async t => {
  for (const args of [['floor'], ['floor', '-1'], ['floor', '2147483648'], ['list', '2'], ['mark'], ['0']]) {
    assert.throws(() => navigationArguments('back', args), /back.*retained targets/);
  }
  for (const args of [['drop'], ['drop', 'one', 'two']]) {
    assert.throws(() => navigationArguments('mark', args), /mark/);
  }
  const wb = await fixture(t, async () => ({
    async info() { assert.fail('prefix must not execute'); }, async close() {},
  }));
  for (const command of ['info first, mark BeforeBoss', 'info first, mark drop BeforeBoss',
    'info first, back floor 49', 'info first, back list extra', 'info first, undo']) {
    await assert.rejects(wb.execute(command), /separate command/);
  }
});

test('queued cancellation and shutdown do not abandon an active command', async t => {
  let finish;
  const gate = new Promise(resolve => { finish = resolve; });
  let calls = 0, closed = false;
  const wb = await fixture(t, async () => ({
    async info() { calls++; await gate; assert.equal(closed, false); return { view: 'settled' }; },
    async close() { closed = true; },
  }));
  const first = wb.execute('info first');
  const controller = new AbortController();
  const second = wb.execute('info cancelled', { signal: controller.signal });
  const rejected = assert.rejects(second, /abort/i);
  controller.abort();
  const closing = wb.close();
  finish();
  assert.equal((await first).text, 'settled');
  await rejected;
  await closing;
  assert.equal(calls, 1);
  assert.equal(closed, true);
  await assert.rejects(wb.execute('read'), /closed/);
});

test('mechanics subprocess failures retain their actual error', async () => {
  for (const failure of [
    { status: 1, stdout: 'detail', stderr: 'failure' },
    { error: new Error('cannot launch') },
  ]) {
    const ctx = { write() { assert.fail('no success output'); }, runEngine() { return failure; } };
    await assert.rejects(info(ctx, ['card', 'Unknown']), /failure|cannot launch/);
  }
});
