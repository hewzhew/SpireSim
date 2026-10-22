import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, readFile, readdir, rm, realpath } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { Context } from '@deepseek-ai/cordis';
import LlmRuntime, { ToolCallId, createUserMessage } from '@deepseek-ai/dsh-llm';
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session';
import ProjectionRegistry from '@deepseek-ai/dsh-session-projection';
import AgentRegistry from '@deepseek-ai/dsh-agent';
import AgentLoop from '@deepseek-ai/dsh-agent-loop';
import SystemPrompt from '@deepseek-ai/dsh-system-prompt';
import * as SessionPersistence from '@deepseek-ai/dsh-session-persistence-jsonl';
import CommandRuntime from '@deepseek-ai/dsh-commands';
import ToolRuntime, { defineTool } from '@deepseek-ai/dsh-tools';
import { createScope, bindScopeParent, scopeOf } from '@deepseek-ai/dsh-scope';
import * as Native from './native.mjs';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const realBinary = join(repositoryRoot, 'target/dev-opt/run_play.exe');
const textOf = value => value.content.filter(block => block.type === 'text').map(block => block.text).join('\n');

async function workspace(t, { prefix = 'sts-native-', beforeRemove = async () => {} } = {}) {
  const base = await realpath(tmpdir());
  const repository = await mkdtemp(join(base, prefix));
  t.after(async () => {
    await beforeRemove();
    const target = await realpath(repository);
    assert.ok(target.startsWith(base + sep) && dirname(target) === base && target.split(/[\\/]/).at(-1).startsWith(prefix));
    await rm(target, { recursive: true, force: true });
  });
  return repository;
}

test('chat binding creates once, survives reopen, and refuses another chat or an unowned directory', async t => {
  const repository = await workspace(t);
  const binary = join(repository, 'game.exe'); await writeFile(binary, 'fixture');
  const agent = { id: 'chat-a', session: { header: { cwd: repository } } };
  const calls = [];
  const run = async (exe, args) => { calls.push([exe, args]); await mkdir(args[2], { recursive: true }); };
  const first = await Native.nativeBinding(agent, { binary }, { run });
  assert.equal(first.ascension, 20); assert.equal(first.goal, 'heart-victory');
  assert.equal(first.agentLocale, 'en');
  assert.deepEqual(await Native.nativeBinding(agent, { binary }, { run }), first);
  assert.equal(calls.length, 1);
  const other = await Native.nativeBinding({ ...agent, id: 'chat-b' }, { binary, manual: true, agentLocale: 'zh-CN' }, { run });
  assert.notEqual(other.session, first.session);
  assert.equal(other.manual, true);
  assert.equal(other.agentLocale, 'zh-CN');
  assert.equal((await Native.nativeBinding({ ...agent, id: 'chat-b' }, { binary }, { run })).manual, true);
  assert.equal((await Native.nativeBinding({ ...agent, id: 'chat-b' }, { binary }, { run })).agentLocale, 'zh-CN');
  await assert.rejects(Native.nativeBinding({ ...agent, id: 'chat-b' }, { binary, agentLocale: 'en' }, { run }), /different agent language/);
  await assert.rejects(Native.nativeBinding({ ...agent, id: 'invalid-locale' }, { binary, agentLocale: 'fr' }, { run }), /Unsupported agentLocale/);
  await assert.rejects(Native.nativeBinding(agent, { binary, manual: true }, { run }), /different play mode/);
  await assert.rejects(Native.nativeBinding({ ...agent, id: 'chat-b' }, { binary, manual: false }, { run }), /另一种玩法/);
  assert.equal(calls.length, 2);
  const file = join(first.session, 'dsh-native.json');
  await writeFile(file, JSON.stringify({ ...first, agent: 'someone-else' }));
  await assert.rejects(Native.nativeBinding(agent, { binary }, { run }), /binding is invalid/);
  assert.equal(calls.length, 2);
  await rm(join(other.session, 'dsh-native.json'));
  await assert.rejects(Native.nativeBinding({ ...agent, id: 'chat-b' }, { binary }, { run }), /not overwritten/);
  assert.equal(calls.length, 2);
});

