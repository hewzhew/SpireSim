#!/usr/bin/env node
// Composition and lifecycle only; the official dsh loop owns model execution.
import { mkdir, readFile, realpath, writeFile, rename, appendFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { performance } from 'node:perf_hooks';
import { createPresentation } from '@sts-simulator/presentation';
import { Context } from '@deepseek-ai/cordis';
import LlmRuntime, { createUserMessage, ToolCallId } from '@deepseek-ai/dsh-llm';
import SessionStore, { SessionId } from '@deepseek-ai/dsh-session';
import SessionProjectionRegistry from '@deepseek-ai/dsh-session-projection';
import AgentRegistry from '@deepseek-ai/dsh-agent';
import AgentLoop from '@deepseek-ai/dsh-agent-loop';
import * as LlmRetry from '@deepseek-ai/dsh-llm-retry';
import SystemPrompt from '@deepseek-ai/dsh-system-prompt';
import ToolRuntime from '@deepseek-ai/dsh-tools';
import * as SessionPersistence from '@deepseek-ai/dsh-session-persistence-jsonl';
import LocalFileSystem from '@deepseek-ai/dsh-fs-local';
import WorkingDirectory from '@deepseek-ai/dsh-working-directory';
import Subprocess from '@deepseek-ai/dsh-subprocess-local';
import TokenMeter from '@deepseek-ai/dsh-token-meter';
import GameContextCompaction, { gameContextSource, sameGameContextSource } from './context.mjs';
import * as FileTools from '@deepseek-ai/dsh-tool-fs';
import * as FileSearch from '@deepseek-ai/dsh-tool-fs-search';
import * as TodoTools from '@deepseek-ai/dsh-tool-todo';
import * as DeepSeek from '@deepseek-ai/dsh-llm-deepseek-api-key';
import * as Player from './index.mjs';
import { readPlayerReview, readPlayerBinding, bindCoachedPlayer, playerIsRunning,
  capturePlayerFrame, renderPlayerReview, setPlayerGuidance,
  playerReviewOutstanding, adoptPlayerProposal } from './supervision.mjs';
import { preparationTaskRoot, preparationTaskProgress, preparationProceduralAction,
  restoredPreparationDecision, preparationReading, preparationRewardProgress, preparationReadingSupply } from './preparation.mjs';
import { preparationCardChoice, renderPreparationCardChoice, adoptPreparationCardChoice } from './card-choice.mjs';
import { inspectPlayer, parseInspectionArgs, recordPlayerRequests } from './inspection.mjs';

function launchText(locale, key) { return createPresentation({ locale }).text(`scripted:launch.${key}`); }
function launcherText(locale, key, parameters = {}) {
  return createPresentation({ locale }).text(`scripted:launcher.${key}`,
    Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));
}

// Decision boundaries cut ordinary room history. Pressure compaction remains
// a fallback for a single unusually long shop or combat investigation.
const playerCompaction = { thresholdRatio: 0.25, retainTokens: 65536 };

function safeError(error) {
  const message = error instanceof Error ? error.message : String(error);
  const key = process.env.DEEPSEEK_API_KEY;
  return key ? message.replaceAll(key, '[redacted]') : message;
}

function samePath(a, b) {
  return process.platform === 'win32' ? a.toLowerCase() === b.toLowerCase() : a === b;
}

// The host owns cancellation. A file request works even when Windows console
// interrupts terminate the process group before signal handlers can clean up.
export function watchPlayerStopRequest(path, stop, intervalMs = 1000) {
  const timer = setInterval(() => {
    if (!existsSync(path)) return;
    clearInterval(timer);
    stop();
  }, intervalMs);
  timer.unref();
  return () => clearInterval(timer);
}

export function continuationTask(task, previousRunOutcome, locale = 'en') {
  const reason = previousRunOutcome === 'interrupted' ? launchText(locale, 'interrupted')
    : previousRunOutcome === 'time-limit' ? launchText(locale, 'timeLimit') : '';
  return reason ? reason + launchText(locale, 'continuation') + '\n\n' + task : task;
}

