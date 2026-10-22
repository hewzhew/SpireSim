// Game navigation has a semantic context boundary, independently of token pressure.
// The official backend owns surface replacement, tool pairing and journal records.
import BasicCompaction from '@deepseek-ai/dsh-compaction-basic';
import { toolPairingBalancedBefore, toolPairingBalancedAfter } from '@deepseek-ai/dsh-compaction';
import { createUserMessage } from '@deepseek-ai/dsh-llm';

const pending = new WeakMap();
const operators = new WeakMap();
export function requestBranchReset(agent) { pending.set(agent, { discardQueries: true }); }
export function hasPendingGameContextReset(agent) { return pending.has(agent); }
export function requestDecisionReset(agent, reading, source, { task, queries = [], queryRefs = [] } = {}) {
  if (typeof reading !== 'string' || !reading.trim()) throw new Error('game context reset requires a fresh actual reading');
  pending.set(agent, { ...pending.get(agent), reading, task, queries, queryRefs,
    ...(source ? { source: gameContextSource(source) } : {}) });
}

// Only explicit read-only definitions and route queries are eligible. Never
// replay a command chain, state dump, preview, search or player navigation.
export function contextQueryCommand(groups) {
  if (groups.length !== 1) return;
  const [verb, args] = groups[0];
  if (verb === 'info' && args.length >= 2
      && ['card', 'enemy', 'encounter', 'power', 'relic', 'potion'].includes(args[0])) return [verb, ...args].join(' ');
  if (verb === 'map' && (!args.length || ['full', 'elites', 'route'].includes(args[0]))) return [verb, ...args].join(' ');
}

export function decisionContextSource(session) {
  for (const seq of [...session.surface.nodes].reverse()) {
    const event = session.eventAt(seq);
    if (event?.type === 'user/message' && event.data.source?.kind === 'sts-decision') return event.data.source;
  }
}

export function decisionQueryReferences(agent, task, normalizeQuery = () => undefined) {
  if (pending.get(agent)?.discardQueries) return [];
  const { session } = agent, references = new Set();
  for (const seq of session.surface.nodes) {
    const event = session.eventAt(seq);
    if (event.type === 'tool/result') references.add(seq);
    if (event.type === 'user/message' && event.data.source?.kind === 'sts-decision'
        && sameGameDecisionTask(event.data.source.task, task)) {
      for (const reference of Array.isArray(event.data.source.queryRefs) ? event.data.source.queryRefs : []) references.add(reference);
    }
  }
  const commands = new Map();
  const end = session.snapshotEvents().length;
  for (const seq of [...references].filter(seq => Number.isSafeInteger(seq) && seq >= 0 && seq < end).sort((a, b) => a - b)) {
    const event = session.eventAt(seq), meta = event?.data?.meta;
    if (event?.type !== 'tool/result' || event.data.message.isError || meta?.schema !== 'sts-context-query-v1'
        || !sameGameDecisionTask(meta.task, task) || !meta.command || normalizeQuery(meta.command) !== meta.command) continue;
    commands.set(meta.command, { seq, command: meta.command });
  }
  return [...commands.values()];
}

// Group a complete investigation, not individual revisions or tool calls.
// Typed room context distinguishes a finished room's map from its rewards;
// nested card selections and reward pages stay with their initiating task.
export function gameDecisionTask(observation, previous) {
  const source = gameContextSource(observation);
  const position = observation.position;
  if (position?.kind === 'terminal') return { location: source.session_id, family: 'terminal', detail: '' };
  const visible = position?.observation;
  const state = visible?.observation;
  const run = visible?.domain === 'strategic' ? state?.run : visible?.run_context;
  if (position?.kind !== 'decision' || !Number.isInteger(run?.act) || !Number.isInteger(run?.floor)) {
    throw new Error('game decision boundary lacks typed run position');
  }
  const location = JSON.stringify([source.session_id, run.act, run.floor]);
  if (visible.domain === 'combat') return { location, family: 'combat', detail: run.encounter_id ?? '' };
  const context = state?.context;
  if (visible.domain !== 'strategic' || typeof context?.kind !== 'string') {
    throw new Error('game decision boundary lacks typed decision context');
  }
  if (previous?.location === location) {
    if (context.kind === 'run_choice' && !['map', 'combat', 'run_transition'].includes(previous.family)) return previous;
    if (['reward', 'card_reward'].includes(context.kind)
        && ['event', 'treasure', 'shop', 'campfire', 'boss_relic', 'rewards'].includes(previous.family)) return previous;
  }
  const family = ['reward', 'card_reward'].includes(context.kind) ? 'rewards' : context.kind;
  const detail = family === 'event' ? context.event
    : family === 'run_transition' ? JSON.stringify(context.stage)
      : family === 'run_choice' ? JSON.stringify([context.source, context.scope, context.reason]) : '';
  return { location, family, detail };
}