test('native preset capability is lazy and isolates real games for two agents sharing one plugin instance',
  { skip: !existsSync(realBinary), timeout: 20000 }, async t => {
    const ctx = new Context();
    const handles = [];
    const repository = await workspace(t, { beforeRemove: async () => {
      for (const handle of handles) await handle.dispose();
      await ctx.fiber.dispose();
    } });
    for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, AgentRegistry, ToolRuntime, CommandRuntime]) await ctx.plugin(plugin);
    await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false });
    ctx.systemPrompt.section({ name: 'fixture:global-coding-guide', order: 10000, text: 'Inspect the harness checkout and rebuild Web artifacts.' });
    await ctx.plugin(AgentLoop, { agents: [] });
    ctx.tools.register(defineTool({ name: 'unrelated_global', description: 'fixture', parameters: {},
      output: { schema: { type: 'string' }, render: (_args, value) => [{ type: 'text', text: value }] },
      execute: () => 'unrelated' }));
    const presetKey = {};
    const preset = createScope(ctx, presetKey);
    const plugin = await preset.ctx.plugin(Native, { binary: realBinary });
    assert.deepEqual(await readdir(repository), [], 'loading the capability must not create a game');
    const create = async id => {
      const handle = await ctx.agents.create({ sessionId: SessionId(id), meta: { cwd: repository },
        agentOptions: { provider: 'unused', model: 'unused' },
        setup: scoped => { bindScopeParent(scopeOf(scoped), presetKey); } });
      handles.push(handle); return handle;
    };
    const a = await create('native-a'), b = await create('native-b');
    assert.deepEqual(ctx.tools.schemas(a.agent).map(tool => tool.name).sort(), ['sts', 'sts_feedback']);
    assert.ok(ctx.tools.schemas().some(tool => tool.name === 'unrelated_global'), 'gameplay restriction does not change the host tool registry');
    const gamePrompt = await ctx.systemPrompt.assemble({ scope: scopeOf(a.agent.ctx) });
    assert.deepEqual(gamePrompt.sections.map(section => section.text), [Native.NATIVE_GUIDE]);
    assert.ok((await ctx.systemPrompt.assemble()).sections.some(section => section.name === 'fixture:global-coding-guide'),
      'the game prompt leaves the host coding prompt available in its own scope');
    const execute = (handle, command, signal = new AbortController().signal) => ctx.tools.execute({
      name: 'sts', arguments: { command }, agent: handle.agent, signal, callId: ToolCallId(randomUUID()),
    });
    const [initialA, initialB] = await Promise.all([execute(a, 'read'), execute(b, 'read')]);
    for (const result of [initialA, initialB]) {
      assert.equal(Boolean(result.isError), false, textOf(result)); assert.match(textOf(result), /Neow \| Event screen 1/);
      assert.equal(result.meta.currentReading.control, 'strategic');
      assert.ok(textOf(result).includes(result.meta.currentReading.text));
      assert.doesNotMatch(textOf(result), /currentReading|"summary"/);
    }
    assert.match(textOf(initialB), /^event:Neow:0  \|  Effect:/m);
    const mistakes = [
      ['info event Neow', /use info with no arguments/],
      ['preview 3', /semantic actions, not row numbers/],
      ['plans', /combat-only/],
      ['research', /combat-only/],
      ['act 0', /semantic actions, not row numbers/],
      ['act ordinal:event:Neow:0', /semantic actions, not row numbers/],
      ['act ordinal:0', /semantic actions, not row numbers/],
      ['choose 0', /Submit one complete command verbatim/],
      ['choose event:Neow:0', /Submit one complete command verbatim/],
    ];
    for (const [command, message] of mistakes) {
      const failed = await execute(b, command);
      assert.equal(failed.isError, true, command);
      assert.match(textOf(failed), message, command);
      if (command !== 'info event Neow') assert.match(textOf(failed), /\n  (?:preview )?event:Neow:0/);
      if (command === 'preview 3') assert.doesNotMatch(textOf(failed), /\n  event:/, 'preview recovery must remain private');
      assert.doesNotMatch(textOf(failed), /explicit ordinal:<path>|invalid digit/);
      assert.equal(textOf(await execute(b, 'read')), textOf(initialB), `${command} must not change the game`);
    }
    const preview = await execute(b, 'preview event:Neow:0');
    assert.equal(Boolean(preview.isError), false, textOf(preview));
    assert.equal(preview.meta.currentReading, undefined);
    assert.match(textOf(preview), /Private result/);
    assert.equal(textOf(await execute(b, 'read')), textOf(initialB));
    const trade = await execute(b, 'preview event:Neow:3');
    assert.equal(Boolean(trade.isError), false, textOf(trade));
    assert.match(textOf(trade), /Preview inventory changes:\n/);
    assert.match(textOf(trade), /^  Relics: -1 Burning Blood, \+1 /m);
    assert.equal(textOf(await execute(b, 'read')), textOf(initialB));
    const currentInfo = await execute(b, 'info');
    assert.equal(Boolean(currentInfo.isError), false, textOf(currentInfo));
    assert.ok(textOf(currentInfo).includes(currentInfo.meta.currentReading.text));
    const mixed = await execute(b, 'info, inventory effects, deck');
    assert.equal(mixed.isError, true);
    assert.match(textOf(mixed), /^Error: deck is not a command\. Use inventory/);
    assert.ok(textOf(mixed).includes(`Earlier results:\n${currentInfo.meta.currentReading.text.trimEnd()}`));
    assert.doesNotMatch(textOf(mixed), /candidates=|transition=/);
    assert.match(textOf(mixed), /Deck \(11\)/);
    assert.doesNotMatch(textOf(mixed), /\.\/p /);
    assert.equal(textOf(await execute(b, 'read')), textOf(initialB));
    const games = await readdir(join(repository, '.oracle-lab/run-play'));
    assert.equal(games.length, 2);
    const partial = await execute(a, 'event:Neow:1, not-a-current-option');
    assert.equal(partial.isError, true);
    assert.match(textOf(partial), /^Error: no unique option matching "not-a-current-option"/);
    assert.match(textOf(partial), /Applied before error:/);
    assert.doesNotMatch(textOf(partial), /\.\/p /);
    const changedA = textOf(await execute(a, 'read'));
    assert.notEqual(changedA, textOf(initialA));
    assert.equal(textOf(await execute(b, 'read')), textOf(initialB));
    const abort = new AbortController(); abort.abort();
    const canceled = await execute(b, 'event:Neow:1', abort.signal);
    assert.equal(canceled.isError, true);
    assert.equal(textOf(await execute(b, 'read')), textOf(initialB));
    await a.dispose();
    assert.equal(textOf(await execute(b, 'read')), textOf(initialB), 'closing one chat must not close the other');
    const reopened = await create('native-a');
    assert.equal(textOf(await execute(reopened, 'read')), changedA);
    assert.equal((await readdir(join(repository, '.oracle-lab/run-play'))).length, 2);
    const assembly = await ctx.systemPrompt.assemble({ agent: b.agent, scope: b.agent });
    assert.ok(assembly.sections.some(section => section.name === 'sts:native-guide'));
    assert.ok(!assembly.sections.some(section => section.name === 'sts:run-rules'));
    await plugin.dispose();
    assert.ok(!ctx.tools.schemas(b.agent).some(tool => tool.name === 'sts'));
  });