export async function runPlayer(options, display = console.log) {
  const locale = createPresentation({ locale: options.locale }).locale;
  if (options.model === 'deepseek-v4-pro') throw new Error(launcherText(locale, 'proModelProhibited'));
  let agentLocale = options.agentLocale ?? 'en';
  if (!['en', 'zh-CN'].includes(agentLocale)) throw new Error(launcherText(locale, 'invalidAgentLocale'));
  let taskScope = options.taskScope ?? 'run';
  if (!['run', 'preparation'].includes(taskScope)) throw new Error(launcherText(locale, 'invalidTaskScope'));
  if (options.supervised && taskScope !== 'run') throw new Error(launcherText(locale, 'coachedScope'));
  const repository = await realpath(resolve(options.repository));
  const session = await realpath(resolve(options.session));
  const home = resolve(options.home ?? join(repository, '.oracle-lab/dsh-player'));
  const binary = await realpath(resolve(options.binary ?? join(repository, 'target/dev-opt/run_play.exe')));
  const id = options.resume ?? `sts-player-${randomUUID()}`;
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_-]*$/.test(id)) throw new Error(launcherText(locale, 'invalidSessionId'));
  const maxMs = Number(options.maxMs ?? 0);
  const maxTokens = Number(options.maxTokens ?? 32768);
  if (!Number.isSafeInteger(maxMs) || maxMs < 0 || maxMs > 2147483647) throw new Error(launcherText(locale, 'invalidMaxMs'));
  if (!Number.isSafeInteger(maxTokens) || maxTokens <= 0) throw new Error(launcherText(locale, 'invalidMaxTokens'));
  if (!options.dry && !process.env.DEEPSEEK_API_KEY) throw new Error(launcherText(locale, 'apiKeyRequired'));
  // Use the same ESM resolver as the imported components, including a host
  // loader. Resolve metadata before opening the game or writing a player binding.
  const versions = {};
  for (const name of ['@deepseek-ai/dsh-agent-loop', '@deepseek-ai/dsh-llm-deepseek-api-key', '@deepseek-ai/dsh-compaction-basic', '@deepseek-ai/dsh-llm-retry']) {
    versions[name] = JSON.parse(await readFile(new URL(import.meta.resolve(`${name}/package.json`)), 'utf8')).version;
  }
  await mkdir(join(home, 'players'), { recursive: true });
  const bindingFile = join(home, 'players', `${id}.json`);
  const binding = { id, repository, session };
  const output = join(home, 'runs', `${id}-${Date.now()}-${randomUUID().slice(0, 8)}`);
  await mkdir(output, { recursive: true });
  let previousGameSource, previousRunOutcome, preparationRoot, rewardProgress = options.rewardProgress;
  if (options.resume) {
    const saved = JSON.parse(await readFile(bindingFile, 'utf8'));
    if (saved.id !== id || typeof saved.repository !== 'string' || typeof saved.session !== 'string'
        || !samePath(saved.repository, repository) || !samePath(saved.session, session)) {
      throw new Error(launcherText(locale, 'bindingMismatch'));
    }
    previousGameSource = saved.lastGameSource;
    previousRunOutcome = saved.lastRunOutcome;
    const savedLocale = saved.agentLocale ?? 'en';
    if (!['en', 'zh-CN'].includes(savedLocale) || options.agentLocale !== undefined && options.agentLocale !== savedLocale) {
      throw new Error(launcherText(locale, 'resumeLanguageMismatch'));
    }
    agentLocale = savedLocale;
    if (options.taskScope !== undefined && options.taskScope !== (saved.taskScope ?? 'run')) {
      throw new Error(launcherText(locale, 'resumeScopeMismatch'));
    }
    taskScope = saved.taskScope ?? 'run';
    options.supervised ??= saved.supervised;
    preparationRoot = saved.preparationRoot;
    rewardProgress = saved.rewardProgress;
    if (taskScope === 'preparation' && !preparationRoot) throw new Error(launcherText(locale, 'missingPreparationRoot'));
  }
  if (options.supervised && taskScope !== 'run') throw new Error(launcherText(locale, 'coachedScope'));
  if (options.supervised && !options.dry && playerIsRunning(await readPlayerBinding(session, locale))) {
    throw new Error(launcherText(locale, 'coachedRunning'));
  }
  if (taskScope === 'preparation' && !preparationRoot) {
    const { openRunPlay } = await import(pathToFileURL(join(repository, 'tools/run_play/client.mjs')));
    const probe = await openRunPlay({ repository, session, binary, agentLocale, recordDirectory: join(output, 'task-root-wire') });
    try { preparationRoot = preparationTaskRoot(await probe.observe(), agentLocale); }
    finally { await probe.close(); }
  }
  Object.assign(binding, { taskScope, agentLocale, supervised: Boolean(options.supervised), ...(preparationRoot ? { preparationRoot } : {}), ...(rewardProgress ? { rewardProgress } : {}) });
  const task = options.taskFile ? await readFile(resolve(options.taskFile), 'utf8')
    : options.task ?? launchText(agentLocale, taskScope === 'preparation' ? 'preparationTask' : 'defaultTask');
  if (!options.dry && !task.trim()) throw new Error(launcherText(locale, 'emptyTask'));
  if (!options.resume) {
    await writeFile(bindingFile, JSON.stringify(binding, null, 2) + '\n', { flag: 'wx' });
  }
  const executionTask = continuationTask(task, previousRunOutcome, agentLocale);
  const stopRequestFile = join(output, 'stop-request');
  const ctx = new Context();
  const started = performance.now();
  const agentOptions = { provider: 'deepseek-official', model: options.model ?? 'deepseek-flash',
    reasoningEffort: options.reasoning ?? 'low', maxTokens };
  let handle, timer, closeStopWatcher, closeExternalStopWatcher, closeRequestRecorder, outcome = 'setup', steps = 0, finalText = '', result, runError;
  const stop = reason => {
    outcome = reason;
    handle?.agent.cancel({ kind: 'user' });
  };
  const onInterrupt = () => stop('interrupted');
  process.once('SIGINT', onInterrupt);
  process.once('SIGTERM', onInterrupt);
  try {
    for (const plugin of [LlmRuntime, SessionStore, SessionProjectionRegistry, TokenMeter, AgentRegistry, ToolRuntime]) {
      await ctx.plugin(plugin);
    }
    await ctx.plugin(SystemPrompt, { includeHarnessIdentity: false, includeRuntimeContext: false,
      personaPrefix: taskScope === 'preparation'
        ? launchText(agentLocale, 'personaPreparation') : launchText(agentLocale, 'personaRun') });
    await ctx.plugin(SessionPersistence, { root: join(home, 'sessions'), compression: 'none' });
    await ctx.plugin(LocalFileSystem, { cwd: repository });
    await ctx.plugin(WorkingDirectory, { defaultDirectory: repository });
    await ctx.plugin(Subprocess);
    await ctx.plugin(FileTools);
    await ctx.plugin(FileSearch, { sampleOverCapGlobResults: true });
    await ctx.plugin(TodoTools, { allowParallelInProgress: true });
    await ctx.plugin(DeepSeek, { reasoningEffort: agentOptions.reasoningEffort, maxTokens });
    await ctx.plugin(GameContextCompaction, { ...playerCompaction, maxTokens });
    await ctx.plugin(AgentLoop, { agents: [], maxParallelToolCalls: 1 });
    await ctx.plugin(LlmRetry);
    const gameCalls = new Set();
    let lastGameText;
    ctx.on('session/event', (owner, event) => {
      if (owner.id !== id) return;
      if (event.type === 'step/start') steps++;
      if (event.type === 'llm/retry') {
        display(launcherText(locale, 'requestRetry', { step: steps, retry: event.data.retry, maxRetries: event.data.maxRetries, failureCode: event.data.failure.code, delayMs: Math.round(event.data.delayMs) }));
      }
      if (event.type === 'tool/call') {
        if (event.data.name === 'sts') gameCalls.add(event.data.callId);
        let args = event.data.arguments;
        try { if (typeof args === 'string') args = JSON.parse(args); } catch { args = undefined; }
        display(`[${steps}] ${event.data.name}${args?.command ? ' ' + args.command : ''}`);
      }
      if (event.type === 'tool/result') {
        const isGameCall = gameCalls.delete(event.data.message?.toolCallId);
        const text = (event.data.message.content ?? []).filter(x => x.type === 'text').map(x => x.text).join('\n');
        if (event.data.message?.isError) display(launcherText(locale, 'toolError', { step: steps, error: safeError(text) }));
        else if (options.supervised && isGameCall && text !== lastGameText) {
          lastGameText = text;
          display(safeError(text));
        }
      }
      if (event.type === 'assistant/message') {
        const text = (event.data.message?.content ?? []).filter(x => x.type === 'text').map(x => x.text).join('\n');
        if (text) { finalText = text; display(text); }
      }
    });
    let attachedGameSource, resetContextOnAttach = true;
    if (options.resume) {
      const { openRunPlay } = await import(pathToFileURL(join(repository, 'tools/run_play/client.mjs')));
      const probe = await openRunPlay({ repository, session, binary, agentLocale,
        recordDirectory: join(output, 'attachment-observation-wire') });
      try { attachedGameSource = gameContextSource(await probe.observe()); }
      finally { await probe.close(); }
      resetContextOnAttach = !sameGameContextSource(previousGameSource, attachedGameSource);
    }
    const setup = async agentCtx => {
      await agentCtx.plugin(Player, { repository, session, binary, agentLocale, resetContextOnAttach, taskScope, supervised: options.supervised, preparationRoot, rewardProgress });
    };
    handle = options.resume
      ? await ctx.agents.resume({ resumeSessionId: SessionId(id), agentOptions, setup })
      : await ctx.agents.create({ sessionId: SessionId(id), meta: { cwd: repository }, agentOptions, setup });
    closeRequestRecorder = recordPlayerRequests(ctx, { id, session: () => handle.agent.session, output });
    if (outcome === 'interrupted') return { id, outcome, steps: 0, output };
    const tools = ctx.tools.schemas(handle.agent).map(x => x.name).sort();
    const required = taskScope === 'preparation' ? ['sts', 'sts_feedback']
      : ['sts', 'sts_feedback', 'read', 'grep', 'glob', 'todo_write'];
    const allowed = new Set(taskScope === 'preparation' ? required : [...required, 'read_image', 'skill']);
    if (required.some(name => !tools.includes(name)) || tools.some(name => !allowed.has(name))) {
      throw new Error(launcherText(locale, 'invalidTools')
        + JSON.stringify({ required, actual: tools }));
    }
    const source = { commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repository, encoding: 'utf8' }).trim(),
      worktree: execFileSync('git', ['status', '--short'], { cwd: repository, encoding: 'utf8' }).trim() };
    await writeFile(join(output, 'manifest.json'), JSON.stringify({ ...binding, binary, agentOptions, tools, task: executionTask, stopRequestFile,
      eventStart: handle.agent.session.snapshotEvents().length,
      source, versions, nodeVersion: process.version, dry: Boolean(options.dry), officialLoop: true,
      contextAttachment: { resetContextOnAttach, gameSource: attachedGameSource },
      compaction: { backend: 'dsh-compaction-basic', ...playerCompaction, maxTokens } }, null, 2) + '\n');
    display(launcherText(locale, 'playerStarted', { id: id, model: agentOptions.model, output: output }));
    if (options.supervised && !options.dry) await bindCoachedPlayer(session, {
      id, home, model: agentOptions.model, reasoning: agentOptions.reasoningEffort, maxTokens, agentLocale,
      running: true, pid: process.pid, lastRun: output,
    });
    if (options.dry) {
      const reading = await ctx.tools.execute({ name: 'sts', arguments: { command: 'read' },
        signal: new AbortController().signal, callId: ToolCallId('player-dry-read'), agent: handle.agent });
      if (reading.isError) throw new Error(reading.content.filter(x => x.type === 'text').map(x => x.text).join('\n'));
      display(reading.content.filter(x => x.type === 'text').map(x => x.text).join('\n'));
      outcome = 'dry-setup-passed';
    } else {
      closeStopWatcher = watchPlayerStopRequest(stopRequestFile, () => stop('interrupted'));
      if (options.externalStopRequestFile) closeExternalStopWatcher = watchPlayerStopRequest(options.externalStopRequestFile, () => stop('interrupted'));
      if (maxMs) timer = setTimeout(() => stop('time-limit'), maxMs);
      handle.agent.followup(createUserMessage({ content: [{ type: 'text', text: executionTask }], source: { kind: 'user' } }));
      await handle.agent.whenIdle();
      if (!['time-limit', 'interrupted'].includes(outcome)) {
        const end = handle.agent.session.snapshotEvents().filter(x => x.type === 'turn/end').at(-1);
        outcome = end?.data.reason.kind ?? 'idle-without-turn';
        if (options.supervised && outcome === 'completed' && playerReviewOutstanding(await readPlayerReview(session, locale))) outcome = 'needs-review';
        if (outcome === 'error') {
          const error = end.data.reason.error;
          runError = { code: error?.code, message: safeError(error?.message ?? error) };
          display(launcherText(locale, 'codedError', { codeSuffix: runError.code ? ` [${runError.code}]` : '', message: runError.message }));
          await writeFile(join(output, 'error.txt'), runError.message + '\n');
        }
      }
    }
  } catch (error) {
    outcome = 'error';
    const message = safeError(error);
    runError = { code: error?.code, message };
    display(launcherText(locale, 'error', { message: message }));
    await writeFile(join(output, 'error.txt'), message + '\n');
  } finally {
    clearTimeout(timer);
    closeStopWatcher?.();
    closeExternalStopWatcher?.();
    closeRequestRecorder?.();
    process.removeListener('SIGINT', onInterrupt);
    process.removeListener('SIGTERM', onInterrupt);
    result = { id, outcome, error: runError, steps, elapsedMs: performance.now() - started,
      finalText: outcome === 'completed' ? finalText : undefined, lastAssistantText: finalText,
      ...(handle ? { eventEnd: handle.agent.session.snapshotEvents().length } : {}),
      modelUsed: !options.dry && steps > 0 };
    const cleanupErrors = [];
    if (handle) {
      try {
        if (taskScope === 'preparation') result.preparationDecision = restoredPreparationDecision(handle.agent.session);
        const projection = ctx.sessionProjections.snapshot(handle.agent.session).values;
        result.tokenUsage = projection.tokenUsage;
        result.contextPressure = projection.contextPressure;
      } catch (error) { cleanupErrors.push(launcherText(locale, 'measurementError', { error: safeError(error) })); }
    }
    for (const [name, close] of [
      [launcherText(locale, 'flush'), async () => { if (handle) await ctx.sessions.flush(handle.agent.session); }],
      [launcherText(locale, 'agentDisposal'), async () => { if (handle) await handle.dispose(); }],
      [launcherText(locale, 'contextDisposal'), async () => { await ctx.fiber.dispose(); }],
    ]) {
      try { await close(); } catch (error) { cleanupErrors.push(`${name}: ${safeError(error)}`); }
    }
    if (cleanupErrors.length) {
      result.cleanupErrors = cleanupErrors;
      result.outcome = outcome = 'cleanup-error';
      display(cleanupErrors.join('\n'));
    }
    // Read only after the player has released its connection. Loop completion
    // is independent of the simulator's actual terminal/decision position.
    if (!cleanupErrors.length) {
      let game, finalGameSource;
      try {
        const { openRunPlay } = await import(pathToFileURL(join(repository, 'tools/run_play/client.mjs')));
        game = await openRunPlay({ repository, session, binary, agentLocale, recordDirectory: join(output, 'final-observation-wire') });
        const observation = await game.observe();
        finalGameSource = gameContextSource(observation);
        if (preparationRoot) result.preparationProgress = preparationTaskProgress(preparationRoot, observation, agentLocale);
        await writeFile(join(output, 'final-observation.json'), JSON.stringify(observation, null, 2) + '\n');
        const position = observation.position;
        result.gamePosition = position.kind === 'terminal'
          ? { kind: position.kind, outcome: position.outcome }
          : { kind: position.kind, site: position.site };
      } catch (error) {
        result.gameObservationError = safeError(error);
        display(launcherText(locale, 'finalObservationFailed', { error: result.gameObservationError }));
      } finally {
        if (game) {
          try { await game.close(); }
          catch (error) { result.gameObservationCloseError = safeError(error); }
        }
      }
      // A dry read does not refresh model knowledge and must never bless stale
      // context after an external move. Crashes/missing observations also leave
      // continuity unconfirmed. Replace the checkpoint only after both owners
      // released their connections and at least one actual model step ran.
      if (!options.dry && steps > 0 && finalGameSource
          && !result.gameObservationError && !result.gameObservationCloseError) {
        const temporary = `${bindingFile}.${randomUUID()}.tmp`;
        try {
          await writeFile(temporary, JSON.stringify({ ...binding, lastGameSource: finalGameSource, lastRunOutcome: outcome }, null, 2) + '\n', { flag: 'wx' });
          await rename(temporary, bindingFile);
        } catch (error) {
          result.contextCheckpointError = safeError(error);
          display(launcherText(locale, 'checkpointFailed', { error: result.contextCheckpointError }));
        }
      }
    }
    await writeFile(join(output, 'result.json'), JSON.stringify(result, null, 2) + '\n');
    if (options.supervised && !options.dry) await bindCoachedPlayer(session, {
      id, home, model: agentOptions.model, reasoning: agentOptions.reasoningEffort, maxTokens, agentLocale,
      running: cleanupErrors.length > 0, pid: process.pid, lastRun: output,
    });
    display(options.supervised ? launcherText(locale, 'coachedResult', { outcome: outcome }) : launcherText(locale, 'playerResult', { outcome: outcome, home: home, id: id }));
  }
  return { ...result, output };
}