export function sameGameDecisionTask(previous, current) {
  return previous?.location === current.location && previous.family === current.family && previous.detail === current.detail;
}

// Only the simulator's observation envelope establishes continuity. Similar
// HP, cards, room labels or an identical-looking reading cannot establish it.
export function gameContextSource(observation) {
  const { schema, session_id, head_revision, state_revision } = observation ?? {};
  if (!/^sts-run-play-agent-observation-v\d+$/.test(schema ?? '')
      || typeof session_id !== 'string' || !session_id
      || !Number.isSafeInteger(head_revision) || head_revision < 0
      || !Number.isSafeInteger(state_revision) || state_revision < 0) {
    throw new Error('game observation lacks an authoritative context source');
  }
  return { schema, session_id, head_revision, state_revision };
}

export function sameGameContextSource(previous, current) {
  try {
    const before = gameContextSource(previous), after = gameContextSource(current);
    return Object.keys(before).every(key => before[key] === after[key]);
  } catch { return false; }
}

function operatorTask(agent) {
  let ledger = operators.get(agent);
  if (!ledger) { ledger = { cursor: -1, operator: null }; operators.set(agent, ledger); }
  const end = agent.session.surface.nodes.at(-1) ?? -1;
  // Recover only the human's task from durable events, including on resume.
  // Model conclusions and previous generated checkpoints cannot become it.
  for (let seq = ledger.cursor + 1; seq <= end; seq++) {
    const event = agent.session.eventAt(seq);
    if (event?.type === 'user/message' && event.data.source?.kind === 'user') ledger.operator = event.data;
  }
  ledger.cursor = end;
  return ledger.operator;
}

export function decisionContextRange(session) {
  const nodes = session.surface.nodes;
  if (!nodes.length) return null;
  if (!toolPairingBalancedAfter(session, nodes.at(-1))) {
    throw new Error('game decision context has an unfinished tool pair; refusing another model step');
  }
  const start = session.eventAt(nodes[0])?.type === 'system/message' ? 1 : 0;
  const history = nodes.slice(start);
  if (!history.some(seq => session.eventAt(seq)?.type === 'assistant/message')) return null;
  if (!toolPairingBalancedBefore(session, history[0])) {
    throw new Error('game decision context begins inside an unfinished tool pair');
  }
  return { start: history[0], end: history.at(-1) };
}

