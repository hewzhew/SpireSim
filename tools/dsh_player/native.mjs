// The official dsh preset owns conversation, compaction and model execution.
// This scoped capability only binds each chat to its own Rust game/workbench.
import { mkdir, readFile, writeFile, stat, realpath, appendFile } from 'node:fs/promises';
import { join, resolve, isAbsolute } from 'node:path';
import { openWorkbench as openSharedWorkbench } from '@sts-simulator/run-play';
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

export function nativeGuide(manual = false) {
  return `You play Slay the Spire through sts. Each chat has its own Ironclad Ascension 20 Heart run.
${manual ? 'Play mode: public-information manual play, with retained free returns. Search, private previews and exact hidden pile order are unavailable. Make decisions from public facts and actual results.' : 'Play mode: search-assisted play.'}
Begin with sts read: it creates a fresh run for this chat or reopens this chat's saved game. Each action returns its successor; use that state and its legal options for your next action. When a game fact or command meaning needed for your decision is unclear, briefly state what you need to clarify, query it, and use the answer for your next choice. Use read to recover missing state or a failed connection. Chat history and compaction are managed by dsh; room changes do not erase this conversation.
Put the complete command before | into sts.command, without adding a verb: {"command":"event:Neow:0"} chooses that event option. Discovery uses choose:<card>; pile selections use act select:<card>, exactly as printed. ${manual ? 'act takes semantic actions, never row numbers or ordinal:<...>. For example, act play:Inflame#0, play:ShrugItOff#0 executes both. After drawing or generating a card, use the actual returned hand before choosing an unknown continuation.' : 'act and preview take semantic actions, never row numbers or ordinal:<...>. Their action line can use spaces or comma-space: preview play:Inflame#0, play:ShrugItOff#0 previews both without applying either. A query or another explicit command ends that action line; preview event:CursedTome:0, act event:CursedTome:0 previews then applies it.'}
info expands the current event; info M0 reads the actual enemy in slot 0. Named mechanisms use info card/enemy/encounter/power/relic/potion <name>; use info <name> if its category is uncertain. info keys reads actual held keys and acquisition rules. inventory [effects] reads the permanent deck and owned resources; map reads this game's public map and Boss. Its printed routes follow connected rooms on that map; map full shows every published node and connection. options expands the current legal commands, including hidden shop removals and exact combat plays; use it rather than inventing a command. Copy command IDs even when display names differ. Use ASCII + for upgrades; +0 explicitly selects an unupgraded card.
Hand costs are current instance costs. Hand damage is per hit after Strength, Weak and the listed target's Vulnerable, before enemy Block; those modifiers are already included. Displayed combat intent damage includes current damage modifiers and are per hit; multiply by the displayed hit count. Block, on-play triggers and turn-end effects affect HP separately. ${manual ? 'Check these public effects when planning, then inspect the actual successor.' : 'Use preview for the actual proposed sequence. A failed or lethal preview rules out that line, not every alternative. Search potion expenditure counts additional spending from the displayed search start.'} Printed card rules describe base effects and can differ from a generated card's current cost.
piles reads the current combat's unordered draw, discard and exhaust card sets. inventory reads the permanent deck; cards exhausted in this fight remain in that permanent list but cannot be redrawn normally this fight.
For deck, upgrade, shop, event and route choices, compare the current deck/resources and the next serious threat with the strongest alternative, including skip or leave where available. Count actual copies; distinguish existing effects from an upgrade's increment and conditional effects from immediately available ones.
${manual ? 'Use act for actual card and potion plays. If a tried line needs revision, back returns to a retained earlier decision and restores its original RNG. Use mark <name> to retain an important start and back mark <name> to return. Attempts and returns remain visible; report any intervention. Do not use a failed transaction to probe unknown cards.' : `When preparation depends on how this deck performs against a named encounter, deck-lab list shows available encounters. For example, deck-lab Hexaghost scenarios 2 work 200000 ms 8000 potions 2 tests the current resources privately; append after <current legal preparation actions> to compare an upgrade, rest, purchase or card choice. Add details before after to include full turn traces in that diagnostic run. Keep the same encounter, seeds, budgets and potion limit for the baseline and alternative. These are fixed diagnostic shuffles, not the actual future or a win-rate estimate; zero retained wins leaves other lines unknown. Actual draws, energy and card movement matter: do not assume a permanent-deck card can be played every turn.
If read lists saved combat wins, they are already replayed from this root; use plans/review/commit without rerunning research. At a combat start, a visible immediate kill can be played directly. Otherwise use research whole_combat 8000 work 200000 (milliseconds 1..20000, work 1..1000000). Each retained win has already been replayed from this exact position and RNG, including generated choices. Copy the complete plan ID printed by research/plans, including its @ report suffix. review <plan ID> [turn] only reads it; commit <plan ID> executes the complete saved combat line to its displayed outcome, with no new search or random reroll. Compare settled HP, minimum observed HP and additional potion spending. A replayed win is a checked baseline; it does not need to be reconstructed turn by turn. If it is expensive, preview one concrete alternative, then adopt a checked line or prefix and search its continuation when useful. If no win is retained at a player-turn boundary without a pending selection, preview end as a baseline, then compare a concrete card or potion line. Playing additional cards need not improve survival. Manual actions make the old search root stale. Budget exhaustion leaves other possibilities unknown.
preview <semantic actions> privately tests combat or supported strategic choices, including events: preview event:Neow:3. If an action cannot resolve, it returns the private boundary reached before that action, including its hand and current costs; use this to repair the line. If it reaches a terminal early, it reports the actual outcome and unsimulated tail. These partial results do not validate the whole requested line. Every preview and act starts from the actual current position; read/options also refer to that actual position, not a prior preview. The displayed go x,y also works inside act or preview, with explicit coordinates; it selects that route without automatically finishing other room choices. A syntax error does not establish that preview is unavailable. research, plans, review and commit are combat-only; plans reads an existing search report. piles sl explicitly reads exact combat piles.`}
back, jump and mark navigate this chat's saved game only; follow the user's rewind instructions. Collect all three Heart keys before leaving Act 3; missing keys end the run there even when both bosses are defeated. Stop at the user's requested checkpoint and report actual resources and unresolved concerns.
The user can pause and return the game through the workspace panel. Explicit continuation supplies a checked actual-state read and any new return receipt. Reuse options only when bound to that current state. A return restores the saved RNG, not a new random draw. Cancellation may leave an in-flight game action completed. Use sts_feedback for a specific tool problem. Source maintenance belongs to the developer.`;
}
export const NATIVE_GUIDE = nativeGuide();