// Operator controls share the existing bound workbench and official launcher.
// Continuation starts only after successful adoption and connection release.
export async function controlPlayer({ repository, session, parameters = [], binary, locale = 'en' }, display = console.log,
  { openWorkbench, startPlayer = runPlayer } = {}) {
  const operation = parameters[0] ?? 'status';
  let args = parameters.slice(1);
  if (operation === 'inspect') {
    const options = parseInspectionArgs(args);
    const inspected = await inspectPlayer({ repository, session, binding: await readPlayerBinding(session, locale), options, locale });
    display(inspected.text);
    return { outcome: 'inspected', report: inspected.report };
  }
  const continueAfter = ['accept', 'replace'].includes(operation) && args[0] === '--continue';
  if (continueAfter) args = args.slice(1);
  if (!['status', 'next', 'accept', 'replace', 'reject', 'coach'].includes(operation)) {
    throw new Error(launcherText(locale, 'controlUsage'));
  }
  if (['status', 'next', 'accept'].includes(operation) && args.length) throw new Error(launcherText(locale, 'controlNoArguments', { operation: operation }));
  if (['replace', 'reject', 'coach'].includes(operation) && !args.join(' ').trim()) throw new Error(launcherText(locale, 'controlArgumentRequired', { operation: operation, argument: operation === 'replace' ? launcherText(locale, 'replacementCommand') : launcherText(locale, 'conciseGuidance') }));
  openWorkbench ??= (await import(pathToFileURL(join(repository, 'tools/run_play/workbench.mjs')))).openWorkbench;
  const binding = await readPlayerBinding(session, locale);
  if (operation !== 'status' && playerIsRunning(binding)) throw new Error(launcherText(locale, 'pendingPlayerRunning'));
  const game = await openWorkbench({ repository, session, binary, agentLocale: binding?.agentLocale ?? 'en', commandPrefix: '' });
  let nextOptions;
  try {
    let review = await readPlayerReview(session, locale);
    let frame = await capturePlayerFrame(game, review?.frameAfter?.task ?? review?.frame?.task ?? review?.guidance?.task, locale);
    if (operation === 'status') display(renderPlayerReview(frame, review, locale));
    else if (operation === 'coach' || operation === 'reject') {
      await setPlayerGuidance(session, frame, args.join(' '), { reject: operation === 'reject', locale });
      display(operation === 'reject' ? launcherText(locale, 'proposalRejected') : launcherText(locale, 'guidanceSaved'));
    } else if (operation === 'accept' || operation === 'replace') {
      const result = await adoptPlayerProposal(session, game, { ...(operation === 'replace' ? { command: args.join(' ') } : {}), locale });
      display(renderPlayerReview(result.frame, result.review, locale));
      frame = result.frame;
      review = result.review;
    }
    if (operation === 'next' || continueAfter) {
      if (playerReviewOutstanding(review)) {
        throw new Error(launcherText(locale, 'outstandingReview'));
      }
      if (frame.task.family === 'terminal') { display(frame.text); return { outcome: 'terminal' }; }
      nextOptions = { repository, session, binary, locale, home: binding?.home,
        resume: binding?.id, model: binding?.model ?? 'deepseek-flash',
        reasoning: binding?.reasoning ?? 'low', maxTokens: binding?.maxTokens ?? 16384,
        maxMs: 90000, supervised: true, agentLocale: binding?.agentLocale ?? 'en',
        task: launchText(binding?.agentLocale ?? 'en', 'defaultTask') };
    }
  } finally { await game.close(); }
  return nextOptions ? startPlayer(nextOptions, display) : { outcome: 'completed' };
}

