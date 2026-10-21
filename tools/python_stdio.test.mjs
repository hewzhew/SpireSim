import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { mkdtemp, writeFile, unlink, rmdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const repository = fileURLToPath(new URL('../', import.meta.url));
const wrapper = fileURLToPath(new URL('../scripts/py.ps1', import.meta.url));
const quote = (value) => `'${value.replaceAll("'", "''")}'`;

function worker(args, t) {
  const child = spawn('pwsh', ['-NoProfile', ...args], {
    cwd: repository, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
  });
  let stderr = '';
  child.stderr.setEncoding('utf8').on('data', (value) => { stderr += value; });
  const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  const completed = new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', (code) => resolve({ code, stderr }));
  });
  t.after(() => {
    if (child.exitCode !== null) return;
    if (process.platform === 'win32') {
      spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
    } else child.kill();
  });
  return { child, lines, completed };
}

test('native pipes allow Python to answer before receiving input or EOF', { timeout: 12000 }, async (t) => {
  const code = 'import json; print(json.dumps({"ready": True}), flush=True); print(input(), flush=True)';
  const session = worker(['-File', wrapper, '-u', '-c', code], t);
  assert.deepEqual(JSON.parse((await session.lines.next()).value), { ready: true });
  session.child.stdin.write('你好，逐行交互\n');
  assert.equal((await session.lines.next()).value, '你好，逐行交互');
  session.child.stdin.end();
  assert.equal((await session.completed).code, 0);
});

test('explicit PowerShell pipeline still forwards UTF-8 input', { timeout: 12000 }, async (t) => {
  const code = 'import json,sys; print(json.dumps(sys.stdin.read(), ensure_ascii=False), flush=True)';
  const command = `${quote('你好，管道')} | & ${quote(wrapper)} -u -c ${quote(code)}`;
  const session = worker(['-Command', command], t);
  session.child.stdin.end();
  assert.equal(JSON.parse((await session.lines.next()).value).trimEnd(), '你好，管道');
  assert.equal((await session.completed).code, 0);
});

test('Python can exit while the parent native stdin is still open', { timeout: 12000 }, async (t) => {
  const session = worker(['-File', wrapper, '-c', 'raise SystemExit(7)'], t);
  assert.equal((await session.completed).code, 7);
});

test('script-file workers receive arguments and keep native JSONL input live', { timeout: 12000 }, async (t) => {
  const directory = await mkdtemp(join(tmpdir(), 'sts-python-stdio-'));
  const script = join(directory, 'worker.py');
  await writeFile(script, 'import json,sys\nprint(json.dumps(sys.argv[1:]), flush=True)\nfor line in sys.stdin:\n print(line.strip(), flush=True)\n');
  t.after(async () => { await unlink(script); await rmdir(directory); });
  const session = worker(['-File', wrapper, '-u', script, '--character', 'watcher'], t);
  assert.deepEqual(JSON.parse((await session.lines.next()).value), ['--character', 'watcher']);
  session.child.stdin.write('{"你好": 2}\n');
  assert.deepEqual(JSON.parse((await session.lines.next()).value), { '你好': 2 });
  session.child.stdin.end();
  assert.equal((await session.completed).code, 0);
});