const executeFile = promisify(execFile);
const bindingSchema = 'sts-dsh-native-binding-v1';

async function bindingLocation(agent, config) {
  const root = config.repository ?? agent.session.header.cwd;
  if (typeof root !== 'string' || !isAbsolute(root)) throw new Error('Select the sts_simulator repository as this chat workspace.');
  const repository = await realpath(root);
  const id = String(agent.id);
  const digest = createHash('sha256').update(id).digest('hex').slice(0, 24);
  const session = join(repository, '.oracle-lab/run-play', `dsh-native-${digest}`);
  const file = join(session, 'dsh-native.json');
  return { repository, id, session, file };
}

async function readBinding({ repository, id, session, file }) {
  let saved;
  try { saved = JSON.parse(await readFile(file, 'utf8')); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (saved) {
    if (saved.schema !== bindingSchema || saved.agent !== id || resolve(saved.repository) !== repository
        || resolve(saved.session) !== session || (saved.manual !== undefined && typeof saved.manual !== 'boolean')
        || (saved.agentLocale !== undefined && !['en', 'zh-CN'].includes(saved.agentLocale))) {
      throw new Error('This chat game binding is invalid; the existing game was not changed.');
    }
  }
  return saved ?? null;
}

// Observation needs the chat's source binding, even without a game executable.
export async function readNativeBinding(agent, config = {}) {
  return readBinding(await bindingLocation(agent, config));
}

export async function nativeBinding(agent, config = {}, { run = executeFile, create = true } = {}) {
  if (config.agentLocale !== undefined && !['en', 'zh-CN'].includes(config.agentLocale)) throw new Error('Unsupported agentLocale; use en or zh-CN');
  const location = await bindingLocation(agent, config);
  const { repository, id, session, file } = location;
  const saved = await readBinding(location);
  if (saved && config.manual !== undefined && config.manual !== (saved.manual ?? false)) {
    throw new Error('This chat has a different play mode. Create a new chat for the requested mode.');
  }
  if (saved && config.agentLocale !== undefined && config.agentLocale !== (saved.agentLocale ?? 'en')) {
    throw new Error('This chat has a different agent language. Create a new chat for the requested language.');
  }
  if (!saved && !create) return null;
  if (config.binary && !isAbsolute(config.binary)) throw new Error('Configured game binary must be an absolute path.');
  const binary = await realpath(config.binary ?? join(repository, 'target/dev-opt/run_play.exe'));
  if (saved) return { ...saved, binary };
  try { await stat(session); throw new Error('A game directory exists without this chat binding; it was not overwritten.'); }
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
      if (!control) throw new Error('This chat has no native game owner.');
      let result;
      try {
        const request = method === 'restore' || method === 'budget' ? JSON.parse(rawInput) : undefined;
        if (method !== 'restore' && method !== 'budget' && rawInput.trim()) throw new Error('This command takes no arguments.');
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
    if (agent[controlKey]) throw new Error('This chat already has a game owner.');
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
      const reason = nativeBudgetStopReason(this.budget, this.admittedRequests, usage);
      if (reason) { this.stopForBudget(reason); payload.signal.throwIfAborted(); }
      const config = await next();
      payload.signal.throwIfAborted();
      // Request preparation may have awaited another hook past the deadline.
      const lateReason = nativeBudgetStopReason(this.budget, this.admittedRequests, usage);
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
  scheduleBudgetDeadline() {
    clearTimeout(this.budgetTimer);
    if (!this.budget?.deadlineAt || this.closed) return;
    const delay = Math.max(0, Date.parse(this.budget.deadlineAt) - Date.now());
    this.budgetTimer = setTimeout(() => {
      if (Date.now() < Date.parse(this.budget.deadlineAt)) this.scheduleBudgetDeadline();
      else this.stopForBudget('deadline reached');
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
    if (this.agent.status === 'running') throw new Error('Pause the player before configuring its experiment budget.');
    const limits = validateNativeBudget(value);
    const totals = this.agent.ctx.sessionProjections.checkpoint(this.agent.session).tokenUsage?.val?.totals;
    if (limits?.maxReportedTokens && !totals) throw new Error('The official token meter is required for a reported-token budget.');
    this.budget = limits ? { ...limits, baselineSeq: this.agent.session.snapshotEvents().at(-1)?.seq ?? -1,
      baselineTokens: totals ? totals.uncachedInputTokens + totals.outputTokens : 0 } : null;
    this.admittedRequests = 0;
    this.budgetStop = undefined;
    this.scheduleBudgetDeadline();
    return { status: 'ready', budget: this.budget, message: limits ? 'Experiment budget configured. It does not resume the player.'
      : 'Experiment budget cleared. The player remains in its current pause state.', control: this.status() };
  }
  serial(operation) {
    if (this.closed) return Promise.reject(new Error('This chat game connection was closed.'));
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
      if (!this.binding) throw new Error('This chat has no game.');
      const { repository, session, binary } = this.binding;
      this.workbench = await this.openWorkbench({ repository, session, binary, manual: this.binding.manual ?? this.config.manual ?? false,
        agentLocale: this.binding.agentLocale ?? this.config.agentLocale ?? 'en', commandPrefix: '' });
      return this.workbench;
    })();
    try { return await this.opening; }
    catch (error) { this.opening = undefined; throw error; }
  }
  stop() {
    if (this.closed) throw new Error('This chat game connection was closed.');
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
      if (this.paused) throw new Error('The game is paused in the workspace.');
      const game = await this.game(true);
      signal.throwIfAborted();
      if (this.paused) throw new Error('The game is paused in the workspace.');
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
      throw new Error('The game changed during the current-state read. Read it again before continuing.');
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
    if (!execution) throw new Error('The native workspace commands are unavailable.');
    const data = JSON.parse(execution.result.text);
    if (data.schema !== workspaceSchema) throw new Error('The workspace command returned an unexpected result.');
    return data.result;
  }
  pause() { return this.invoke('sts-pause'); }
  restore(request) { return this.invoke('sts-return', request); }
  resume() { return this.invoke('sts-resume'); }
  _pause() {
    return this.pausedOperation(async () => {
      const game = await this.existingGame();
      const message = this.budgetStop ? { message: 'Experiment stopped: ' + this.budgetStop.reason } : {};
      if (!game) return { status: 'ready', text: 'Paused. No game has been created for this chat.', ...message, control: this.status() };
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
      if (!this.paused) throw new Error('The game is not paused.');
      return this.maintenance(async signal => {
        const game = await this.existingGame();
        const text = game ? await this.readCurrent(game) : 'No game has been created for this chat. Begin with sts read to create this chat’s run.';
        signal.throwIfAborted();
        const facts = workspaceFacts(this.agent.session.snapshotEvents());
        const resumedThrough = facts.findLast(fact => fact.kind === 'resumed')?.seq ?? -1;
        const lastReturn = facts.findLast(fact => fact.receipt && fact.seq > resumedThrough);
        const context = `The user explicitly resumed this game from the workspace. Follow pending user instructions. This is a checked current-state read. Reuse ${this.config.manual ? 'options' : 'options and candidates'} only when bound to this current state.${this.config.manual ? ' This is public-information manual play.' : ' Pausing alone does not invalidate unchanged-root saved wins. Any saved combat wins listed below have been checked against this root.'}${lastReturn ? '\nLatest new return receipt (the saved RNG was restored, not rerolled): ' + JSON.stringify(lastReturn.receipt) : ''}\n\n${text}`;
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
      return 'Feedback recorded; the game was not advanced.';
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
    if (disposed) throw new Error('The game plugin is disabled.');
    if (!agent) throw new Error('Game operations require an initiating chat.');
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
    controlFor(agent, manual);
    // Preset tools are inherited by the Agent, so restrict on the Agent's
    // own scope rather than filtering the preset's descendant tools away.
    await agent.ctx.plugin({ inject: ['tools', 'systemPrompt'], apply(child) {
      child.tools.restrict({ allow: ['sts', 'sts_feedback'] });
      child.systemPrompt.section({ name: 'sts:native-guide', order: 300, text: nativeGuide(manual), interpolate: false, complete: true });
    } });
  });
  registerNativeCommands(ctx);
  ctx.effect(() => () => {
    disposed = true;
    return Promise.all([...controls.values()].map(control => control.dispose()));
  }, 'sts-native.sessions');
  ctx.tools.presentAs('native');
  ctx.tools.register(defineTool({
    name: 'sts',
    description: 'Operate this chat’s A20 Heart game in its declared play mode. Start with read. Copy the complete command before | into command, e.g. event:Neow:0. act executes real actions; info queries mechanics. Each action returns its actual successor.',
    parameters: { command: { type: 'string', required: true, description: 'A complete displayed command or query; do not prepend choose to an event command. Comma-space separates complete commands.' } },
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
    description: 'Record a specific missing game fact or tool problem for the developer.',
    parameters: { message: { type: 'string', required: true, description: 'What happened and what information or behavior was needed.' } },
    output: { schema: { type: 'string' }, render: (_args, value) => [{ type: 'text', text: value }] },
    async execute({ message }, exec) {
      if (!message.trim()) throw new Error('Describe a concrete problem.');
      return controlFor(exec.agent).feedback(message, exec.signal);
    },
  }));
}
