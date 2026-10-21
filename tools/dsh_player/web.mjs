// One local launch configuration replaces repeated environment/CLI commands.
import { readFile, writeFile, mkdir, open, realpath } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFile, spawn } from 'node:child_process';
import { promisify } from 'node:util';

const execute = promisify(execFile);
const repository = fileURLToPath(new URL('../../', import.meta.url));
export function validateWebConfig(config) {
  if (typeof config.home !== 'string' || !config.home || typeof config.cli !== 'string' || !config.cli
      || !Number.isSafeInteger(config.port) || config.port < 1 || config.port > 65535) {
    throw new Error('dsh-web.json 需要 home、cli 和有效的 port。');
  }
  if (config.hotReload !== undefined && typeof config.hotReload !== 'boolean') throw new Error('hotReload 必须为 true 或 false。');
  return config;
}
export function localHmrPatch(root) {
  return JSON.stringify([{ id: 'hmr', config: { base: pathToFileURL(resolve(root) + '/').href,
    root: ['tools/dsh_player', 'tools/run_play'] } }], null, 2) + '\n';
}
export function ownedWebProcess(process, cli, repo) {
  const normalized = value => value.replaceAll('\\', '/').toLowerCase();
  const command = normalized(process.command ?? '');
  const relative = normalized(cli).replace(normalized(repo).replace(/\/$/, '') + '/', '');
  return process.name?.toLowerCase() === 'node.exe'
    && (command.includes(normalized(cli)) || command.includes(relative));
}
async function currentServer(port) {
  const script = `$webPid = Get-NetTCPConnection -LocalPort ${port} -State Listen -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique; if ($webPid) { if (@($webPid).Count -ne 1) { throw 'Multiple server processes' }; $webOwner = Get-CimInstance Win32_Process -Filter "ProcessId=$webPid"; if ($webOwner) { @{ pid=$webOwner.ProcessId; name=$webOwner.Name; command=$webOwner.CommandLine } | ConvertTo-Json -Compress } }; exit 0`;
  const { stdout } = await execute('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true });
  return stdout.trim() ? JSON.parse(stdout) : null;
}
async function main() {
  const action = process.argv[2] ?? 'start';
  if (process.argv.length > 3 || !['start', 'restart', 'status'].includes(action)) throw new Error('用法：dsh.ps1 [start|restart|status]');
  const file = join(repository, '.oracle-lab/dsh-web.json');
  let config;
  try { config = validateWebConfig(JSON.parse(await readFile(file, 'utf8'))); }
  catch (error) {
    if (error.code === 'ENOENT') throw new Error('先在 .oracle-lab/dsh-web.json 配置本机 dsh 的 home、cli 和 port。');
    throw error;
  }
  const home = await realpath(resolve(repository, config.home)), cli = await realpath(resolve(repository, config.cli));
  const existing = await currentServer(config.port);
  if (existing && !ownedWebProcess(existing, cli, repository)) throw new Error(`${config.port} 端口被其他程序占用，未停止它。`);
  const entry = `http://127.0.0.1:${config.port}/`;
  if (action === 'status') {
    console.log(existing ? `dsh 正在运行（PID ${existing.pid}）\n${entry}` : 'dsh 未运行。'); return;
  }
  if (existing && action === 'start') { console.log(`dsh 已在运行\n${entry}`); return; }
  if (existing) {
    process.kill(existing.pid);
    for (let attempt = 0; attempt < 20 && await currentServer(config.port); attempt++) {
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    if (await currentServer(config.port)) throw new Error('旧服务还未退出，没有启动第二个服务。');
  }
  const directory = join(repository, '.oracle-lab/dsh-web'); await mkdir(directory, { recursive: true });
  const patch = join(directory, 'hmr.patch.yml');
  if (config.hotReload !== false) await writeFile(patch, localHmrPatch(repository));
  const log = join(directory, 'server.log'), handle = await open(log, 'w');
  const child = spawn(process.execPath, [cli, '--profile', 'web',
    ...(config.hotReload !== false ? ['--patch', patch] : []), '--no-open', '--port', String(config.port)], {
    cwd: repository, env: { ...process.env, DSH_HOME: home }, detached: true, windowsHide: true,
    stdio: ['ignore', handle.fd, handle.fd],
  });
  await new Promise((resolve, reject) => { child.once('spawn', resolve); child.once('error', reject); });
  child.unref(); await handle.close();
  await writeFile(join(directory, 'server.json'), JSON.stringify({ pid: child.pid, cli, home, port: config.port }, null, 2) + '\n');
  for (let attempt = 0; attempt < 120; attempt++) {
    const output = await readFile(log, 'utf8');
    const address = output.match(new RegExp(`http://127\\.0\\.0\\.1:${config.port}/\\?token=[A-Za-z0-9_-]+`))?.[0];
    if (address) { console.log(`dsh 已启动。打开下面的地址，新建聊天即可试跑：\n${address}`); return; }
    if (child.exitCode !== null || child.signalCode !== null) throw new Error(`dsh 启动时退出。请查看 ${log}`);
    await new Promise(resolve => setTimeout(resolve, 500));
  }
  throw new Error(`服务尚未报告启动完成。请查看 ${log}`);
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1; });
}