function decisionCheckpoint(messages, reading, { retainOperator = true, queries = [] } = {}) {
  const operator = retainOperator ? messages.filter(m => m.role === 'user' && m.source?.kind === 'user').at(-1) : undefined;
  const instruction = operator?.content.filter(c => c.type === 'text').map(c => c.text).join('\n');
  // Keep completion evidence, including successful prefixes of a failed batch,
  // without importing the last assistant's reasoning or an obsolete inventory.
  const calls = new Map();
  let execution;
  for (const message of messages) {
    if (message.role === 'assistant') {
      for (const block of message.content) {
        if (block.type !== 'tool-call' || block.name !== 'sts') continue;
        let args = block.arguments;
        try { if (typeof args === 'string') args = JSON.parse(args); } catch { continue; }
        if (typeof args?.command === 'string') calls.set(block.id, args.command);
      }
    }
    if (message.role !== 'tool') continue;
    const command = calls.get(message.toolCallId);
    calls.delete(message.toolCallId);
    if (!command) continue;
    // Classification is supplied by the current stage's complete table. Its
    // heading alone would look like an empty game execution after this cut.
    const receipts = message.content.filter(c => c.type === 'text').flatMap(c =>
      c.text.split('\n').filter(line => /^Completed:|^Error:/.test(line)));
    if (receipts.length) execution = { command, receipts };
    else if (!message.isError && execution) {
      // A successful recovery query supersedes the old failure, while actual
      // adopted prefixes of a failed batch remain completion evidence.
      execution.receipts = execution.receipts.filter(line => !/^Error:/.test(line));
      if (!execution.receipts.length) execution = undefined;
    }
  }
  const prefix = [
    'Player context refreshed from actual game state. Continue the operator task from its current progress.',
    ...(instruction ? ['Operator task:', instruction] : []),
    ...(execution ? [
      'Latest game execution by this player before the context refresh:',
      `Submitted command: ${execution.command}`,
      'Returned execution receipts:', ...execution.receipts,
    ] : []),
    'Current actual game reading (already read by the host):',
  ].join('\n') + '\n';
  return { readingRange: { start: prefix.length, end: prefix.length + reading.length },
    content: [{ type: 'text', text: prefix + [reading,
    ...(queries.length ? ['Previously requested information, refreshed by the host for this task:',
      ...queries.flatMap(query => [`Query: ${query.command}`, query.text])] : []),
    ].join('\n') }] };
}

// A game already owns its complete working state. Preserve an actual tool
// reading at the cut, rather than asking the player to restate mechanics and
// turn its own guesses into the next request's established background.
export function recordedGameReading(result) {
  if (result?.message?.isError || !result?.meta?.currentReading) return;
  const reading = result.meta.currentReading;
  const text = result.message.content.filter(block => block.type === 'text').map(block => block.text).join('\n');
  if (!['strategic', 'combat', 'terminal'].includes(reading.control)
      || typeof reading.summary !== 'string' || !reading.summary.trim()
      || typeof reading.text !== 'string' || !reading.text.trim() || !text.includes(reading.text)) {
    throw new Error('Recorded game reading does not match its tool result');
  }
  return reading;
}

export function gameCheckpoint(messages, results = []) {
  const recorded = new Map(results.filter(result => typeof result.message?.id === 'string')
    .map(result => [result.message.id, result]));
  const calls = new Set();
  let reading;
  for (const message of messages) {
    if (message.role === 'assistant') {
      for (const block of message.content) {
        if (block.type !== 'tool-call' || block.name !== 'sts') continue;
        calls.add(block.id);
      }
    }
    if (message.role !== 'tool') continue;
    const paired = calls.has(message.toolCallId);
    calls.delete(message.toolCallId);
    if (!paired) continue;
    if (message.isError) { reading = undefined; continue; }
    const result = typeof message.id === 'string' ? recorded.get(message.id) : undefined;
    if (!result || result.message.toolCallId !== message.toolCallId) continue;
    const current = recordedGameReading({ message, meta: result.meta });
    if (current) reading = current.text;
  }
  if (calls.size) reading = undefined;
  const operator = messages.filter(m => m.role === 'user' && m.source?.kind === 'user').at(-1);
  const instruction = operator?.content.filter(c => c.type === 'text').map(c => c.text).join('\n');
  return [{ type: 'text', text: [
    'Game history checkpoint. Continue from the recent messages following this checkpoint.',
    ...(instruction ? ['Operator task:', instruction] : []),
    ...(reading ? ['Last recorded actual game reading in the earlier history:', reading] : []),
    'Rebuild the startup, defense/damage and next-need plan from the current inventory. Obtain card, relic, potion, power and enemy mechanics from game readings and info queries.',
  ].join('\n') }];
}

