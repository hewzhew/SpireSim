import test from 'node:test';
import assert from 'node:assert/strict';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {validateWebConfig,ownedWebProcess,localHmrPatch,parseWebArguments} from './web.mjs';

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

test('web argument languages leave actions HMR and configuration unchanged', () => {
  assert.deepEqual(parseWebArguments([]),{action:'start',locale:'zh-CN',help:false});
  const config={home:'local home',cli:'local cli',port:3415,hotReload:false};
  const saved=JSON.stringify(config);
  for(const locale of ['en','zh-CN']) {
    for(const action of ['start','restart','status']) {
      assert.deepEqual(parseWebArguments([action,'--locale',locale]),{action,locale,help:false});
      assert.deepEqual(parseWebArguments(['--locale='+locale,action,'-h']),{action,locale,help:true});
    }
    assert.equal(validateWebConfig(config,locale),config);
    assert.throws(()=>validateWebConfig({...config,port:0},locale),locale==='en'?/requires home, cli and a valid port/:/需要 home、cli 和有效的 port/);
    assert.throws(()=>validateWebConfig({...config,hotReload:1},locale),locale==='en'?/must be true or false/:/必须为 true 或 false/);
    for(const args of [['other'],['start','status'],['--unknown']])
      assert.throws(()=>parseWebArguments(['--locale',locale,...args]),locale==='en'?/^Error: Usage:/:/^Error: 用法：/);
  }
  assert.equal(JSON.stringify(config),saved);
  assert.throws(()=>parseWebArguments(['--locale','fr']),/Unsupported text locale/);
});

test('both web help entry points exit before reading configuration or controlling a service', async () => {
  const run=promisify(execFile),entry=fileURLToPath(new URL('./web.mjs',import.meta.url));
  const script=fileURLToPath(new URL('../../scripts/dsh.ps1',import.meta.url));
  for(const locale of ['en','zh-CN']) {
    const expected=locale==='en'?/^Usage:/:/^用法：/;
    const direct=await run(process.execPath,[entry,'restart','--locale',locale,'--help'],{windowsHide:true});
    assert.match(direct.stdout,expected);assert.equal(direct.stderr,'');
    if(process.platform==='win32') {
      const wrapper=await run('pwsh',['-NoProfile','-File',script,'restart','-Locale',locale,'-Help'],{windowsHide:true});
      assert.equal(wrapper.stdout,direct.stdout);assert.equal(wrapper.stderr,'');
    }
  }
});
