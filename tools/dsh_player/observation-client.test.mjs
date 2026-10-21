import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { runInNewContext } from 'node:vm';

const walk = node => !node || typeof node !== 'object' ? [] : Array.isArray(node) ? node.flatMap(walk) : [node, ...node.children.flatMap(walk)];
const text = node => node == null ? '' : Array.isArray(node) ? node.map(text).join('') : typeof node === 'object' ? text(node.children) : String(node);
const button = (tree, label) => walk(tree).find(node => node.type === 'button' && (node.children.includes(label) || node.props['aria-label'] === label));
const row = (tree, step) => walk(tree).find(node => node.type === 'button' && node.props['aria-label']?.startsWith('第 ' + step + ' 步 '));
const item = (step, command, outcome = 'completed') => ({ step, seq: step * 10, revision: step * 10 + 9,
  label: command, outcome, durationMs: 100, usage: null, calls: [{ name: 'sts', command, outcome }] });

async function fixture({ usage = null } = {}) {
  let definition, panel, cursor = 0, effectCursor = 0, tick = 1;
  const states = [], dependencies = [], effects = [], cleanups = [], calls = [], copies = [];
  const react = { Fragment: 'fragment', createElement: (type, props, ...children) => ({ type, props, children }),
    useState(initial) { const index = cursor++; if (!(index in states)) states[index] = initial;
      return [states[index], value => { states[index] = value; }]; },
    useReducer() { cursor++; return [0, () => {}]; },
    useRef(initial) { const index = cursor++; if (!(index in states)) states[index] = { current: initial }; return states[index]; },
    useEffect(effect, deps) { const index = effectCursor++;
      if (!dependencies[index] || deps.some((value, i) => value !== dependencies[index][i])) effects.push({ effect, index });
      dependencies[index] = deps; },
  };
  runInNewContext(await readFile(new URL('./client.js', import.meta.url), 'utf8'), {
    window: { __ModuleLoader__: { load: value => { definition = value; } } }, AbortController,
    navigator: { clipboard: { writeText: async value => copies.push(value) } },
  });
  let overview = { status: 'ready', turn: 1, turns: [1], phase: 'completed',
    timeline: [item(1, 'deck', 'error'), { ...item(2, 'read'), usage }], gameCalls: 2, errors: 1, pending: 0, accounting: null,
    control: { paused: false, running: false, queued: 0 } };
  const identity = revision => ({ head_revision: revision, state_revision: revision, archive_node_id: 'n' + revision, public_snapshot_id: 's' + revision });
  const point = (revision, origins) => ({ identity: identity(revision), origins,
    location: { kind: 'strategic', act: 1, floor: revision, site: 'map' }, resources: { hp: 65, max_hp: 75, gold: 30, potions: [null, null] } });
  let points = { schema: 'run-play-return-points-v1', source: { session_path: 'selected-game', session_id: 'same', identity: identity(7) },
    current: point(7, []), targets: [point(6, [{ kind: 'floor', floor: 6 }])] }, failRestore = false;
  const ctx = { effect: fn => fn(), sidebarRightTabs: { register: () => () => {} },
    slots: { inject: (_name, fn) => fn(), register: (entry, component) => { if (entry.key === 'sts-observe') panel = component; return () => {}; } },
    connection: { rpc: { call: async (channel, endpoint, payload) => {
      const args = JSON.parse(JSON.stringify(payload)).args; calls.push({ channel, endpoint, args });
      if (args.sessionId !== 'selected-chat') return { ok: true, value: { status: 'not-started', message: 'This chat has no game.' } };
      if (endpoint === 'stsObservation/read') return { ok: true, value: overview };
      if (endpoint === 'stsObservation/step') return { ok: true, value: { status: 'ready', turn: 1, step: args.step, throughSeq: 1000, messages: [],
        calls: [{ name: 'sts', command: args.step === 1 ? 'deck' : 'read', outcome: args.step === 1 ? 'error' : 'completed', text: 'Full return ' + args.step }],
        ...(args.input ? { inputs: [] } : {}) } };
      if (endpoint === 'stsObservation/live') return { ok: true, value: { status: 'ready', text: 'Actual inventory' } };
      if (endpoint === 'stsObservation/points') return { ok: true, value: { status: 'ready', points, control: overview.control } };
      if (endpoint === 'stsObservation/pause' || endpoint === 'stsObservation/restore') {
        overview = { ...overview, control: { paused: true, running: false, queued: 1 } };
        if (endpoint.endsWith('/restore')) {
          if (failRestore) return { ok: true, value: { status: 'error', message: 'stale source', control: overview.control } };
          const from = points.current.identity, to = args.request.target;
          const receipt = { schema: 'run-play-return-receipt-v1', from, to, previous_endpoint: from };
          points = { ...points, source: { ...points.source, identity: to }, current: point(to.state_revision, []),
            targets: [point(from.state_revision, [{ kind: 'previous_endpoint' }])] };
          return { ok: true, value: { status: 'ready', receipt, points, text: 'Actual returned inventory', control: overview.control } };
        }
        return { ok: true, value: { status: 'ready', points, text: 'Actual paused inventory', control: overview.control } };
      }
      if (endpoint === 'stsObservation/resume') {
        overview = { ...overview, control: { paused: false, running: true, queued: 0 } };
        return { ok: true, value: { status: 'ready', text: 'Actual resumed inventory', control: overview.control } };
      }
      assert.fail('Unexpected endpoint ' + endpoint);
    } } } };
  definition.factory(() => react).apply(ctx);
  return { calls, copies, failRestore() { failRestore = true; },
    render(sessionId = 'selected-chat') { cursor = 0; effectCursor = 0; return panel({ sessionId, useProjection: () => tick }); },
    async settle() { effects.splice(0).forEach(({ effect, index }) => { cleanups[index]?.(); cleanups[index] = effect(); }); await Promise.resolve(); await Promise.resolve(); },
    advance() { overview = { ...overview, timeline: [...overview.timeline, item(3, 'inventory')], gameCalls: 3 }; tick++; },
  };
}