test('Chinese binding diagnostics retain the saved game and do not create or rewrite another run', async t => {
  const repository = await workspace(t);
  const binary = join(repository, 'game.exe'); await writeFile(binary, 'fixture');
  const agent = { id: 'binding-zh', session: { header: { cwd: repository } } };
  let calls = 0;
  const run = async (_exe, args) => { calls++; await mkdir(args[2], { recursive: true }); };
  const binding = await Native.nativeBinding(agent, { binary, manual: true, agentLocale: 'zh-CN' }, { run });
  const file = join(binding.session, 'dsh-native.json');
  const before = await readFile(file);
  await assert.rejects(Native.nativeBinding(agent, { binary, manual: false }, { run }), /另一种玩法/);
  await assert.rejects(Native.nativeBinding(agent, { binary: 'relative.exe' }, { run }), /绝对路径/);
  await assert.rejects(Native.nativeBinding(agent, { binary, agentLocale: 'en' }, { run }), /different agent language/);
  await assert.rejects(Native.readNativeBinding({ id: 'no-workspace', session: { header: { cwd: 'relative' } } },
    { agentLocale: 'zh-CN' }), /工作区/);
  assert.deepEqual(await readFile(file), before);
  assert.equal(calls, 1);
  assert.equal((await Native.nativeBinding(agent, { binary }, { run })).agentLocale, 'zh-CN');
});

