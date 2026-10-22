// The official dsh preset owns conversation, compaction and model execution.
// This scoped capability only binds each chat to its own Rust game/workbench.
import { mkdir, readFile, writeFile, stat, realpath, appendFile } from 'node:fs/promises';
import { join, resolve, isAbsolute } from 'node:path';
import { openWorkbench as openSharedWorkbench } from '@sts-simulator/run-play';
import { createPresentation } from '@sts-simulator/presentation';
import { createHash, randomBytes } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import z from '@deepseek-ai/schemastery';
import { defineTool } from '@deepseek-ai/dsh-tools';
import { createUserMessage } from '@deepseek-ai/dsh-llm';
import { appendPluginRecord } from '@deepseek-ai/dsh-session';
import { validateNativeBudget, nativeBudgetRecord, nativeBudgetUsage, nativeBudgetStopReason } from './native-budget.mjs';
import { recordSessionRequests } from './inspection.mjs';

export const name = 'sts-native-player';
export const inject = ['tools', 'systemPrompt', 'commands'];
export const Config = z.object({
  repository: z.string().required(false),
  binary: z.string().required(false),
  manual: z.boolean().required(false),
  agentLocale: z.union(['en', 'zh-CN']).required(false),
});

export function nativeGuide(manual = false, locale = 'en') {
  const presentation = createPresentation({ locale });
  const text = key => presentation.text(`native:guide.${key}`);
  return [text('opening'), text(manual ? 'modeManual' : 'modeAssisted'), text('begin'),
    `${text('commands')} ${text(manual ? 'syntaxManual' : 'syntaxAssisted')}`, text('info'),
    `${text('combat')} ${text(manual ? 'combatManual' : 'combatAssisted')} ${text('printed')}`,
    text('piles'), text('preparation'),
    ...(manual ? [text('manualPlay')] : [text('deckLab'), text('research'), text('preview')]),
    text('returns'), text('workspace')].join('\n');
}
export const NATIVE_GUIDE = nativeGuide();

const executeFile = promisify(execFile);
const bindingSchema = 'sts-dsh-native-binding-v1';

function nativeMessage(locale, key, parameters) {
  return createPresentation({ locale: locale === 'zh-CN' ? 'zh-CN' : 'en' }).text(`native:message.${key}`, parameters);
}

async function bindingLocation(agent, config) {
  const root = config.repository ?? agent.session.header.cwd;
  if (typeof root !== 'string' || !isAbsolute(root)) throw new Error(nativeMessage(config.agentLocale, 'selectWorkspace'));
  const repository = await realpath(root);
  const id = String(agent.id);
  const digest = createHash('sha256').update(id).digest('hex').slice(0, 24);
  const session = join(repository, '.oracle-lab/run-play', `dsh-native-${digest}`);
  const file = join(session, 'dsh-native.json');
  return { repository, id, session, file, agentLocale: config.agentLocale };
}