test('native observation uses named args, loads and reuses input on demand, and hides another chat’s records', async () => {
  const f = await fixture();
  f.render(); await f.settle();
  assert.deepEqual(f.calls, [{ channel: '/api', endpoint: 'stsObservation/read', args: { sessionId: 'selected-chat', turn: null } }]);
  f.render(); await f.settle();
  assert.deepEqual(f.calls.at(-1).args, { sessionId: 'selected-chat', turn: 1, step: 2, input: false });
  button(f.render(), '输入').props.onClick(); f.render(); await f.settle();
  assert.deepEqual(f.calls.at(-1).args, { sessionId: 'selected-chat', turn: 1, step: 2, input: true });
  assert.match(text(f.render()), /本步没有保存请求边界/);
  const count = f.calls.length;
  button(f.render(), '返回').props.onClick(); f.render(); await f.settle();
  button(f.render(), '输入').props.onClick(); f.render(); await f.settle();
  assert.equal(f.calls.length, count, 'switching content does not re-fetch an already loaded input');
  button(f.render(), '当前游戏').props.onClick(); f.render(); await f.settle();
  assert.deepEqual(f.calls.at(-1), { channel: '/api', endpoint: 'stsObservation/live', args: { sessionId: 'selected-chat' } });
  assert.match(text(f.render()), /Actual inventory/);
  await button(f.render(), '复制').props.onClick(); assert.deepEqual(f.copies, ['Actual inventory']);
  f.render('another-chat'); await f.settle();
  const changed = text(f.render('another-chat'));
  assert.match(changed, /This chat has no game/); assert.doesNotMatch(changed, /Full return|Actual inventory/);
});

test('step usage separates total input, cache reads and uncached input without calling a model', async () => {
  const f = await fixture({ usage: { inputTokens: 282, cacheReadTokens: 45184, cacheWriteTokens: 0, outputTokens: 122 } });
  f.render(); await f.settle(); f.render(); await f.settle();
  const usage = walk(f.render()).find(node => node.props?.['aria-label'] === '本步最后一次报告的用量');
  assert.equal(text(usage), '总输入 45,466未缓存 282缓存读取 45,184输出 122');
  assert.ok(f.calls.every(call => ['stsObservation/read', 'stsObservation/step'].includes(call.endpoint)));
  button(f.render(), '当前游戏').props.onClick(); f.render(); await f.settle();
  assert.equal(walk(f.render()).some(node => node.props?.['aria-label'] === '本步最后一次报告的用量'), false);
  const missing = await fixture({ usage: { inputTokens: 8, outputTokens: 3 } });
  missing.render(); await missing.settle(); missing.render(); await missing.settle();
  assert.match(text(missing.render()), /总输入 未报告/);
  assert.match(text(missing.render()), /缓存读取 未报告/);
  const reportedTotal = await fixture({ usage: { inputTokens: 8, outputTokens: 3, totalTokens: 31 } });
  reportedTotal.render(); await reportedTotal.settle(); reportedTotal.render(); await reportedTotal.settle();
  assert.match(text(reportedTotal.render()), /总输入 28未缓存 8缓存读取 未报告输出 3/);
  const cacheWrite = await fixture({ usage: { inputTokens: 8, cacheReadTokens: 10, cacheWriteTokens: 20, outputTokens: 3 } });
  cacheWrite.render(); await cacheWrite.settle(); cacheWrite.render(); await cacheWrite.settle();
  assert.match(text(cacheWrite.render()), /总输入 38未缓存 8缓存读取 10缓存写入 20输出 3/);
});