test('bundle distribution declares native entry, localized metadata, icon, and no creation side effects', async () => {
  const packageDir = dirname(fileURLToPath(import.meta.url));
  const manifest = JSON.parse(await readFile(join(packageDir, 'package.json'), 'utf8'));
  assert.equal(manifest.dsh.bundle.patch, './cordis.patch.yml');
  assert.equal(manifest.dependencies['@sts-simulator/presentation'], 'file:../presentation');
  assert.ok(manifest.bundledDependencies.includes('@sts-simulator/presentation'));
  assert.equal(manifest.exports['./native'], './native.mjs');
  const patch = await readFile(join(packageDir, 'cordis.patch.yml'), 'utf8');
  assert.match(patch, /@sts-simulator\/dsh-player\/native/);
  assert.doesNotMatch(patch, /dsh-player\/context|includeRuntimeContext: true|default:/);
  for (const language of ['zh', 'en']) {
    const locale = JSON.parse(await readFile(join(packageDir, `locale/${language}.json`), 'utf8'));
    assert.ok(locale.meta.title && locale.meta.description);
  }
});

test('one preset gives English and Chinese chats independent model guides and tool schemas',
  { skip: !existsSync(realBinary), timeout: 20000 }, async t => {
    const ctx = new Context(), handles = [];
    const repository = await workspace(t, { beforeRemove: async () => {
      for (const handle of handles) await handle.dispose();
      await ctx.fiber.dispose();
    } });
    for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, AgentRegistry, ToolRuntime, CommandRuntime]) await ctx.plugin(plugin);
    await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false });
    await ctx.plugin(AgentLoop, { agents: [] });
    const presetKey = {}, preset = createScope(ctx, presetKey);
    const plugin = await preset.ctx.plugin(Native, { binary: realBinary });
    await Native.nativeBinding({ id: 'locale-zh', session: { header: { cwd: repository } } },
      { binary: realBinary, manual: true, agentLocale: 'zh-CN' });
    const descriptions = [], signatures = [];
    for (const [id, locale, manual] of [['locale-en', 'en', false], ['locale-zh', 'zh-CN', true]]) {
      const handle = await ctx.agents.create({ sessionId: SessionId(id), meta: { cwd: repository },
        agentOptions: { provider: 'unused', model: 'unused' }, setup: scoped => { bindScopeParent(scopeOf(scoped), presetKey); } });
      handles.push(handle);
      const prompt = await ctx.systemPrompt.assemble({ scope: scopeOf(handle.agent.ctx) });
      assert.deepEqual(prompt.sections.map(section => section.text), [Native.nativeGuide(manual, locale)]);
      const tools = ctx.tools.schemas(handle.agent);
      assert.deepEqual(tools.map(tool => tool.name).sort(), ['sts', 'sts_feedback']);
      assert.equal(Native.nativeGameControl(handle.agent).workbench, undefined);
      const game = tools.find(tool => tool.name === 'sts');
      const feedback = tools.find(tool => tool.name === 'sts_feedback');
      descriptions.push([game.description, game.parameters.properties.command.description, feedback.description]);
      signatures.push(JSON.parse(JSON.stringify(tools, (key, value) => key === 'description' ? undefined : value)));
      assert.deepEqual(Object.keys(game.parameters.properties), ['command']);
      const missing = await ctx.tools.execute({ name: 'sts_feedback', arguments: { message: ' ' }, agent: handle.agent,
        signal: new AbortController().signal, callId: ToolCallId(randomUUID()) });
      assert.equal(missing.isError, true);
      assert.match(textOf(missing), locale === 'en' ? /concrete problem/ : /具体问题/);
      assert.equal(Native.nativeGameControl(handle.agent).workbench, undefined, 'validation does not create or open a game');
      assert.equal(handle.agent.session.snapshotEvents().filter(event => event.type === 'step/start').length, 0);
    }
    assert.match(descriptions[0][0], /Operate this chat/);
    assert.match(descriptions[1][0], /当前聊天/);
    assert.notDeepEqual(descriptions[0], descriptions[1]);
    assert.deepEqual(signatures[0], signatures[1], 'language changes descriptions, not the tool schema or callable identities');
    assert.equal(ctx.tools.schemas(handles[0].agent).find(tool => tool.name === 'sts').description, descriptions[0][0]);
    await plugin.dispose();
    for (const handle of handles) {
      assert.ok(!ctx.tools.schemas(handle.agent).some(tool => tool.name === 'sts'));
      const prompt = await ctx.systemPrompt.assemble({ scope: scopeOf(handle.agent.ctx) });
      assert.ok(!prompt.sections.some(section => section.name === 'sts:native-guide'));
    }
  });

