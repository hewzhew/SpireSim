import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { copyFile, mkdir, mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = fileURLToPath(new URL('../', import.meta.url));

async function fixture(t) {
  const temporary = await realpath(tmpdir());
  const root = await mkdtemp(join(temporary, 'sts shortcuts-'));
  t.after(async () => {
    const target = await realpath(root);
    assert.ok(target.startsWith(temporary + sep) && basename(target).startsWith('sts shortcuts-'));
    await rm(target, { recursive: true });
  });
  await mkdir(join(root, 'scripts'));
  for (const name of ['setup-local.ps1', 'py.ps1', 'p.mjs']) {
    await copyFile(join(repository, 'scripts', name), join(root, 'scripts', name));
  }
  const setup = () => spawnSync('pwsh', ['-NoProfile', '-File', join(root, 'scripts', 'setup-local.ps1')],
    { cwd: root, windowsHide: true, encoding: 'utf8' });
  return { root, setup };
}

test('generated Python shortcut preserves live UTF-8 native input and argument boundaries', { timeout: 15000 }, async t => {
  const { root, setup } = await fixture(t);
  const installed = setup();
  assert.equal(installed.status, 0, installed.stderr);
  const refreshed = setup();
  assert.equal(refreshed.status, 0, refreshed.stderr);
  const interpreter = spawnSync('pwsh', ['-NoProfile', '-File', join(repository, 'scripts', 'py.ps1'),
    '-c', 'import sys; print(sys.executable)'], { cwd: repository, encoding: 'utf8', windowsHide: true });
  assert.equal(interpreter.status, 0, interpreter.stderr);
  const code = 'import json,sys; print(json.dumps(sys.argv[1:],ensure_ascii=False),flush=True); print(input(),flush=True); raise SystemExit(7)';
  const child = spawn('pwsh', ['-NoProfile', '-File', join(root, 'py.ps1'), '-Python', interpreter.stdout.trim(),
    '-u', '-c', code, '--character', '有 空格'], { cwd: root, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  t.after(() => {
    if (child.exitCode !== null) return;
    if (process.platform === 'win32') {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
    } else child.kill();
  });
  let stderr = '';
  child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
  const completed = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', code => resolve({ code, stderr }));
  });
  const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  assert.deepEqual(JSON.parse((await lines.next()).value), ['--character', '有 空格']);
  child.stdin.write('你好，实时输入\n');
  assert.equal((await lines.next()).value, '你好，实时输入');
  assert.deepEqual(await completed, { code: 7, stderr: '' });
});

test('shortcut setup preserves unrelated existing files before writing any shortcut', async t => {
  const { root, setup } = await fixture(t);
  const local = join(root, 'py.ps1');
  await writeFile(local, '# My own local script\n');
  const result = setup();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Preserving existing file/);
  assert.equal(await readFile(local, 'utf8'), '# My own local script\n');
  await assert.rejects(readFile(join(root, 'p')), { code: 'ENOENT' });
});
