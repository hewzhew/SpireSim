#!/usr/bin/env node
// npm owns file selection and dependency installation. Distribution packages
// use materialized dependencies, while development keeps its live file links.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { access, copyFile, lstat, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';

const run = promisify(execFile);
const repository = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const sha = value => createHash('sha256').update(value).digest('hex');
function within(root, target) {
  const path = relative(root, target);
  if (!path || path === '..' || path.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`) || isAbsolute(path)) {
    throw new Error(`Path must stay inside ${root}: ${target}`);
  }
}

if (process.argv.slice(2).join(' ') === '--check') {
  const directory = process.cwd();
  const manifest = JSON.parse(await readFile(join(directory, 'package.json'), 'utf8'));
  for (const name of manifest.bundledDependencies ?? []) {
    if ((await lstat(join(directory, 'node_modules', ...name.split('/')))).isSymbolicLink()) {
      throw new Error('npm cannot safely bundle these development file links. Use npm run pack:distribution -- --output <new directory>.');
    }
  }
} else {
  const args = process.argv.slice(2);
  if (args.length !== 2 || args[0] !== '--output' || !isAbsolute(args[1])) throw new Error('pack_node.mjs --output <new absolute directory>');
  const output = resolve(args[1]);
  await mkdir(output); // A new build never mixes with an earlier package set.
  const stageRoot = join(output, 'stage');
  within(output, stageRoot);
  await mkdir(stageRoot);
  const npm = process.env.npm_execpath ?? join(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js');
  await access(npm);
  const revision = (await run('git', ['rev-parse', 'HEAD'], { cwd: repository, windowsHide: true })).stdout.trim();
  const sourceDirty = Boolean((await run('git', ['status', '--porcelain'], { cwd: repository, windowsHide: true })).stdout.trim());
  const report = { schema: 'sts-node-distribution-v1', source_revision: revision, source_dirty: sourceDirty,
    producer_sha256: sha(await readFile(fileURLToPath(import.meta.url))), packages: [], source_files_sha256: {}, installed_reading_checks: [], installed_native_checks: [], installed_scripted_checks: [], remote_modified: false };
  let commandIndex = 0;
  async function npmCommand(cwd, parameters) {
    const log = join(output, `npm-${String(commandIndex++).padStart(2, '0')}.txt`);
    try {
      const result = await run(process.execPath, [npm, ...parameters], { cwd, windowsHide: true, maxBuffer: 32 * 1024 * 1024 });
      await writeFile(log, result.stdout + result.stderr);
      return result.stdout;
    } catch (error) {
      await writeFile(log, (error.stdout ?? '') + (error.stderr ?? ''));
      throw error;
    }
  }
  const artifacts = new Map();
  try {
    await npmCommand(join(repository, 'tools/dsh_player'), ['run', 'build:client']);
    for (const directoryName of ['presentation', 'run_play', 'dsh_player']) {
      const source = join(repository, 'tools', directoryName);
      const manifest = JSON.parse(await readFile(join(source, 'package.json'), 'utf8'));
      const [listed] = JSON.parse(await npmCommand(source, ['pack', '--dry-run', '--json', '--ignore-scripts']));
      const stage = join(stageRoot, directoryName);
      await mkdir(stage);
      // Select only this package's files, from npm's own packlist. Linked
      // dependency entries are installed afresh below, never copied from '..'.
      for (const entry of listed.files) {
        const parts = entry.path.split(/[\\/]/);
        if (parts.includes('..') || parts[0] === 'node_modules') continue;
        const from = resolve(source, entry.path), to = resolve(stage, entry.path);
        within(source, from); within(stage, to);
        await mkdir(dirname(to), { recursive: true });
        await copyFile(from, to);
        report.source_files_sha256[`${directoryName}/${entry.path}`] = sha(await readFile(from));
      }
      const installing = structuredClone(manifest);
      delete installing.devDependencies;
      delete installing.scripts;
      // Root bundle declarations make npm treat the incoming private packages
      // as already bundled during install. They belong to the final pack step.
      delete installing.bundledDependencies;
      delete installing.bundleDependencies;
      for (const [name, artifact] of artifacts) {
        if (name in (installing.dependencies ?? {})) installing.dependencies[name] = `file:${artifact.path.replaceAll('\\', '/')}`;
      }
      await writeFile(join(stage, 'package.json'), JSON.stringify(installing, null, 2) + '\n');
      try {
        const lock = await readFile(join(source, 'package-lock.json'));
        report.source_files_sha256[`${directoryName}/package-lock.json`] = sha(lock);
        const installedLock = JSON.parse(lock);
        // Preserve third-party pins, replacing only the development-link
        // entries whose packages are now supplied by the preceding tarballs.
        installedLock.packages[''].dependencies = installing.dependencies ?? {};
        delete installedLock.packages[''].devDependencies;
        delete installedLock.packages[''].bundleDependencies;
        delete installedLock.packages[''].bundledDependencies;
        for (const name of artifacts.keys()) {
          delete installedLock.packages[`node_modules/${name}`];
        }
        for (const path of Object.keys(installedLock.packages)) {
          if (path.split('/').includes('..')) delete installedLock.packages[path];
        }
        await writeFile(join(stage, 'package-lock.json'), JSON.stringify(installedLock, null, 2) + '\n');
      } catch (error) { if (error.code !== 'ENOENT') throw error; }
      await npmCommand(stage, ['install', '--ignore-scripts', '--omit=dev', '--install-links=true', '--legacy-peer-deps', '--no-audit', '--no-fund']);
      await copyFile(join(stage, 'package-lock.json'), join(output, `lock-${directoryName}.json`));
      const packaged = structuredClone(manifest);
      delete packaged.scripts?.prepack;
      delete packaged.scripts?.['pack:distribution'];
      for (const [name, artifact] of artifacts) {
        if (name in (packaged.dependencies ?? {})) packaged.dependencies[name] = artifact.version;
      }
      await writeFile(join(stage, 'package.json'), JSON.stringify(packaged, null, 2) + '\n');
      const [packed] = JSON.parse(await npmCommand(stage, ['pack', '--ignore-scripts', '--json', '--pack-destination', output]));
      if (packed.files.some(entry => entry.path.split(/[\\/]/).includes('..'))) throw new Error(`Unsafe npm bundle entry: ${packed.name}`);
      const path = join(output, packed.filename);
      artifacts.set(manifest.name, { path, version: manifest.version });
      report.packages.push({ name: manifest.name, version: manifest.version, filename: packed.filename, sha256: sha(await readFile(path)), bytes: packed.size });
      if (directoryName !== 'presentation') {
        const verification = join(stageRoot, `verify-${directoryName}`);
        await mkdir(verification);
        await writeFile(join(verification, 'package.json'), '{"private":true,"type":"module"}\n');
        const hostPeers = Object.entries(manifest.peerDependencies ?? {}).map(([name, version]) => `${name}@${version}`);
        await npmCommand(verification, ['install', '--offline', '--ignore-scripts', '--no-audit', '--no-fund', path, ...hostPeers]);
        const installed = join(verification, 'node_modules', ...manifest.name.split('/'));
        const workbench = directoryName === 'run_play' ? installed : join(installed, 'node_modules/@sts-simulator/run-play');
        const renderer = join(workbench, 'render.mjs');
        const dependency = createRequire(renderer).resolve('@sts-simulator/presentation');
        within(verification, await realpath(dependency));
        const { printReading } = await import(pathToFileURL(renderer).href);
        for (const locale of ['en', 'zh-CN']) {
          let text = '';
          await printReading({ view: 'Original recorded text\n' }, { locale, combatCommands: ['end'], write(chunk) { text += chunk; } });
          if (!text.startsWith('Original recorded text\n') || !text.includes('./p act end') || !text.includes(locale === 'en' ? 'Playable now' : '当前可打出')) {
            throw new Error(`Installed reading did not retain language and command: ${manifest.name}/${locale}`);
          }
          report.installed_reading_checks.push({ package: manifest.name, locale, preserved_record_and_command: true });
        }
        if (directoryName === 'dsh_player') {
          const entry = join(installed, 'native.mjs');
          within(verification, await realpath(createRequire(entry).resolve('@sts-simulator/presentation')));
          const { nativeGuide } = await import(pathToFileURL(entry).href);
          const { renderNativeObservation } = await import(pathToFileURL(join(installed, 'native-inspection.mjs')).href);
          const { StsObservationView } = await import(pathToFileURL(join(installed, 'observation-view.mjs')).href);
          const { playerGuide, apply: applyScripted } = await import(pathToFileURL(join(installed, 'index.mjs')).href);
          const { continuationTask, runPlayer } = await import(pathToFileURL(join(installed, 'launch.mjs')).href);
          const { preparationInstructions, preparationStageGuide } = await import(pathToFileURL(join(installed, 'preparation.mjs')).href);
          const { cardChoiceInstructions, preparationCardChoiceRoute } = await import(pathToFileURL(join(installed, 'card-choice.mjs')).href);
          const { shopInstructions, renderShopBudget } = await import(pathToFileURL(join(installed, 'shop.mjs')).href);
          const { playedCardLifecycle, strengthTiming } = await import(pathToFileURL(join(installed, 'deck.mjs')).href);
          const { playerPauseText } = await import(pathToFileURL(join(installed, 'supervision.mjs')).href);
          const { renderInspection } = await import(pathToFileURL(join(installed, 'inspection.mjs')).href);
          for (const locale of ['en', 'zh-CN']) {
            const manual = nativeGuide(true, locale), assisted = nativeGuide(false, locale);
            if (!manual.includes(locale === 'en' ? 'public-information manual play' : '依据公开信息手动出牌')
                || manual.includes('research whole_combat') || !assisted.includes('research whole_combat 8000 work 200000')) {
              throw new Error(`Installed native guide lost its language or play mode: ${locale}`);
            }
            const nativeInspection = renderNativeObservation({ binding: { agent: 'offline-agent', session: 'offline-game' }, title: 'Opaque {{title}}',
              status: 'empty', steps: 0, gameCalls: 0, errors: [], pending: [], intervening: [], accounting: null }, locale);
            if (!nativeInspection.includes(locale === 'en' ? 'Token usage was not reported.' : '未报告 token 用量。')
                || !nativeInspection.includes('Opaque {{title}}') || !nativeInspection.includes('observe N input')) {
              throw new Error(`Installed native inspection lost language, record or command: ${locale}`);
            }
            let observationError;
            try { await StsObservationView.prototype.journal.call({}, '', locale); }
            catch (error) { observationError = error.message; }
            if (observationError !== (locale === 'en' ? 'Select the current session.' : '需要指定当前会话。')) {
              throw new Error(`Installed observation service lost operator language: ${locale}`);
            }
            report.installed_native_checks.push({ package: manifest.name, locale, imported_entry: true, preserved_mode_and_commands: true, inspection_operator_language: true, observation_service_operator_language: true });
            const guide = playerGuide(locale), task = 'Opaque {{operator}}\nact play:Strike@0';
            if (!guide.includes(locale === 'en' ? 'Bare info expands the current options' : '无参数 info 展开当前选项')
                || !guide.includes('research whole_combat 8000 work 200000')
                || !continuationTask(task, 'interrupted', locale).endsWith('\n\n' + task)) {
              throw new Error(`Installed scripted guide lost language, command or operator text: ${locale}`);
            }
            let runtimeError;
            try { await applyScripted({}, { agentLocale: locale }); }
            catch (error) { runtimeError = error.message; }
            if (runtimeError !== (locale === 'en' ? 'sts-player must be mounted inside a dedicated agent/preset scope'
                : 'sts-player 必须挂载在独立的 Agent 或预设作用域中')) {
              throw new Error(`Installed scripted runtime lost its scoped language: ${locale}`);
            }
            let launcherError;
            try { await runPlayer({ locale, agentLocale: locale === 'en' ? 'zh-CN' : 'en', model: 'deepseek-v4-pro' }); }
            catch (error) { launcherError = error.message; }
            if (!launcherError?.startsWith(locale === 'en' ? 'deepseek-v4-pro is prohibited' : '此玩家不使用 deepseek-v4-pro')) {
              throw new Error(`Installed launcher did not separate operator and agent language: ${locale}`);
            }
            const paused = playerPauseText({ status: 'pending', command: 'buy:Inflame#0', reason: 'Opaque {{reason}}' }, locale);
            if (!paused.startsWith(locale === 'en' ? 'Pending proposal (not executed)' : '待审提案（尚未执行）')
                || !paused.includes('buy:Inflame#0') || !paused.includes('Opaque {{reason}}')) {
              throw new Error(`Installed supervision lost language or recorded content: ${locale}`);
            }
            const inspected = renderInspection({ directory: 'offline-check', manifest: { id: 'offline-player' }, steps: [], scope: 'noRecordedTurn' }, { locale });
            if (!inspected.includes(locale === 'en' ? 'No recorded model turn in this run.' : '此运行没有已记录的模型轮次。')
                || !inspected.includes('player inspect N input')) {
              throw new Error(`Installed inspection lost language or command: ${locale}`);
            }
            const preparation = preparationInstructions(locale), card = cardChoiceInstructions(locale), shop = shopInstructions(locale);
            if (!preparationStageGuide('pending', undefined, {}, undefined, locale).includes(locale === 'en' ? 'Current task: rate' : '当前任务：评估')
                || !preparation.text('eventCombatEntry').includes(locale === 'en' ? 'received after victory' : '在获胜后取得')
                || !preparation.text('chokerSharedAllowance').includes(locale === 'en' ? 'Drawing a card alone' : '单纯抽到一张牌')
                || !preparation.text('classificationRowFormat').includes(locale === 'en' ? 'Use the references' : '使用该命令旁的引用')
                || !card.parameters.command.description.includes('choose') || !shop.spendingGuide.includes('then FOLLOW_UP_COMMAND')
                || preparation.comparisons.map(row => row.id).join(',') !== '@function,@access,@protection,@resources,@opportunity'
                || !playedCardLifecycle({ type: 'Power' }, locale).includes(locale === 'en' ? 'after deployment' : '部署后')
                || !strengthTiming('each_player_turn_start_post_draw', locale).includes(locale === 'en' ? 'after draw' : '抽牌之后')
                || !preparationCardChoiceRoute({ rules: [], candidates: [{ command: 'skip-card#0' }] }, locale).gap.message
                  .includes(locale === 'en' ? 'supplied strategy policy' : '需要提供策略规则')
                || !renderShopBudget({ gold: 40, starts: [] }, undefined, locale).includes(locale === 'en' ? 'Shop budget: Gold 40' : '商店预算：金币 40')) {
              throw new Error(`Installed preparation guide lost language or protocol: ${locale}`);
            }
            report.installed_scripted_checks.push({ package: manifest.name, locale, imported_entries: true, preserved_guide_commands_and_operator: true, runtime_control_messages: true, launcher_operator_locale_independent: true, supervision_record_content_preserved: true, inspection_typed_scope_messages: true, preparation_guides_and_schemas: true, deck_mechanism_messages: true, card_choice_messages: true, shop_messages: true, preparation_card_conditions: true, preparation_resource_conditions: true, preparation_classification_messages: true });
          }
        }
      }
    }
    report.passed = true;
    await writeFile(join(output, 'distribution.json'), JSON.stringify(report, null, 2) + '\n');
    console.log(JSON.stringify({ passed: true, packages: report.packages }));
  } finally {
    // Resolve the one known build directory before recursive removal. Source
    // checkouts, node_modules and arbitrary output files are outside this path.
    const absoluteStage = await realpath(stageRoot);
    within(output, absoluteStage);
    if (absoluteStage.toLowerCase() !== stageRoot.toLowerCase()) throw new Error('Unexpected redirected staging directory');
    await rm(absoluteStage, { recursive: true, force: true });
  }
}