// Conserve potions among requested-quality wins or small HP costs with a
// usable reserve. Keep the leading quality win's persistent resources; the HP
// allowance is a maintained adoption preference, not a search quality claim.
// Cyclic counter identities and ranges follow the shared relic owner in
// src/content/relics/persistent_state.rs. For this adoption preference, equal
// or greater progress may preserve a potion; other relic fields stay exact.
function retainsCombatRelics(relics, baseline, preparation) {
  if (!Array.isArray(relics) || !Array.isArray(baseline)) return false;
  const triggers = { Sundial: 3, HappyFlower: 3, InkBottle: 10, Nunchaku: 10, PenNib: 10 };
  return relics.length === baseline.length && relics.every((relic, index) => {
    const original = baseline[index];
    if (preparation && relic.id === 'IncenseBurner' && original?.id === relic.id) {
      const { counter, ...fields } = relic, { counter: originalCounter, ...originalFields } = original;
      if (JSON.stringify(fields) !== JSON.stringify(originalFields)) return false;
      if (![counter, originalCounter].every(value => Number.isSafeInteger(value) && value >= 0 && value < 6)) return false;
      if (counter === originalCounter) return true;
      // Rust projects the next trigger turn from the relic owner. The Heart's
      // first attack pair occupies turns 2 and 3; the order stays unrevealed.
      return preparation.earlyAttackCoverage;
    }
    const trigger = triggers[relic.id];
    if (!trigger) return JSON.stringify(relic) === JSON.stringify(original);
    const { counter = 0, ...fields } = relic;
    const { counter: originalCounter = 0, ...originalFields } = original;
    return JSON.stringify(fields) === JSON.stringify(originalFields)
      && Number.isSafeInteger(counter) && counter >= 0 && counter < trigger
      && Number.isSafeInteger(originalCounter) && originalCounter >= 0 && originalCounter < trigger
      && counter >= originalCounter;
  });
}

