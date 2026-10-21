import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, resolve, sep } from 'node:path';
import { readCurrentSession, resolveInvocation, resolveNewSession, resolveSession } from './session.mjs';

async function fixture(t) {
  const base = await realpath(tmpdir());
  const repo = await mkdtemp(join(base, 'run-play-session-'));
  t.after(async () => {
    const target = await realpath(repo);
    assert.ok(target.startsWith(base + sep) && basename(target).startsWith('run-play-session-'));
    await rm(target, { recursive: true });
  });
  const sessions = join(repo, '.oracle-lab', 'run-play');
  for (const name of ['glm-playonly-20260919', 'other-run', 'other-run-next', 'map', 'jump', '0', 'Bash']) {
    await mkdir(join(sessions, name), { recursive: true });
  }
  return { repo, sessions, current: join(sessions, 'other-run') };
}

test('session routing recognizes short names with a different default and preserves command groups', async t => {
  const { repo, sessions, current } = await fixture(t);
  const selected = join(sessions, 'glm-playonly-20260919');
  const route = args => {
    const invocation = resolveInvocation(args, current, repo);
    return { ...invocation, session: resolve(invocation.session) };
  };
  for (const name of ['glm-playonly-20260919', 'GLM-PLAYONLY', selected]) {
    assert.deepEqual(route([name]), {
      session: selected, command: undefined, parameters: [],
    });
    assert.deepEqual(route([name, 'preview', 'play:Bash+@1', ',', 'board']), {
      session: selected, command: 'preview', parameters: ['play:Bash+@1', ',', 'board'],
    });
  }
});

test('commands and indices keep the default while named choices remain available', async t => {
  const { repo, sessions, current } = await fixture(t);
  const resolvedCurrent = resolveSession(current, repo);
  assert.deepEqual(resolveInvocation([], current, repo), { session: resolvedCurrent, command: undefined, parameters: [] });
  for (const command of ['map', 'preview', 'act', 'plans', 'player', 'use', 'undo', 'jump', 'refresh', 'scout', 'pick', '0', 'e3', 'Disarm', 'take', 'claim:BagOfPreparation#0', 'claim:SapphireKey#0', 'open-card#0']) {
    assert.deepEqual(resolveInvocation([command, 'x'], current, repo), {
      session: resolvedCurrent, command, parameters: ['x'],
    });
  }
  // Reserved command names require a path; a matching option name can
  // also be addressed by putting it after an explicit session path.
  assert.equal(resolveInvocation([join(sessions, 'map')], current, repo).command, undefined);
  assert.equal(resolveInvocation([join(sessions, 'jump')], current, repo).command, undefined);
  assert.equal(resolveInvocation(['Bash'], current, repo).session, join(sessions, 'Bash'));
  assert.deepEqual(resolveInvocation([current, 'Bash'], current, repo), {
    session: resolvedCurrent, command: 'Bash', parameters: [],
  });
  assert.deepEqual(resolveInvocation(['take', 'Bash'], current, repo), {
    session: resolvedCurrent, command: 'take', parameters: ['Bash'],
  });
});

test('exact session names win over substrings, ambiguous names fail without falling through to choices', async t => {
  const { repo, sessions, current } = await fixture(t);
  assert.equal(resolveSession('other-run', repo), join(sessions, 'other-run'));
  assert.throws(() => resolveInvocation(['other'], current, repo), /ambiguous session/);
  assert.throws(() => resolveInvocation(['other'], null, repo), /ambiguous session/);
  assert.throws(() => resolveInvocation(['missing'], null, repo), /no session .* matches "missing"/);
  assert.throws(() => resolveInvocation(['map'], null, repo), /no current session/);
  assert.throws(() => resolveInvocation([], null, repo), /no current session/);
  assert.equal(resolveInvocation(['glm-playonly'], null, repo).session, join(sessions, 'glm-playonly-20260919'));
});

test('new accepts a fresh short name without relaxing existing-session lookup', async t => {
  const { repo, sessions } = await fixture(t);
  assert.equal(resolveNewSession('fresh-a20h', repo), join(sessions, 'fresh-a20h'));
  assert.equal(resolve(resolveNewSession(join(sessions, 'explicit-new'), repo)),
    resolve(join(sessions, 'explicit-new')));
  assert.throws(() => resolveSession('fresh-a20h', repo), /no session/);
  assert.throws(() => resolveNewSession('bad name', repo), /must use letters/);
});

test('missing or empty current-session is awaited and does not prevent an explicit session', async t => {
  const { repo, sessions } = await fixture(t);
  assert.equal(await readCurrentSession(repo), null);
  assert.equal(resolveInvocation(['glm-playonly'], await readCurrentSession(repo), repo).session,
    join(sessions, 'glm-playonly-20260919'));
  const records = join(repo, '.oracle-lab', 'run-play-records');
  await mkdir(records);
  const file = join(records, 'current-session');
  await writeFile(file, '  \n');
  assert.equal(await readCurrentSession(repo), null);
  await writeFile(file, join(sessions, 'other-run') + '\n');
  assert.equal(await readCurrentSession(repo), join(sessions, 'other-run'));
});
