import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, writeFile, rm, realpath } from 'node:fs/promises';
import { dirname, join, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { runInNewContext } from 'node:vm';
import { Context } from '@deepseek-ai/cordis';
import SessionStore from '@deepseek-ai/dsh-session';
import ProjectionRegistry from '@deepseek-ai/dsh-session-projection';
import { nativeBinding } from './native.mjs';
import { StsMapView, mapViewFromDecision, mapTickProjection } from './map-view.mjs';
import { validateWebConfig, ownedWebProcess, localHmrPatch } from './web.mjs';
import { createHostLocale } from './client/test-support.mjs';
import { mapNamespace } from './client/locale.mjs';

const publicMap = { current_x: 1, current_y: 0, boss: 'Hexaghost', nodes: [
  { x: 1, y: 0, room: 'monster', has_emerald_key: false, edges: [{ destination_x: 0, destination_y: 1 }, { destination_x: 2, destination_y: 1 }] },
  { x: 0, y: 1, room: 'elite', has_emerald_key: true, edges: [{ destination_x: 1, destination_y: 2 }] },
  { x: 2, y: 1, room: 'rest', has_emerald_key: false, edges: [{ destination_x: 1, destination_y: 2 }] },
  { x: 1, y: 2, room: 'boss', has_emerald_key: false, edges: [] },
] };
const decision = () => ({ decision: { observation: { domain: 'strategic', observation: {
  run: { act: 1, floor: 1 }, public_map: structuredClone(publicMap),
} } } });

test('map picture retains branches and current position across strategic rooms and combat', () => {
  const strategic = mapViewFromDecision(decision());
  assert.equal(strategic.act, 1); assert.deepEqual(strategic.current, [1, 0]);
  assert.match(strategic.picture, /E.*R/); assert.ok(strategic.picture.indexOf('B') < strategic.picture.indexOf('M'));
  assert.match(strategic.picture, /[\\/]/); assert.doesNotMatch(strategic.picture, /[@K]/);
  const rooms = strategic.lines.flat().filter(token => token.kind === 'room');
  assert.equal(rooms.find(token => token.state === 'current').text, 'M');
  assert.equal(rooms.find(token => token.emerald).text, 'E');
  assert.equal(rooms.find(token => token.text === 'B').room, 'boss');
  assert.equal(strategic.boss, 'Hexaghost');
  assert.ok(!('note' in strategic) && !('message' in strategic));
  const combat = mapViewFromDecision({ decision: { observation: { domain: 'combat', run_context: {
    status: 'available', act: 1, floor: 1, public_map: publicMap,
  } } } });
  assert.equal(combat.picture, strategic.picture); assert.equal(combat.floor, 1);
  const start = decision(); start.decision.observation.observation.public_map.current_x = -1;
  start.decision.observation.observation.public_map.current_y = -1;
  assert.doesNotMatch(mapViewFromDecision(start).picture, /@/);
  assert.deepEqual(mapViewFromDecision(start).current, [-1, -1]);
  assert.equal(mapViewFromDecision({ decision: { observation: { domain: 'combat', run_context: { status: 'unavailable' } } } }).status, 'unavailable');
  const broken = decision(); broken.decision.observation.observation.public_map.nodes.pop();
  assert.throws(() => mapViewFromDecision(broken), /no published destination/);
});

test('map refresh projection follows completed game calls including errors, without any map or model text in the log', () => {
  const initial = mapTickProjection.init();
  assert.equal(mapTickProjection.apply(initial, { type: 'assistant/message', data: {} }), initial);
  const started = mapTickProjection.apply(initial, { type: 'tool/call', data: { name: 'sts', callId: 'a' } });
  const failed = mapTickProjection.apply(started, { seq: 12, type: 'tool/result', data: { message: { toolCallId: 'a', isError: true } } });
  assert.equal(mapTickProjection.wire.view(failed), 12); assert.equal(failed.callId, null);
  const next = mapTickProjection.apply(failed, { seq: 13, type: 'tool/result', data: { message: {}, meta: { schema: 'sts-native-game-v1' } } });
  assert.equal(mapTickProjection.wire.view(next), 13);
});

test('human map read does not initialize a game and uses only the selected chat binding and public decision', async t => {
  const base = await realpath(tmpdir()), repository = await mkdtemp(join(base, 'sts-map-view-'));
  const ctx = new Context();
  t.after(async () => {
    await ctx.fiber.dispose(); const target = await realpath(repository);
    assert.ok(dirname(target) === base && target.startsWith(base + sep) && target.split(/[\\/]/).at(-1).startsWith('sts-map-view-'));
    await rm(target, { recursive: true });
  });
  await ctx.plugin(SessionStore); await ctx.plugin(ProjectionRegistry);
  await mkdir(join(repository, 'target/dev-opt'), { recursive: true });
  await writeFile(join(repository, 'target/dev-opt/run_play.exe'), 'fixture');
  const session = ctx.sessions.create('map-chat', { meta: { cwd: repository } });
  const calls = [];
  const view = new StsMapView(ctx, { inspectSession: async id => {
    const selected = ctx.sessions.get(id); if (!selected) throw new Error('fixture: 未加载');
    return { meta: selected.header };
  }, execute: async (binary, args) => { calls.push({ binary, args }); return { stdout: JSON.stringify(decision()) }; } });
  assert.equal((await view.read(session.id)).status, 'not-started'); assert.equal(calls.length, 0);
  const binding = await nativeBinding({ id: session.id, session }, {}, { run: async (_binary, args) => mkdir(args[2]) });
  const before = await readFile(join(binding.session, 'dsh-native.json'), 'utf8');
  assert.equal((await view.read(session.id)).status, 'ready');
  assert.deepEqual(calls[0].args, ['show', '--session', binding.session, '--decision-json']);
  assert.equal(await readFile(join(binding.session, 'dsh-native.json'), 'utf8'), before);
  await assert.rejects(view.read('another-chat'), /未加载/);
});

test('client registers map and observation controls through official slots, and cleans up registrations', async () => {
  let definition;
  const packageDir = dirname(fileURLToPath(import.meta.url));
  runInNewContext(await readFile(join(packageDir, 'client.js'), 'utf8'), { window: { __ModuleLoader__: { load: value => { definition = value; } } } });
  assert.equal(definition.id, '@sts-simulator/dsh-player');
  const module = definition.factory(name => { assert.equal(name, 'react'); return {}; });
  const entries = [], effects = [], ctx = {
    locale: await createHostLocale(),
    effect: register => effects.push(register()),
    sidebarRightTabs: { register: entry => { entries.push(entry); return () => entries.pop(); } },
    slots: { inject: (_slot, register) => register(), register: entry => { entries.push(entry); return () => entries.pop(); } },
  };
  module.apply(ctx);
  assert.ok(module.inject.includes('locale'));
  assert.deepEqual(entries.map(entry => entry.kind ?? entry.name), ['sts-map', 'sidebar.right.pane.tab.title', 'conversation.session.header.actions', 'sidebar.right.pane.tab',
    'sts-observe', 'sidebar.right.pane.tab.title', 'conversation.session.header.actions', 'sidebar.right.pane.tab']);
  effects.reverse().forEach(dispose => dispose()); assert.equal(entries.length, 0);
});

test('map and update controls use named Remote args and only publish on a click', async () => {
  let definition, panel;
  const pendingEffects = [], calls = [], updates = [];
  const react = { createElement: (type, props, ...children) => ({ type, props, children }),
    useState: initial => [initial, value => updates.push(value)],
    useReducer: () => [0, () => {}], useEffect: effect => pendingEffects.push(effect) };
  runInNewContext(await readFile(new URL('./client.js', import.meta.url), 'utf8'), {
    window: { __ModuleLoader__: { load: value => { definition = value; } } }, AbortController,
  });
  const ctx = { locale: await createHostLocale(), effect: effect => effect(), sidebarRightTabs: { register: () => () => {} },
    slots: { inject: (_name, register) => register(), register: (entry, component) => {
      if (entry.name === 'sidebar.right.pane.tab' && entry.key === 'sts-map') panel = component;
      return () => {};
    } }, connection: { rpc: { call: async (channel, endpoint, payload) => {
      calls.push({ channel, endpoint, payload: JSON.parse(JSON.stringify(payload)) });
      return { ok: true, value: { status: 'ready', reason: 'published' } };
    } } } };
  definition.factory(() => react).apply(ctx);
  const tree = panel({ sessionId: 'selected-chat', useProjection: () => 1, t: ctx.locale.bind(mapNamespace) });
  const cleanups = pendingEffects.map(effect => effect());
  await Promise.resolve();
  assert.deepEqual(calls, [
    { channel: '/api', endpoint: 'stsToolUpdates/status', payload: { args: {} } },
    { channel: '/api', endpoint: 'stsMapView/read', payload: { args: { sessionId: 'selected-chat' } } },
  ]);
  function find(node) {
    if (!node || typeof node !== 'object') return null;
    if (node.type === 'button' && node.children.includes('更新工具')) return node;
    return node.children?.map(find).find(Boolean);
  }
  await find(tree).props.onClick();
  assert.deepEqual(calls.at(-1), { channel: '/api', endpoint: 'stsToolUpdates/publish', payload: { args: {} } });
  assert.ok(updates.some(value => value?.reason === 'published'));
  cleanups.forEach(dispose => dispose());
});

test('launcher validates local configuration and refuses a foreign listener', () => {
  assert.throws(() => validateWebConfig({ home: 'home', cli: 'cli', port: 0 }));
  assert.equal(validateWebConfig({ home: 'home', cli: 'cli', port: 3415 }).port, 3415);
  const cli = 'D:/repo/runtime/@deepseek-ai/dsh/lib/bin.js';
  assert.equal(ownedWebProcess({ name: 'node.exe', command: `node "${cli}" --profile web` }, cli, 'D:/repo'), true);
  assert.equal(ownedWebProcess({ name: 'node.exe', command: 'node runtime/@deepseek-ai/dsh/lib/bin.js --profile web' }, cli, 'D:/repo'), true);
  assert.equal(ownedWebProcess({ name: 'node.exe', command: 'node other/server.js' }, cli, 'D:/repo'), false);
  assert.equal(ownedWebProcess({ name: 'other.exe', command: cli }, cli, 'D:/repo'), false);
  assert.throws(() => validateWebConfig({ home: 'home', cli: 'cli', port: 3415, hotReload: 'true' }), /hotReload/);
  const root = join(tmpdir(), 'sts-hmr-root'), patch = JSON.parse(localHmrPatch(root));
  assert.deepEqual(patch.map(row => row.id), ['hmr']);
  assert.equal(patch[0].config.base, pathToFileURL(root + '/').href);
  assert.deepEqual(patch[0].config.root, ['tools/dsh_player', 'tools/run_play']);
});