export function preparationNextCombat(candidate, observation, locale = 'en') {
  const context = observation?.position?.observation?.run_context;
  if (context?.status !== 'available' || context.act !== 4
      || context.encounter_id !== 'ShieldAndSpear' || context.run_goal !== 'heart_victory') return;
  const turn = candidate?.nextIncenseTurn;
  if (!Number.isSafeInteger(turn) || turn < 1 || turn > 6) return;
  const incense = candidate.outcome?.final_relics?.find(relic => relic.id === 'IncenseBurner');
  if (!incense || !Number.isSafeInteger(incense.counter) || incense.counter < 0 || incense.counter >= 6) return;
  return { encounter: 'TheHeart', incenseTriggerTurn: turn,
    earlyAttackCoverage: turn === 2 || turn === 3,
    basis: launchText(locale, 'heartOpening') };
}

function smallCombatHpCostAllowance(player) {
  return Number.isSafeInteger(player?.hp) && Number.isSafeInteger(player?.max_hp)
    && player.max_hp > 0 ? Math.floor(player.max_hp / 10) : undefined;
}

export function preparationCombatCandidate(candidates, observation) {
  const qualified = candidates.filter(row => row.meetsRequestedSatisfaction);
  const player = observation?.position?.observation?.observation?.player;
  const context = observation?.position?.observation?.run_context;
  if (context?.run_goal === 'heart_victory' && context.encounter_id === 'TheHeart') return candidates[0];
  const allowance = smallCombatHpCostAllowance(player);
  const eligible = candidates.filter(row => row.meetsRequestedSatisfaction
    || (allowance !== undefined && Number.isSafeInteger(row.outcome.final_hp)
      && player.hp - row.outcome.final_hp <= allowance
      && row.outcome.final_hp * 3 > player.max_hp));
  if (!eligible.length) return candidates[0];
  const baseline = (qualified[0] ?? eligible[0]).outcome;
  const baselineCandidate = qualified[0] ?? eligible[0];
  const baselinePreparation = preparationNextCombat(baselineCandidate, observation);
  const persistent = { max_hp: 1, recoverable_gold_delta: 1, unrecovered_stolen_gold: -1,
    ritual_dagger_value: 1, genetic_algorithm_value: 1, external_burden_count: -1 };
  const gold = outcome => outcome.final_gold + (outcome.pending_rewards ?? [])
    .filter(row => ['gold', 'stolen_gold'].includes(row.kind)).reduce((sum, row) => sum + row.amount, 0);
  const nonGold = outcome => (outcome.pending_rewards ?? []).filter(row => !['gold', 'stolen_gold'].includes(row.kind));
  const retained = eligible.filter(row => {
    const outcome = row.outcome;
    const preparation = preparationNextCombat(row, observation);
    const timingPreference = preparation && baselinePreparation
      && preparation.earlyAttackCoverage
      && outcome.final_hp >= baseline.final_hp
      && outcome.total_potions_consumed <= baseline.total_potions_consumed
      && JSON.stringify(outcome.consumed_entry_potion_slots) === JSON.stringify(baseline.consumed_entry_potion_slots)
      ? preparation : undefined;
    return Object.entries(persistent).every(([key, sign]) =>
      outcome.persistent?.[key] * sign >= baseline.persistent?.[key] * sign)
      && gold(outcome) >= gold(baseline)
      && retainsCombatRelics(outcome.final_relics, baseline.final_relics, timingPreference)
      && ['meta_changes', 'generated_potions'].every(key =>
        JSON.stringify(outcome[key]) === JSON.stringify(baseline[key]))
      && JSON.stringify(nonGold(outcome)) === JSON.stringify(nonGold(baseline));
  });
  return retained.sort((a, b) => a.outcome.total_potions_consumed - b.outcome.total_potions_consumed
    || a.outcome.consumed_entry_potion_slots.length - b.outcome.consumed_entry_potion_slots.length
    || b.outcome.final_hp - a.outcome.final_hp
    || Number(preparationNextCombat(b, observation)?.earlyAttackCoverage ?? false)
      - Number(preparationNextCombat(a, observation)?.earlyAttackCoverage ?? false))[0] ?? qualified[0] ?? eligible[0];
}