export default class GameContextCompaction extends BasicCompaction {
  constructor(ctx, config) {
    super(ctx, config);
    // A decision replacement may grow (new owned mechanics), whereas pressure
    // compaction must shrink. Both use the official append-only surface owner.
    // This admission hook propagates a failed replacement to the caller.
    ctx.on('agent/pre-step', async ({ agent, signal }, next) => {
      if (!pending.has(agent)) return next();
      signal.throwIfAborted();
      const reset = pending.get(agent);
      if (!reset.reading) throw new Error('game context reset requires the current actual reading before another model step');
      const decision = await next();
      if (decision.kind !== 'enter') return decision;
      // Newly accepted operator messages are not durable yet. The loop will
      // append them to this request; copying the previous task would both
      // duplicate instructions and revive obsolete continuation guidance.
      const retainOperator = !decision.messages.some(message => message.role === 'user' && message.source?.kind === 'user');
      // Before the first step, the loop has claimed its operator messages but
      // has not committed its protected system head. Admit the factual reading
      // through the loop's message decision so it follows that head in the log.
      if (agent.session.surface.nodes.length === 0) {
        const rendered = decisionCheckpoint(decision.messages, reset.reading, { retainOperator, queries: reset.queries });
        const checkpoint = createUserMessage({
          source: { kind: 'sts-decision', version: 1, ...(reset.source ? { gameSource: reset.source } : {}),
            readingRange: rendered.readingRange,
            ...(reset.task ? { task: reset.task } : {}), ...(reset.queryRefs.length ? { queryRefs: reset.queryRefs } : {}) },
          content: rendered.content,
        });
        pending.delete(agent);
        return { ...decision, messages: [...decision.messages, checkpoint] };
      }
      this.resetDecisionContext(agent, signal, { retainOperator });
      return decision;
    });
  }

  resetDecisionContext(agent, signal, { retainOperator = true } = {}) {
    signal.throwIfAborted();
    const reset = pending.get(agent);
    if (!reset) return null;
    if (!reset.reading) throw new Error('game context reset requires the current actual reading before another model step');
    const range = decisionContextRange(agent.session);
    const nodes = agent.session.surface.nodes;
    const sources = range ? nodes.slice(nodes.indexOf(range.start), nodes.indexOf(range.end) + 1) : [];
    const operator = operatorTask(agent);
    const messages = agent.session.deriveMessages();
    const rendered = decisionCheckpoint(operator ? [...messages, operator] : messages,
      reset.reading, { retainOperator, queries: reset.queries });
    const checkpoint = createUserMessage({
      source: { kind: 'sts-decision', version: 1, ...(reset.source ? { gameSource: reset.source } : {}),
        readingRange: rendered.readingRange,
        ...(reset.task ? { task: reset.task } : {}), ...(reset.queryRefs.length ? { queryRefs: reset.queryRefs } : {}) },
      content: rendered.content,
    });
    // A fresh player has no old reasoning to replace, but still needs the
    // supplied actual state. Keep its operator task and append the checkpoint.
    // Checks and append are synchronous for both official surface operations.
    const event = agent.session.append('user/message', checkpoint, range ? {
      surfaceOp: { op: 'replace', startSeq: range.start, endSeq: range.end }, sourceEventSeqs: [...sources],
    } : { surfaceOp: 'append' });
    pending.delete(agent);
    return event;
  }

  async compactIfNeeded(agent, trigger, signal) {
    // The mandatory admission hook handles semantic replacement. Skip pressure
    // compaction of history that is about to be discarded there.
    return pending.has(agent) ? null : super.compactIfNeeded(agent, trigger, signal);
  }

  async summarize(input, agent, signal) {
    signal?.throwIfAborted();
    const operator = operatorTask(agent);
    const messages = operator ? [...input.messages, operator] : input.messages;
    return {
      summary: gameCheckpoint(messages, agent.session.snapshotEvents()
        .filter(event => event.type === 'tool/result').map(event => event.data)),
      provider: 'sts-player', model: 'game-checkpoint',
    };
  }
}