test('native workspace returns and restores its prior endpoint through the same real game owner without model calls',
  { skip: !existsSync(realBinary), timeout: 20000 }, async t => {
    const ctx = new Context(); let handle;
    const repository = await workspace(t, { beforeRemove: async () => { await handle?.dispose(); await ctx.fiber.dispose(); } });
    await mkdir(join(repository, 'tools/run_play'), { recursive: true });
    await writeFile(join(repository, 'tools/run_play/workbench.mjs'),
      `export {openWorkbench} from ${JSON.stringify(new URL('../run_play/workbench.mjs', import.meta.url).href)};`);
    for (const plugin of [LlmRuntime, SessionStore, ProjectionRegistry, AgentRegistry, ToolRuntime, CommandRuntime]) await ctx.plugin(plugin);
    await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false });
    await ctx.plugin(SessionPersistence, { root: join(repository, 'dsh/sessions'), compression: 'zstd' });
    await ctx.plugin(AgentLoop, { agents: [] });
    const presetKey = {}, preset = createScope(ctx, presetKey);
    await preset.ctx.plugin(Native, { binary: realBinary });
    await Native.nativeBinding({ id: 'real-native-return', session: { header: { cwd: repository } } },
      { binary: realBinary, manual: true, agentLocale: 'zh-CN' });
    handle = await ctx.agents.create({ sessionId: SessionId('real-native-return'), meta: { cwd: repository },
      agentOptions: { provider: 'unused', model: 'unused' },
      setup: scoped => { bindScopeParent(scopeOf(scoped), presetKey); } });
    const execute = command => ctx.tools.execute({ name: 'sts', arguments: { command }, agent: handle.agent,
      signal: new AbortController().signal, callId: ToolCallId(randomUUID()) });
    const initial = textOf(await execute('read'));
    const control = Native.nativeGameControl(handle.agent);
    const beforeInfo = (await control.points()).points.source;
    assert.match(textOf(await execute('info potion SteroidPotion')), /药水定义：类固醇药水 \| ID：SteroidPotion/);
    assert.deepEqual((await control.points()).points.source, beforeInfo);
    const prompt = await ctx.systemPrompt.assemble({ scope: scopeOf(handle.agent.ctx) });
    assert.deepEqual(prompt.sections.map(section => section.text), [Native.nativeGuide(true, 'zh-CN')]);
    assert.match(ctx.tools.schemas(handle.agent).find(tool => tool.name === 'sts').description, /当前聊天/);
    assert.doesNotMatch(prompt.sections[0].text, /research whole_combat|deck-lab|piles sl|preview play:/);
    assert.equal((await control.workbench.observe()).capabilities.permission_envelope.profile, 'public_manual_v1');
    assert.match(textOf(await execute('preview end')), /公开信息手打不提供搜索/);
    const start = (await control.points()).points.current;
    await execute('event:Neow:1');
    const endpoint = await control.live();
    const listed = await control.points();
    const target = listed.points.targets.find(point => point.identity.state_revision === start.identity.state_revision);
    assert.ok(target, 'initial Neow decision is a retained precise point');
    const returned = await control.restore({ schema: 'run-play-return-request-v1', source: listed.points.source, target: target.identity });
    assert.equal(returned.status, 'ready', returned.message); assert.ok(returned.text.startsWith(initial));
    assert.equal(control.workbench, undefined, 'paused restore releases its executable after settlement');
    assert.equal(Native.nativeGameControl(handle.agent), control, 'restore keeps the same game owner');
    assert.equal(handle.agent.status, 'idle'); assert.equal(control.paused, true);
    const now = await control.points(), previous = now.points.targets.find(point => point.origins.some(origin => origin.kind === 'previous_endpoint'));
    const back = await control.restore({ schema: 'run-play-return-request-v1', source: now.points.source, target: previous.identity });
    assert.equal(back.status, 'ready', back.message); assert.equal(back.text, endpoint);
    assert.equal((await control.restore({ schema: 'run-play-return-request-v1', source: listed.points.source, target: target.identity })).status, 'error');
    assert.equal(await control.live(), endpoint, 'stale restore was not adopted');
    assert.equal(handle.agent.session.snapshotEvents().filter(event => event.type === 'step/start').length, 0);
    handle.agent.followup(createUserMessage({ source: { kind: 'user' }, content: [{ type: 'text', text: 'Keep this queued after restart.' }] }));
    await handle.agent.whenIdle(); assert.equal(control.status().queued, 1);
    await ctx.sessions.flush(handle.agent.session); await handle.dispose();
    handle = await ctx.agents.resume({ resumeSessionId: SessionId('real-native-return'),
      agentOptions: { provider: 'unused', model: 'unused' }, setup: scoped => { bindScopeParent(scopeOf(scoped), presetKey); } });
    await handle.agent.whenIdle();
    const resumedOwner = Native.nativeGameControl(handle.agent);
    assert.equal(resumedOwner.config.manual, true);
    assert.equal(resumedOwner.paused, true);
    assert.equal(resumedOwner.status().queued, 0, 'official Agent disposal explicitly clears its inbox; the plugin does not recreate discarded input');
    assert.equal(resumedOwner.workbench, undefined, 'restoring the official chat does not open Rust or run a model');
    assert.equal(await resumedOwner.live(), endpoint);
    assert.equal(resumedOwner.binding.agentLocale, 'zh-CN');
    const reopenedPrompt = await ctx.systemPrompt.assemble({ scope: scopeOf(handle.agent.ctx) });
    assert.deepEqual(reopenedPrompt.sections.map(section => section.text), [Native.nativeGuide(true, 'zh-CN')]);
    assert.match(ctx.tools.schemas(handle.agent).find(tool => tool.name === 'sts').description, /当前聊天/);
    const reopenedInfo = await resumedOwner.serial(async () => (await resumedOwner.game(false)).execute('info potion FairyPotion'));
    assert.match(reopenedInfo.text, /绽放印记会阻止此次复活/);
    assert.equal(handle.agent.session.snapshotEvents().filter(event => event.type === 'step/start').length, 0);
  });