// Review material HP costs before adoption. A replayed win describes one
// reachable outcome; a large loss needs investigation while the root remains
// available, rather than becoming the next classifier's depleted inventory.
export function preparationCombatReview(candidate, observation, locale = 'en') {
  const player = observation.position?.observation?.observation?.player;
  const hp = candidate?.outcome?.final_hp;
  if (!player || !Number.isSafeInteger(player.hp) || !Number.isSafeInteger(player.max_hp)
      || player.max_hp <= 0 || !Number.isSafeInteger(hp)) {
    throw new Error(launcherText(locale, 'actualCombatHpRequired'));
  }
  const context = observation.position.observation.run_context;
  // The goal is achieved by any replayed Heart win; there is no later route
  // for which the remaining HP must be conserved.
  if (context?.run_goal === 'heart_victory' && context.encounter_id === 'TheHeart') return;
  const loss = player.hp - hp;
  if (loss <= 0) return;
  const allowance = smallCombatHpCostAllowance(player);
  const reasons = [];
  if (!candidate.meetsRequestedSatisfaction && loss > allowance) {
    reasons.push('unqualified-hp-cost-exceeds-small-allowance');
  }
  if (loss * 3 >= player.max_hp) reasons.push('loss-at-least-one-third-max-hp');
  if (hp * 3 <= player.max_hp) reasons.push('remaining-hp-at-most-one-third-max-hp');
  if (!reasons.length) return;
  return { reasons, startingHp: player.hp, maxHp: player.max_hp, settledHp: hp,
    netHpLoss: loss, smallHpCostAllowance: allowance, candidate: candidate.ref, adopted: false };
}

// When bounded search finds no win at low HP, a legal Smoke Bomb can provide
// a verified resource-preserving exit. Preview the actual escape settlement,
// including its pending End Turn, before adopting it. The missing win remains
// Unknown; escape is recorded separately from a combat victory.
export async function preparationCombatEscape(game, observation) {
  const visible = observation.position?.observation;
  const player = visible?.observation?.player;
  if (visible?.domain !== 'combat' || !Number.isSafeInteger(player?.hp)
      || !Number.isSafeInteger(player.max_hp) || player.hp <= 0 || player.max_hp <= 0
      || !Number.isSafeInteger(player.gold)
      || player.hp * 3 > player.max_hp) return;
  const action = observation.position.actions.find(row => row.candidate?.action?.kind === 'use_potion'
    && row.combat_effect?.effect?.potion?.potion_id === 'SmokeBomb');
  if (!action) return;
  const started = await game.preview([action.semantic_action]);
  if (started.endpoint?.observation?.domain !== 'combat'
      || started.endpoint.observation.observation.turn?.player_escaping !== true) return;
  const end = started.endpoint.actions?.find(row => row.candidate?.action?.kind === 'end_turn');
  if (!end) return;
  const actionLine = [action.semantic_action, end.semantic_action];
  const settled = await game.preview(actionLine);
  const state = settled.endpoint?.observation;
  if (settled.endpoint?.kind !== 'decision' || state?.domain !== 'strategic'
      || !Number.isSafeInteger(state.observation.run?.current_hp)
      || state.observation.run.current_hp < player.hp
      || !Number.isSafeInteger(state.observation.run.gold) || state.observation.run.gold < player.gold) return;
  return { actionLine, startingHp: player.hp, settledHp: state.observation.run.current_hp,
    endpoint: settled.endpoint, view: settled.view, searchResult: 'unknown' };
}