test('workspace reads exact return points, submits one source-bound restore, stays paused and exposes the previous endpoint', async () => {
  const f = await fixture(); f.render(); await f.settle(); f.render(); await f.settle();
  button(f.render(), '返回点').props.onClick(); f.render(); await f.settle();
  assert.match(text(f.render()), /第 1 幕 F6 地图/);
  assert.equal(f.calls.filter(call => call.endpoint.endsWith('/restore')).length, 0);
  const start = button(f.render(), '返回这里').props.onClick;
  const first = start(), duplicate = start(); await Promise.all([first, duplicate]);
  const restore = f.calls.filter(call => call.endpoint.endsWith('/restore'));
  assert.equal(restore.length, 1);
  assert.deepEqual(restore[0].args, { sessionId: 'selected-chat', request: { schema: 'run-play-return-request-v1',
    source: { session_path: 'selected-game', session_id: 'same', identity: { head_revision: 7, state_revision: 7, archive_node_id: 'n7', public_snapshot_id: 's7' } },
    target: { head_revision: 6, state_revision: 6, archive_node_id: 'n6', public_snapshot_id: 's6' } } });
  assert.match(text(f.render()), /已暂停/); assert.ok(button(f.render(), '继续运行')); assert.ok(button(f.render(), '回到刚才'));
  assert.equal(f.calls.some(call => call.endpoint.endsWith('/resume')), false);
  await button(f.render(), '回到刚才').props.onClick();
  assert.equal(f.calls.at(-1).args.request.target.state_revision, 7);
  await button(f.render(), '继续运行').props.onClick(); assert.equal(f.calls.at(-1).endpoint, 'stsObservation/resume');
  f.render('another-chat'); await f.settle();
  assert.doesNotMatch(text(f.render('another-chat')), /Actual returned inventory|已暂停|回到刚才/);
});

test('rejected return removes actionable old points and shows the actual error while retaining pause', async () => {
  const f = await fixture(); f.render(); await f.settle(); f.render(); await f.settle();
  button(f.render(), '返回点').props.onClick(); f.render(); await f.settle();
  f.failRestore(); await button(f.render(), '返回这里').props.onClick();
  assert.match(text(f.render()), /stale source/); assert.equal(button(f.render(), '返回这里'), undefined);
  assert.ok(button(f.render(), '继续运行')); assert.ok(button(f.render(), '重新读取'));
});

test('command search and error filtering keep inspected history pinned while new steps arrive', async () => {
  const f = await fixture();
  f.render(); await f.settle(); f.render(); await f.settle();
  button(f.render(), '仅报错').props.onClick();
  assert.ok(row(f.render(), 1)); assert.equal(row(f.render(), 2), undefined);
  row(f.render(), 1).props.onClick(); f.render(); await f.settle();
  assert.match(text(f.render()), /Full return 1/);
  assert.doesNotMatch(text(f.render()), /部分动作可能已经完成/, 'an error alone is not evidence of an applied action');
  const before = f.calls.filter(call => call.endpoint === 'stsObservation/step').length;
  f.advance(); f.render(); await f.settle(); f.render(); await f.settle();
  assert.match(text(f.render()), /Full return 1/);
  assert.equal(f.calls.filter(call => call.endpoint === 'stsObservation/step').length, before);
  button(f.render(), '回到最新').props.onClick(); f.render(); await f.settle();
  assert.equal(f.calls.at(-1).args.step, 3); assert.match(text(f.render()), /Full return 3/);
  walk(f.render()).find(node => node.type === 'input').props.onChange({ target: { value: 'DECK' } });
  assert.ok(row(f.render(), 1)); assert.equal(row(f.render(), 2), undefined); assert.equal(row(f.render(), 3), undefined);
});
