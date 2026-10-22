import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { pathToFileURL } from 'node:url';
import { Context } from '@deepseek-ai/cordis';
import { StsToolUpdates } from './updates.mjs';

async function fixture(t, { watched = true, misbased = false, linkedWorkbench = true } = {}) {
  const base = await realpath(tmpdir()), root = await mkdtemp(join(base, 'sts-tool-updates-'));
  const ctx = new Context();
  t.after(async () => {
    await ctx.fiber.dispose();
    const target = await realpath(root);
    assert.equal(dirname(target), base); assert.ok(target.split(/[\\/]/).at(-1).startsWith('sts-tool-updates-'));
    await rm(target, { recursive: true });
  });
  await mkdir(join(root, 'tools/dsh_player'), { recursive: true });
  await mkdir(join(root, 'tools/run_play'), { recursive: true });
  for (const file of ['tools/dsh_player/native.mjs', 'tools/dsh_player/map-view.mjs', 'tools/run_play/workbench.mjs']) {
    await writeFile(join(root, file), 'original');
  }
  await writeFile(join(root, 'tools/dsh_player/package.json'), '{}');
  await writeFile(join(root, 'tools/dsh_player/cordis.patch.yml'), '- insert: []\n');
  let queue = Promise.resolve(), restarts = 0, invalidImport = false;
  const entry = { options: { id: 'preset-sts-a20h' }, fiber: { restart: async () => { restarts++; } } };
  const hmr = { baseDir: misbased ? join(root, 'profile') : root, config: { root: watched
    ? misbased ? [join(root, 'tools/dsh_player'), join(root, 'tools/run_play')] : ['tools/dsh_player', 'tools/run_play'] : [] },
    getLinked: async url => linkedWorkbench && url === pathToFileURL(join(root, 'tools/dsh_player/native.mjs')).href
      ? [pathToFileURL(join(root, 'tools/run_play/workbench.mjs')).href] : [], runExclusive: operation => {
      const task = queue.then(operation); queue = task.catch(() => {}); return task;
    } };
  const resolved = [];
  for (const [name, service] of Object.entries({ hmr,
    loader: { import: async () => { if (invalidImport) throw Error('invalid dynamic module'); }, entries: () => [entry], await: async () => {} },
    agentPresets: { resolve: async id => { resolved.push(id); return { id }; } },
  })) ctx.effect(() => ctx.reflect.provide(name, service));
  const mount = async () => {
    const fiber = await ctx.plugin({ apply(child) { new StsToolUpdates(child, { root }); } });
    return { fiber, updates: ctx.stsToolUpdates };
  };
  let mounted = await mount();
  const updates = mounted.updates;
  assert.equal((await updates.status()).status, watched && !misbased && linkedWorkbench ? 'ready' : 'unavailable');
  return { root, ctx, updates, entry, resolved, restarts: () => restarts, invalid: value => { invalidImport = value; },
    remount: async () => { await mounted.fiber.dispose(); mounted = await mount(); return mounted.updates; },
    change: async value => writeFile(join(root, 'tools/run_play/workbench.mjs'), value) };
}

test('publication waits for official source processing and serializes repeated requests', async t => {
  const f = await fixture(t);
  await f.change('new');
  assert.equal((await f.updates.publish()).status, 'loading'); assert.equal(f.restarts(), 0);
  f.ctx.emit('hmr/reload', new Map());
  assert.equal((await f.updates.status()).status, 'available');
  const results = await Promise.all([f.updates.publish(), f.updates.publish()]);
  assert.ok(results.every(result => result.status === 'ready'));
  assert.equal(results[0].revision, results[1].revision); assert.equal(f.restarts(), 1);
  assert.deepEqual(f.resolved, ['sts-a20h-manual', 'sts-a20h']);
});

test('invalid dynamic code is checked before retiring the preset and can recover through hmr/change', async t => {
  const f = await fixture(t);
  await f.change('invalid'); f.invalid(true); f.ctx.emit('hmr/reload', new Map());
  const failed = await f.updates.publish();
  assert.equal(failed.status, 'failed'); assert.equal(failed.reason, 'failed');
  assert.equal(failed.error, 'invalid dynamic module'); assert.ok(!('message' in failed)); assert.equal(f.restarts(), 0);
  await f.change('fixed'); f.invalid(false);
  f.ctx.emit('hmr/change', pathToFileURL(join(f.root, 'tools/run_play/workbench.mjs')).href);
  assert.equal((await f.updates.publish()).status, 'ready'); assert.equal(f.restarts(), 1);
});

test('unpublished revision and client metadata boundary survive a Remote service replacement', async t => {
  const f = await fixture(t), original = (await f.updates.status()).revision;
  await f.change('new');
  const replacement = await f.remount();
  assert.equal((await replacement.status()).status, 'available');
  assert.equal((await replacement.status()).revision, original);
  await writeFile(join(f.root, 'tools/dsh_player/package.json'), '{"new":true}');
  const another = await f.remount();
  assert.equal((await another.publish()).status, 'restart-required'); assert.equal(f.restarts(), 0);
});

test('disabled source watches do not cause a pretend successful update', async t => {
  const f = await fixture(t, { watched: false });
  assert.equal((await f.updates.publish()).status, 'unavailable'); assert.equal(f.restarts(), 0);
});

test('preset declaration changes require a service reload before publishing code', async t => {
  const f = await fixture(t);
  await writeFile(join(f.root, 'tools/dsh_player/cordis.patch.yml'), '- insert: [{id: changed-preset}]\n');
  const replacement = await f.remount();
  assert.equal((await replacement.status()).status, 'restart-required');
  assert.equal((await replacement.publish()).status, 'restart-required');
  assert.equal(f.restarts(), 0);
});

test('disk revisions cannot publish a workbench outside the native static module tree', async t => {
  const f = await fixture(t, { linkedWorkbench: false });
  assert.equal((await f.updates.status()).status, 'unavailable');
  assert.equal((await f.updates.status()).reason, 'loadFailed');
  assert.match((await f.updates.status('en')).error, /static module tree does not include the shared workbench/);
  assert.match((await f.updates.status('zh-CN')).error, /静态模块树没有包含共享工作台/);
  assert.equal((await f.updates.status('en')).reason, (await f.updates.status('zh-CN')).reason);
  assert.equal((await f.updates.publish()).status, 'unavailable');
  assert.equal(f.restarts(), 0);
});

test('absolute watch roots outside the HMR base are not reported as working source watches', async t => {
  const f = await fixture(t, { misbased: true });
  assert.equal((await f.updates.publish()).status, 'unavailable'); assert.equal(f.restarts(), 0);
});

test('update diagnostics localize owned failures and preserve dynamic loader errors across languages', async t => {
  const missing = await fixture(t);
  missing.ctx.loader.entries = () => [];
  for (const locale of ['en', 'zh-CN']) {
    await assert.rejects(missing.updates.status(locale), locale === 'en'
      ? /no unique Slay the Spire preset declaration/ : /没有唯一的杀戮尖塔预设声明/);
  }
  assert.equal(missing.restarts(), 0);

  const dynamic = await fixture(t), raw = 'Loader {{original}} 原文 🐈';
  dynamic.ctx.loader.import = async () => { throw new Error(raw); };
  await dynamic.change('changed'); dynamic.ctx.emit('hmr/reload', new Map());
  const failed = await dynamic.updates.publish('zh-CN');
  assert.equal(failed.error, raw);
  assert.equal((await dynamic.updates.status('en')).error, raw);
  assert.equal((await dynamic.updates.status('zh-CN')).error, raw);
  assert.equal(dynamic.restarts(), 0);
});