// Compatible single-card defaults run before model setup. Other strategic
// choices get a fresh official player; combat and forced continuations remain
// with their existing owners.
export async function runPreparationFlow(options, display = console.log) {
  const locale = createPresentation({ locale: options.locale }).locale;
  const agentLocale = options.agentLocale ?? 'en';
  if (!['en', 'zh-CN'].includes(agentLocale)) throw new Error(launcherText(locale, 'invalidAgentLocale'));
  if (options.resume || options.dry || options.task || options.taskFile) {
    throw new Error(launcherText(locale, 'flowTaskRequired'));
  }
  if (options.model && options.model !== 'deepseek-flash') throw new Error(launcherText(locale, 'flowModelRequired'));
  const maxSteps = Number(options.flowMaxSteps ?? 64), stopFloor = Number(options.stopFloor ?? 0);
  if (!Number.isSafeInteger(maxSteps) || maxSteps < 1 || !Number.isSafeInteger(stopFloor) || stopFloor < 0) {
    throw new Error(launcherText(locale, 'invalidFlowLimits'));
  }
  const repository = await realpath(resolve(options.repository)), session = await realpath(resolve(options.session));
  const binary = await realpath(resolve(options.binary ?? join(repository, 'target/dev-opt/run_play.exe')));
  const output = join(resolve(options.home ?? join(repository, '.oracle-lab/dsh-player')), 'flows', randomUUID());
  await mkdir(output, { recursive: true });
  const { openRunPlay } = await import(pathToFileURL(join(repository, 'tools/run_play/client.mjs')));
  const { openWorkbench } = await import(pathToFileURL(join(repository, 'tools/run_play/workbench.mjs')));
  const { strategicOptionCommand } = await import(pathToFileURL(join(repository, 'tools/run_play/contract.mjs')));
  const stopRequestFile = join(output, 'stop-request');
  const rewardProgressFile = join(session, 'dsh-reward-progress.json');
  let rewardProgress = existsSync(rewardProgressFile) ? JSON.parse(await readFile(rewardProgressFile, 'utf8')) : undefined;
  const result = { outcome: 'step-limit', output, stopRequestFile, steps: 0, modelTasks: 0,
    combatSearches: 0, combatVictories: 0, combatEscapes: 0, forcedActions: 0, cardPolicyActions: 0 };
  await writeFile(join(output, 'manifest.json'), JSON.stringify({ repository, session, binary, agentLocale, maxSteps, stopFloor,
    model: 'deepseek-flash', maxMsPerTask: options.maxMs ?? 180000, stopRequestFile,
    source: { commit: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repository, encoding: 'utf8' }).trim(),
      worktree: execFileSync('git', ['status', '--short'], { cwd: repository, encoding: 'utf8' }).trim() } }, null, 2));
  display(launcherText(locale, 'flowStarted', { output: output }));
  try {
    for (let step = 0; step < maxSteps; step++) {
      if (existsSync(stopRequestFile)) { result.outcome = 'interrupted'; break; }
      const folder = join(output, `step-${String(step).padStart(3, '0')}`);
      await mkdir(folder);
      let game = await openRunPlay({ repository, session, binary, agentLocale, recordDirectory: join(folder, 'before-wire') });
      let before;
      try { before = await game.observe(); } finally { await game.close(); }
      await writeFile(join(folder, 'before.json'), JSON.stringify(before, null, 2));
      if (before.position.kind === 'terminal') { result.outcome = 'terminal'; result.terminal = before.position.outcome; break; }
      const visible = before.position.observation, combat = visible.domain === 'combat';
      const run = combat ? visible.run_context : visible.observation.run;
      if (stopFloor && run.floor >= stopFloor && !combat && visible.observation.context.kind === 'map') {
        result.outcome = 'checkpoint'; break;
      }
      display(launcherText(locale, 'flowStep', { step: step, act: run.act, floor: run.floor, context: combat ? 'combat' : visible.observation.context.kind }));
      const policyReading = preparationReading(before, rewardProgress);
      const procedural = combat ? undefined : preparationProceduralAction(policyReading);
      let cardFrame;
      if (!combat && !procedural) {
        try {
          cardFrame = preparationCardChoice(policyReading, strategicOptionCommand, agentLocale);
          if (cardFrame?.route.kind === 'model') {
            const supply = preparationReadingSupply(policyReading, strategicOptionCommand, agentLocale);
            policyReading.candidateReadings = supply.candidates;
            policyReading.ownedRelicReadings = supply.relicReadings;
            cardFrame = preparationCardChoice(policyReading, strategicOptionCommand, agentLocale);
            renderPreparationCardChoice(cardFrame, agentLocale);
          }
        }
        catch (error) {
          if (!error.preparationGap) throw error;
          result.outcome = 'preparation-needs-review'; result.preparationGap = error.preparationGap;
          if (cardFrame) await writeFile(join(folder, 'card-choice.json'), JSON.stringify(cardFrame, null, 2));
          await writeFile(join(folder, 'card-choice-gap.json'), JSON.stringify(error.preparationGap, null, 2));
          display(launcherText(locale, 'cardChoiceGap', { kind: error.preparationGap.kind, reason: error.preparationGap.reason }));
          break;
        }
      }
      if (cardFrame) await writeFile(join(folder, 'card-choice.json'), JSON.stringify(cardFrame, null, 2));
      if (cardFrame?.route.kind === 'review') {
        result.outcome = 'preparation-needs-review'; result.preparationGap = cardFrame.route.gap;
        await writeFile(join(folder, 'card-choice-gap.json'), JSON.stringify(cardFrame.route.gap, null, 2));
        display(launcherText(locale, 'cardChoiceGap', { kind: cardFrame.route.gap.kind, reason: cardFrame.route.gap.reason }));
        break;
      }
      let execution, adoptedAction, needsReview = false;
      if (combat) {
        game = await openRunPlay({ repository, session, binary, agentLocale, recordDirectory: join(folder, 'combat-wire') });
        try {
          const report = await game.research({ effort: 'whole_combat', maxSearchMs: 8000, maxGenerationWork: 200000 });
          result.combatSearches++;
          await writeFile(join(folder, 'search.json'), JSON.stringify(report.raw, null, 2));
          await writeFile(join(folder, 'search.txt'), report.view);
          if (!report.candidates.length) {
            const escape = await preparationCombatEscape(game, before);
            if (!escape) { result.outcome = 'search-needs-review'; break; }
            await writeFile(join(folder, 'combat-escape-preview.json'), JSON.stringify(escape, null, 2));
            const adopted = await game.act(escape.actionLine);
            await writeFile(join(folder, 'combat-escape.txt'), adopted.view);
            execution = { owner: 'verified-low-hp-escape', actionLine: escape.actionLine,
              startingHp: escape.startingHp, settledHp: escape.settledHp, searchResult: 'unknown' };
            result.combatEscapes++;
          } else {
            const candidate = preparationCombatCandidate(report.candidates, before);
            await writeFile(join(folder, 'combat-selection.json'), JSON.stringify({
              policy: 'requested-quality-or-small-hp-cost-then-conserve-potions',
              startingHp: visible.observation.player.hp, maxHp: visible.observation.player.max_hp,
              smallHpCostAllowance: smallCombatHpCostAllowance(visible.observation.player),
              selected: candidate.ref,
              minimumObservedHp: candidate.minimumObservedHp,
              nextCombatPreparation: preparationNextCombat(candidate, before, agentLocale),
              alternatives: report.candidates.map(row => ({ candidate: row.ref,
                meetsRequestedSatisfaction: row.meetsRequestedSatisfaction,
                minimumObservedHp: row.minimumObservedHp,
                nextCombatPreparation: preparationNextCombat(row, before, agentLocale), outcome: row.outcome })),
            }, null, 2));
            const review = preparationCombatReview(candidate, before, locale);
            if (review) {
              await writeFile(join(folder, 'combat-adoption-review.json'), JSON.stringify(review, null, 2));
              result.outcome = 'combat-needs-review'; result.combatReview = review;
              display(launcherText(locale, 'combatReview', { startingHp: review.startingHp, settledHp: review.settledHp, candidate: review.candidate }));
              break;
            }
            const adopted = await candidate.commit();
            await writeFile(join(folder, 'combat-commit.txt'), adopted.view);
            execution = { owner: 'CombatSolver', candidate: candidate.ref,
              policy: 'requested-quality-or-small-hp-cost-then-conserve-potions', outcome: candidate.outcome };
            result.combatVictories++;
          }
        } finally { await game.close(); }
      } else if (procedural) {
        const command = strategicOptionCommand(procedural.row.semantic_action, before);
        const workbench = await openWorkbench({ repository, session, binary, agentLocale, recordDirectory: join(folder, 'forced-wire'), commandPrefix: '' });
        try {
          if (!sameGameContextSource(before, await workbench.observe())) {
            throw new Error(launcherText(locale, 'proceduralPositionChanged'));
          }
          await writeFile(join(folder, 'forced.txt'), (await workbench.execute(command)).text);
        }
        finally { await workbench.close(); }
        execution = { owner: procedural.owner, command, candidate: procedural.row.candidate };
        adoptedAction = procedural.row.candidate.action;
        result.forcedActions++;
      } else if (cardFrame?.route.kind === 'automatic') {
        const workbench = await openWorkbench({ repository, session, binary, agentLocale,
          recordDirectory: join(folder, 'card-policy-wire'), commandPrefix: '' });
        try {
          const { adopted } = await adoptPreparationCardChoice(workbench, cardFrame, policyReading, {
            choice: cardFrame.route.commands[0], owner: 'host-card-policy', locale: agentLocale,
            record: row => appendFile(join(folder, 'adoptions.jsonl'), JSON.stringify(row) + '\n'),
          });
          execution = { owner: adopted.owner, command: adopted.command, route: adopted.route };
          adoptedAction = before.position.actions.find(row => strategicOptionCommand(row.semantic_action, before)
            === adopted.command)?.candidate?.action;
          display(launcherText(locale, 'cardPolicy', { command: adopted.command }));
          result.cardPolicyActions++;
        } finally { await workbench.close(); }
      } else {
        const player = await runPlayer({ ...options, repository, session, binary, agentLocale, home: join(folder, 'player'), taskScope: 'preparation',
          model: 'deepseek-flash', maxMs: options.maxMs ?? 180000, externalStopRequestFile: stopRequestFile, rewardProgress }, display);
        execution = { owner: cardFrame ? 'flash-card-choice' : 'flash-classification', player };
        result.modelTasks++;
        adoptedAction = before.position.actions.find(row => strategicOptionCommand(row.semantic_action, before)
          === player.preparationDecision?.command)?.candidate?.action;
        if (player.outcome !== 'completed' || player.preparationProgress !== 'complete') {
          result.outcome = 'preparation-needs-review'; result.player = player; needsReview = true;
        }
      }
      game = await openRunPlay({ repository, session, binary, agentLocale, recordDirectory: join(folder, 'after-wire') });
      let after;
      try { after = await game.observe(); } finally { await game.close(); }
      await writeFile(join(folder, 'after.json'), JSON.stringify(after, null, 2));
      rewardProgress = preparationRewardProgress(rewardProgress, before, after, adoptedAction);
      const temporary = `${rewardProgressFile}.tmp`;
      await writeFile(temporary, JSON.stringify(rewardProgress ?? null, null, 2));
      await rename(temporary, rewardProgressFile);
      await appendFile(join(output, 'journey.jsonl'), JSON.stringify({ step, before: gameContextSource(before), after: gameContextSource(after), execution }) + '\n');
      if (needsReview) break;
      result.steps++;
      if (gameContextSource(before).state_revision === gameContextSource(after).state_revision) { result.outcome = 'no-progress'; break; }
    }
  } catch (error) { result.outcome = 'error'; result.error = safeError(error); }
  await writeFile(join(output, 'result.json'), JSON.stringify(result, null, 2));
  display(launcherText(locale, 'flowResult', { outcome: result.outcome, modelTasks: result.modelTasks, cardPolicyActions: result.cardPolicyActions, combatSearches: result.combatSearches, combatVictories: result.combatVictories, combatEscapes: result.combatEscapes, output: output }));
  return result;
}