async function readBinding({ repository, id, session, file, agentLocale }) {
  let saved;
  try { saved = JSON.parse(await readFile(file, 'utf8')); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (saved) {
    if (saved.schema !== bindingSchema || saved.agent !== id || resolve(saved.repository) !== repository
        || resolve(saved.session) !== session || (saved.manual !== undefined && typeof saved.manual !== 'boolean')
        || (saved.agentLocale !== undefined && !['en', 'zh-CN'].includes(saved.agentLocale))) {
      throw new Error(nativeMessage(agentLocale ?? saved.agentLocale, 'invalidBinding'));
    }
  }
  return saved ?? null;
}

// Observation needs the chat's source binding, even without a game executable.
export async function readNativeBinding(agent, config = {}) {
  return readBinding(await bindingLocation(agent, config));
}

export async function nativeBinding(agent, config = {}, { run = executeFile, create = true } = {}) {
  if (config.agentLocale !== undefined && !['en', 'zh-CN'].includes(config.agentLocale)) throw new Error(nativeMessage(config.agentLocale, 'unsupportedLocale'));
  const location = await bindingLocation(agent, config);
  const { repository, id, session, file } = location;
  const saved = await readBinding(location);
  const text = key => nativeMessage(config.agentLocale ?? saved?.agentLocale, key);
  if (saved && config.manual !== undefined && config.manual !== (saved.manual ?? false)) {
    throw new Error(text('differentMode'));
  }
  if (saved && config.agentLocale !== undefined && config.agentLocale !== (saved.agentLocale ?? 'en')) {
    throw new Error(text('differentLocale'));
  }
  if (!saved && !create) return null;
  if (config.binary && !isAbsolute(config.binary)) throw new Error(text('absoluteBinary'));
  const binary = await realpath(config.binary ?? join(repository, 'target/dev-opt/run_play.exe'));
  if (saved) return { ...saved, binary };
  try { await stat(session); throw new Error(text('unownedDirectory')); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  const seed = randomBytes(8).readBigUInt64LE().toString();
  await mkdir(join(repository, '.oracle-lab/run-play'), { recursive: true });
  await run(binary, ['new', '--session', session, '--seed', seed, '--ascension', '20', '--goal', 'heart-victory'],
    { cwd: repository, windowsHide: true, timeout: 20000, maxBuffer: 1024 * 1024 });
  const binding = { schema: bindingSchema, agent: id, repository, session, seed, ascension: 20, goal: 'heart-victory', manual: config.manual ?? false, agentLocale: config.agentLocale ?? 'en' };
  await writeFile(file, JSON.stringify(binding, null, 2) + '\n', { flag: 'wx' });
  return { ...binding, binary };
}

// The retained preset generation owns this capability on its exact Agent.
// The global symbol lets a reloaded viewer find that owner without a new writer.
const controlKey = Symbol.for('@sts-simulator/dsh-player/native-game-control');
const workspaceNames = new Set(['sts-pause', 'sts-return', 'sts-resume', 'sts-budget']);
const workspaceSchema = 'sts-native-workspace-v1';
function commandData(event) {
  if (event.type !== 'command/done' || typeof event.data.text !== 'string') return null;
  try {
    const data = JSON.parse(event.data.text);
    return data.schema === workspaceSchema && typeof data.paused === 'boolean' && data.result ? data : null;
  }
  catch { return null; }
}
export const isWorkspaceEvent = event => event.type === 'command/run' && workspaceNames.has(event.data.name) || Boolean(commandData(event));
export function workspaceFacts(events) {
  const commands = new Map(), facts = [];
  for (const event of events) {
    if (event.type === 'command/run' && workspaceNames.has(event.data.name)) {
      commands.set(event.data.commandId, event.data.name);
      if (event.data.name === 'sts-pause' || event.data.name === 'sts-return') facts.push({ seq: event.seq, time: event.time,
        kind: event.data.name === 'sts-return' ? 'return-requested' : 'pause-requested', paused: true });
    } else if (commands.has(event.data?.commandId)) {
      const data = commandData(event);
      if (data) facts.push({ seq: event.seq, time: event.time, kind: data.kind, paused: data.paused,
        ...(data.result.receipt ? { receipt: data.result.receipt } : {}),
        ...(data.result.message ? { message: data.result.message } : {}) });
    }
  }
  return facts;
}
export const workspaceState = events => workspaceFacts(events).at(-1)?.paused ?? false;
export function nativeGameControl(agent) { return agent?.[controlKey] ?? null; }

// Use the official human-command lifecycle. Unknown custom required events are
// rejected by persistence; command/run + command/done reconstruct without a
// plugin-specific log format or model-surface message.
export function registerNativeCommands(ctx) {
  const operations = { 'sts-pause': ['pause', '暂停对局，保留排队消息'],
    'sts-return': ['restore', '使用对局工作台中的完整来源与目标返回'], 'sts-resume': ['resume', '从实际局面继续运行'],
    'sts-budget': ['budget', '设置原生实验的请求次数、用量阈值、单次输出与截止时间；null 清除'] };
  for (const [name, [method, description]] of Object.entries(operations)) ctx.commands.register({
    name, description,
    async handler({ agent, rawInput }) {
      const control = nativeGameControl(agent);
      if (!control) throw new Error(nativeMessage(undefined, 'noOwner'));
      let result;
      try {
        const request = method === 'restore' || method === 'budget' ? JSON.parse(rawInput) : undefined;
        if (method !== 'restore' && method !== 'budget' && rawInput.trim()) throw new Error(control.text('message.noArguments'));
        result = await control['_' + method](request);
      } catch (error) { result = { status: 'error', message: error.message, control: control.status() }; }
      const kind = result.status === 'error' ? { pause: 'pause-failed', restore: 'return-failed', resume: 'resume-failed', budget: 'budget-failed' }[method]
        : { pause: 'paused', restore: 'returned', resume: 'resumed', budget: 'budget-configured' }[method];
      return { kind: result.status === 'error' ? 'error' : 'success',
        text: JSON.stringify({ schema: workspaceSchema, kind, paused: control.paused, result }) };
    },
  });
}

export class NativeGameControl {
  constructor(agent, config = {}, { binding = nativeBinding, openWorkbench = openSharedWorkbench, commands } = {}) {
    this.agent = agent;
    this.config = config;
    this.bind = binding;
    this.openWorkbench = openWorkbench;
    this.commands = commands;
    this.paused = workspaceState(agent.session.snapshotEvents());
    this.budget = nativeBudgetRecord(agent.session.snapshotEvents());
    this.admittedRequests = this.budget ? nativeBudgetUsage(agent.session.snapshotEvents(), this.budget).conservativeRequests : 0;
    this.tail = Promise.resolve();
    this.closed = false;
    if (agent[controlKey]) throw new Error(this.text('message.duplicateOwner'));
    // AgentLoop emits this synchronously before scheduling its driver. Cancel
    // before any inbox claim or model request, leaving queued input untouched.
    this.unlisten = agent.ctx.on('agent/status', ({ agent: subject, status }) => {
      if (subject === agent && status === 'running' && this.paused && !this.closed) {
        agent.cancel({ kind: 'user' }, { keepInbox: true });
      }
    });
    agent[controlKey] = this;
    this.unrequest = agent.ctx.on('agent/request', async (payload, next) => {
      if (payload.agent !== agent || !this.budget) return next();
      const usage = this.budgetUsage();
      const reason = nativeBudgetStopReason(this.budget, this.admittedRequests, usage, Date.now(), this.locale);
      if (reason) { this.stopForBudget(reason); payload.signal.throwIfAborted(); }
      const config = await next();
      payload.signal.throwIfAborted();
      // Request preparation may have awaited another hook past the deadline.
      const lateReason = nativeBudgetStopReason(this.budget, this.admittedRequests, usage, Date.now(), this.locale);
      if (lateReason) { this.stopForBudget(lateReason); payload.signal.throwIfAborted(); }
      this.admittedRequests++;
      // dsh persists the resolved call config as the next request's proposal.
      // A previous experiment limit is therefore not an independent ceiling.
      return this.budget.maxTokensPerRequest ? { ...config,
        maxTokens: this.budget.maxTokensPerRequest } : config;
    });
    this.unrecord = recordSessionRequests(agent.ctx, { id: String(agent.id), session: () => agent.session,
      record: async boundary => {
        appendPluginRecord(agent.session, 'plugin:sts/request-boundary', { schema: 'sts-native-request-boundary-v1',
          sessionId: String(agent.id), ...boundary });
        await agent.ctx.sessions.flush(agent.session);
      } });
    this.scheduleBudgetDeadline();
  }
  status() {
    return { paused: this.paused, running: this.agent.status === 'running',
      queued: this.agent.inbox.nextTurn.length + this.agent.inbox.nextStep.length,
      ...(this.budget ? { budget: { ...this.budget, admittedRequests: this.admittedRequests,
        ...this.budgetUsage() } } : {}),
      ...(this.budgetStop ? { budgetStop: this.budgetStop } : {}) };
  }
  budgetUsage() {
    return nativeBudgetUsage(this.agent.session.snapshotEvents(), this.budget,
      this.agent.ctx.sessionProjections.checkpoint(this.agent.session).tokenUsage?.val);
  }
  get locale() { return this.binding?.agentLocale ?? this.agentLocale ?? this.config.agentLocale ?? 'en'; }
  text(key, parameters) {
    const locale = this.locale;
    if (this.presentation?.locale !== locale) this.presentation = createPresentation({ locale });
    return this.presentation.text(`native:${key}`, parameters);
  }
  scheduleBudgetDeadline() {
    clearTimeout(this.budgetTimer);
    if (!this.budget?.deadlineAt || this.closed) return;
    const delay = Math.max(0, Date.parse(this.budget.deadlineAt) - Date.now());
    this.budgetTimer = setTimeout(() => {
      if (Date.now() < Date.parse(this.budget.deadlineAt)) this.scheduleBudgetDeadline();
      else this.stopForBudget(this.text('budget.deadline'));
    }, Math.min(delay, 2147483647));
    this.budgetTimer.unref?.();
  }
  stopForBudget(reason) {
    if (this.closed || (this.budgetStop && this.paused)) return;
    this.budgetStop = { reason, at: new Date().toISOString(), admittedRequests: this.admittedRequests };
    const stop = this.budgetStop;
    this.stop();
    // Do not await pause from inside the request hook: pause waits for that
    // same driver to become idle. The official command persists the pause.
    this.budgetPause = this.pause().then(result => {
      if (result.status === 'error') stop.pauseError = result.message;
    }).catch(error => { stop.pauseError = error.message; });
  }
  _budget(value) {
    if (this.agent.status === 'running') throw new Error(this.text('message.pauseBeforeBudget'));
    const limits = validateNativeBudget(value, this.locale);
    const totals = this.agent.ctx.sessionProjections.checkpoint(this.agent.session).tokenUsage?.val?.totals;
    if (limits?.maxReportedTokens && !totals) throw new Error(this.text('message.meterRequired'));
    this.budget = limits ? { ...limits, baselineSeq: this.agent.session.snapshotEvents().at(-1)?.seq ?? -1,
      baselineTokens: totals ? totals.uncachedInputTokens + totals.outputTokens : 0 } : null;
    this.admittedRequests = 0;
    this.budgetStop = undefined;
    this.scheduleBudgetDeadline();
    return { status: 'ready', budget: this.budget, message: limits ? this.text('message.budgetConfigured')
      : this.text('message.budgetCleared'), control: this.status() };
  }
  serial(operation) {
    if (this.closed) return Promise.reject(new Error(this.text('message.closed')));
    const pending = this.tail.then(async () => {
      try { return await operation(); }
      finally {
        // A paused player needs durable state, not a resident executable.
        // Release it after the serialized operation settles; resume and reads
        // reconnect through this same owner without changing the game/RNG.
        if (this.paused && this.workbench) {
          const game = this.workbench;
          this.workbench = undefined;
          this.opening = undefined;
          await game.close();
        }
      }
    });
    this.tail = pending.catch(() => {});
    return pending;
  }
  async game(create = false) {
    this.opening ??= (async () => {
      this.binding = await this.bind(this.agent, this.config, { create });
      if (!this.binding) throw new Error(this.text('message.noGame'));
      const { repository, session, binary } = this.binding;
      this.workbench = await this.openWorkbench({ repository, session, binary, manual: this.binding.manual ?? this.config.manual ?? false,
        agentLocale: this.binding.agentLocale ?? this.config.agentLocale ?? 'en', commandPrefix: '' });
      return this.workbench;
    })();
    try { return await this.opening; }
    catch (error) { this.opening = undefined; throw error; }
  }
  stop() {
    if (this.closed) throw new Error(this.text('message.closed'));
    this.paused = true;
    this.agent.cancel({ kind: 'user' }, { keepInbox: true });
  }
  async maintenance(operation) {
    // A newly queued wake is cancelled by the status listener. Wait for that
    // driver to converge too, then acquire the official maintenance phase.
    while (true) {
      await this.agent.whenIdle();
      let acquired = false;
      try {
        return await this.agent.runMaintenance(async signal => {
          acquired = true;
          signal.throwIfAborted();
          return operation(signal);
        });
      } catch (error) {
        if (acquired || this.agent.status !== 'running') throw error;
      }
    }
  }
  execute(command, signal) {
    return this.serial(async () => {
      signal.throwIfAborted();
      if (this.paused) throw new Error(this.text('message.paused'));
      const game = await this.game(true);
      signal.throwIfAborted();
      if (this.paused) throw new Error(this.text('message.paused'));
      const result = await game.execute(command, { signal });
      return { text: result.text, game: { agent: this.binding.agent, session: this.binding.session, seed: this.binding.seed,
        ...(result.currentReading ? { currentReading: result.currentReading } : {}) } };
    });
  }
  points() { return this.serial(async () => ({ points: await (await this.game()).returnPoints(), control: this.status() })); }
  async readCurrent(game) {
    const before = await game.observe();
    const text = (await game.execute(before.position.kind === 'terminal' ? 'read' : 'read, inventory')).text;
    const after = await game.observe();
    if (before.head_revision !== after.head_revision || before.state_revision !== after.state_revision
        || before.position.public_snapshot_id !== after.position.public_snapshot_id) {
      throw new Error(this.text('message.readChanged'));
    }
    return text;
  }
  live() { return this.serial(async () => this.readCurrent(await this.game())); }
  async existingGame() {
    if (!this.opening && !await this.bind(this.agent, this.config, { create: false })) return null;
    return this.game();
  }
  pausedOperation(operation) {
    this.stop();
    return this.serial(() => {
      this.stop();
      return this.maintenance(operation);
    });
  }
  async invoke(name, request) {
    const execution = await this.commands.execute(this.agent, '/' + name + (request ? ' ' + JSON.stringify(request) : ''), [], new AbortController().signal);
    if (!execution) throw new Error(this.text('message.commandsUnavailable'));
    const data = JSON.parse(execution.result.text);
    if (data.schema !== workspaceSchema) throw new Error(this.text('message.unexpectedResult'));
    return data.result;
  }
  pause() { return this.invoke('sts-pause'); }
  restore(request) { return this.invoke('sts-return', request); }
  resume() { return this.invoke('sts-resume'); }
  _pause() {
    return this.pausedOperation(async () => {
      const game = await this.existingGame();
      const message = this.budgetStop ? { message: this.text('message.experimentStopped', { reason: this.budgetStop.reason }) } : {};
      if (!game) return { status: 'ready', text: this.text('message.pausedNoGame'), ...message, control: this.status() };
      const text = await this.readCurrent(game);
      return { status: 'ready', text, ...message, points: await game.returnPoints(), control: this.status() };
    });
  }
  _restore(request) {
    return this.pausedOperation(async () => {
      const game = await this.game();
      let receipt;
      try {
        const restored = await game.restorePoint(request);
        receipt = restored.receipt;
        const text = await this.readCurrent(game);
        return { status: 'ready', receipt, points: await game.returnPoints(), text, control: this.status() };
      } catch (error) {
        receipt ??= error.receipt;
        return { status: 'error', message: error.message, ...(receipt ? { receipt } : {}), control: this.status() };
      }
    });
  }
  _resume() {
    return this.serial(() => {
      if (!this.paused) throw new Error(this.text('message.notPaused'));
      return this.maintenance(async signal => {
        const game = await this.existingGame();
        const text = game ? await this.readCurrent(game) : this.text('resume.noGame');
        signal.throwIfAborted();
        const facts = workspaceFacts(this.agent.session.snapshotEvents());
        const resumedThrough = facts.findLast(fact => fact.kind === 'resumed')?.seq ?? -1;
        const lastReturn = facts.findLast(fact => fact.receipt && fact.seq > resumedThrough);
        const manual = this.binding?.manual ?? this.config.manual ?? false;
        const context = this.text('resume.lead', { reuse: this.text(manual ? 'resume.reuseManual' : 'resume.reuseAssisted') })
          + this.text(manual ? 'resume.manual' : 'resume.assisted')
          + (lastReturn ? this.text('resume.receipt', { receipt: JSON.stringify(lastReturn.receipt) }) : '') + '\n\n' + text;
        this.paused = false;
        try {
          this.agent.send(createUserMessage({ source: { kind: 'sts-workspace' }, content: [{ type: 'text', text: context }] }), 'next-step', true);
        } catch (error) {
          this.paused = true;
          throw error;
        }
        return { status: 'ready', text, control: this.status() };
      });
    });
  }
  feedback(message, signal) {
    return this.serial(async () => {
      signal.throwIfAborted();
      await this.game(true);
      await appendFile(join(this.binding.session, 'native-feedback.jsonl'), JSON.stringify({ at: new Date().toISOString(),
        agent: this.binding.agent, message }) + '\n');
      return this.text('message.feedbackRecorded');
    });
  }
  async close() {
    this.closed = true;
    this.unlisten();
    this.unrequest();
    this.unrecord();
    clearTimeout(this.budgetTimer);
    await this.tail;
    if (this.workbench) await this.workbench.close();
    if (this.agent[controlKey] === this) delete this.agent[controlKey];
  }
}

export async function apply(ctx, config) {
  const controls = new Map();
  let disposed = false;
  function controlFor(agent, manual = config.manual) {
    if (disposed) throw new Error(nativeMessage(config.agentLocale, 'disabled'));
    if (!agent) throw new Error(nativeMessage(config.agentLocale, 'requiresAgent'));
    let control = controls.get(agent);
    if (!control) {
      control = new NativeGameControl(agent, { ...config, manual }, { commands: ctx.commands });
      controls.set(agent, control);
      control.dispose = agent.ctx.effect(() => async () => { await control.close(); controls.delete(agent); }, 'sts-native.game');
    }
    return control;
  }
  ctx.on('agent/created', async ({ agent }) => {
    const binding = await readNativeBinding(agent, config);
    const manual = binding?.manual ?? config.manual ?? false;
    // Mask inherited host tools; each chat owns its localized game schemas.
    const locale = binding?.agentLocale ?? config.agentLocale ?? 'en';
    controlFor(agent, manual).agentLocale = locale;
    const capability = await agent.ctx.plugin({ inject: ['tools', 'systemPrompt'], apply(child) {
      child.tools.restrict({ allow: [] });
      registerNativeTools(child, controlFor, locale);
      child.systemPrompt.section({ name: 'sts:native-guide', order: 300, text: nativeGuide(manual, locale), interpolate: false, complete: true });
    } });
    ctx.effect(() => () => capability.dispose(), 'sts-native.capability');
  });
  registerNativeCommands(ctx);
  ctx.effect(() => () => {
    disposed = true;
    return Promise.all([...controls.values()].map(control => control.dispose()));
  }, 'sts-native.sessions');
  ctx.tools.presentAs('native');
}

function registerNativeTools(ctx, controlFor, locale) {
  const presentation = createPresentation({ locale });
  const text = key => presentation.text(`native:tools.${key}`);
  ctx.tools.register(defineTool({
    name: 'sts',
    description: text('sts'),
    parameters: { command: { type: 'string', required: true, description: text('command') } },
    output: { schema: { type: 'object', additionalProperties: false, properties: {
      text: { type: 'string', required: true }, game: { type: 'object', additionalProperties: true, required: true },
    } }, render: (_args, value) => [{ type: 'text', text: value.text }],
      presentationMeta: (_args, value) => ({ schema: 'sts-native-game-v1', ...value.game }) },
    async execute({ command }, exec) {
      return controlFor(exec.agent).execute(command, exec.signal);
    },
  }));
  ctx.tools.register(defineTool({
    name: 'sts_feedback',
    description: text('feedback'),
    parameters: { message: { type: 'string', required: true, description: text('feedbackMessage') } },
    output: { schema: { type: 'string' }, render: (_args, value) => [{ type: 'text', text: value }] },
    async execute({ message }, exec) {
      if (!message.trim()) throw new Error(text('feedbackRequired'));
      return controlFor(exec.agent).feedback(message, exec.signal);
    },
  }));
}
