import test from 'node:test';
import assert from 'node:assert/strict';
import { jumpTo, jumpPoint, showJumpPoint, runEngine } from './commands/navigation.mjs';
import { commandGroups } from './session.mjs';

const status = 'status session=test h104 state=27 node=nd4c0bbde96ce evidence_compatibility=current\n'
  + 'depth=247 site=Combat proof=unknown snapshot=2a34bbe1dbe8\n';
const snapshotId = 'public_information_snapshot_v1:261fc811fffdf99b68a673562f802a34bbe1dbe8';

test('the asynchronous engine captures output, launch failure and exit status without a shell', async () => {
  const result = await runEngine(process.execPath, ['-e', 'console.log(process.argv[1]); console.error("detail"); process.exitCode = 7;', 'literal $(text); spaces'], { windowsHide: true });
  assert.equal(result.stdout.trim(), 'literal $(text); spaces');
  assert.equal(result.stderr.trim(), 'detail');
  assert.equal(result.status, 7);
  await assert.rejects(runEngine('nonexistent-sts-engine-for-test', [], {}), { code: 'ENOENT' });
  const controller = new AbortController(); controller.abort();
  await assert.rejects(runEngine(process.execPath, [], { signal: controller.signal }), /abort/i);
});
function combatReading() {
  return { control: 'combat', raw: { reading: { state: { information: { snapshot_id: snapshotId } } } } };
}

test('combat return point works without a choice source and uses the Rust-issued node', async t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  const reading = combatReading();
  await showJumpPoint({ sessionLabel: 'practice',
    runEngine(binary, args) { assert.equal(args[0], 'show'); return { status: 0, stdout: status }; },
    game: { async decision(options) { assert.deepEqual(options, { details: ['brief', 'state'] }); return reading; } },
    async printCurrentReading(result) { assert.equal(result, reading); },
  });
  assert.equal(output, 'Return point (practice): ./p jump nd4c0bbde96ce\n');
  assert.equal(jumpPoint(reading, status.replace('h104', 'h109')), 'nd4c0bbde96ce');
});

test('terminal readings do not invent a recoverable target', () => {
  assert.equal(jumpPoint({ control: 'terminal' }, 'terminal status'), null);
});

test('terminal jump guidance explains retained history without restoring or inventing a node', async t => {
  let output = '';
  const calls = [];
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  await showJumpPoint({
    runEngine(binary, args) { calls.push(args[0]); return { status: 0, stdout: 'terminal status' }; },
    game: { async decision() { return { control: 'terminal' }; } },
    async printCurrentReading() {},
  });
  assert.deepEqual(calls, ['show']);
  assert.match(output, /do not require a prior jump call/);
  assert.match(output, /\.\/p back/);
  assert.doesNotMatch(output, /exe history/);
  assert.doesNotMatch(output, /Return point|jump n[0-9a-f]+\b/);
});

test('missing or changed identity fails rather than printing a return command for another state', () => {
  assert.throws(() => jumpPoint(combatReading(), 'unrecognized status'), /cannot read a return point/);
  assert.throws(() => jumpPoint({ control: 'combat' }, status), /cannot read a return point/);
  assert.throws(() => jumpPoint(combatReading(), status.replace('2a34bbe1dbe8', 'ffffffffffff')), /decision changed/);
  const reading = combatReading();
  reading.raw.reading.source = { archive_node_id: 'task_run_archive_node_v1:other', public_snapshot_id: snapshotId };
  assert.throws(() => jumpPoint(reading, status), /decision changed/);
  reading.raw.reading.source.archive_node_id = 'task_run_archive_node_v1:051ce368a1fa8eaedf5c1457a1c2d4c0bbde96ce';
  assert.equal(jumpPoint(reading, status), 'nd4c0bbde96ce');
});

test('scratch diagnostic show issues a return point while compact status is not an identity source', () => {
  const scratch = 'run-play session revision=h104 state=27 node=nd4c0bbde96ce mode=scratch\n'
    + 'depth=247 replay=0 site=Combat proof=unknown snapshot=2a34bbe1dbe8\n';
  assert.equal(jumpPoint(combatReading(), scratch), 'nd4c0bbde96ce');
  assert.throws(() => jumpPoint(combatReading(), 'Act 1 F1 | HP 68/75 | Combat\n'), /cannot read a return point/);
});

test('jump delegates its selector and session to Rust without running a shell', async t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  for (const selector of ['n0123', 'source:h12', 'target:h15']) {
    output = '';
    await jumpTo({ binary: 'run_play.exe', session: 'session with spaces', repository: 'repo',
      runEngine(binary, args, options) {
        assert.equal(binary, 'run_play.exe');
        assert.deepEqual(args, ['jump', '--session', 'session with spaces', selector]);
        assert.deepEqual(options, { cwd: 'repo', encoding: 'utf8', windowsHide: true });
        return { status: 0, stdout: 'raw engine state', stderr: '' };
      },
    }, selector);
    assert.equal(output, '');
  }
});

test('jump failures remain failures, with no success receipt or guessed retry', async t => {
  let output = '';
  t.mock.method(process.stdout, 'write', chunk => { output += chunk; return true; });
  for (const failure of [
    { status: 1, stdout: '', stderr: 'no retained public decision matches' },
    { status: 1, stdout: '', stderr: 'selector is ambiguous across 2 retained decisions' },
    { status: null, signal: 'SIGTERM', stdout: '', stderr: '' },
    { error: new Error('could not launch engine') },
  ]) {
    let calls = 0;
    await assert.rejects(jumpTo({
      runEngine() { calls++; return failure; },
    }, 'nmissing'), /no retained|ambiguous|SIGTERM|could not launch/);
    assert.equal(calls, 1);
    assert.equal(output, '');
  }
});

test('navigation is validated before a grouped action prefix could execute', () => {
  for (const [command, parameters] of [
    ['jump', ['nA', 'nB']],
    ['jump', ['nA', ',', 'act', 'end']],
    ['act', ['end', ';', 'jump', 'nA']],
    ['board', [',', 'jump']],
  ]) {
    assert.throws(() => commandGroups(command, parameters), /run jump as a separate command/);
  }
  assert.deepEqual(commandGroups('jump', []), [['jump', []]]);
  assert.deepEqual(commandGroups('jump', ['target:h4']), [['jump', ['target:h4']]]);
  assert.deepEqual(commandGroups(undefined, []), [[undefined, []]]);
  assert.deepEqual(commandGroups('act', ['end', ',', 'board']), [['act', ['end']], ['board', []]]);
});