async function main() {
  const { values } = parseArgs({ options: {
    repository: { type: 'string' }, session: { type: 'string' }, home: { type: 'string' }, binary: { type: 'string' },
    resume: { type: 'string' }, model: { type: 'string' }, reasoning: { type: 'string' },
    'task-scope': { type: 'string' },
    'agent-locale': { type: 'string' }, locale: { type: 'string' },
    supervised: { type: 'boolean' },
    'max-tokens': { type: 'string' }, 'max-ms': { type: 'string' },
    'flow-max-steps': { type: 'string' }, 'stop-floor': { type: 'string' },
    task: { type: 'string' }, 'task-file': { type: 'string' }, dry: { type: 'boolean' }, help: { type: 'boolean' },
  } });
  const locale = values.locale ?? 'en';
  if (!['en', 'zh-CN'].includes(locale)) throw new Error(launcherText('en', 'invalidLocale'));
  if (values.help) {
    console.log(launcherText(locale, 'usage'));
    return;
  }
  if (!values.repository || !values.session) throw new Error(launcherText(locale, 'pathsRequired'));
  if (values.task !== undefined && values['task-file'] !== undefined) throw new Error(launcherText(locale, 'exclusiveTask'));
  const runner = values['task-scope'] === 'preparation-flow' ? runPreparationFlow : runPlayer;
  const result = await runner({ ...values, agentLocale: values['agent-locale'], taskScope: values['task-scope'], taskFile: values['task-file'], maxTokens: values['max-tokens'], maxMs: values['max-ms'],
    flowMaxSteps: values['flow-max-steps'], stopFloor: values['stop-floor'] });
  if (!['completed', 'dry-setup-passed', 'checkpoint', 'step-limit', 'terminal', 'needs-review'].includes(result.outcome)) process.exitCode = 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch(error => { console.error(safeError(error)); process.exitCode = 1; });
}
