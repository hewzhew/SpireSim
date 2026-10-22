// Read-only inspection of the official dsh journal. Rust state is never opened
// here; request boundaries reference existing messages instead of copying them.
import { appendFile, readFile, readdir } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { createPresentation } from '@sts-simulator/presentation';
import { Context } from '@deepseek-ai/cordis';
import { Session, SessionId } from '@deepseek-ai/dsh-session';
import { isAgentLoopRequest, assembleAssistantStream, assistantStreamChunks } from '@deepseek-ai/dsh-llm';
import * as SessionPersistence from '@deepseek-ai/dsh-session-persistence-jsonl';
import { gameContextSource, sameGameContextSource, sameGameDecisionTask } from './context.mjs';

const usage = 'player inspect [STEP[.ATTEMPT] [input]] [--run RUN_DIRECTORY_NAME]';
function inspectionText(locale, key, parameters = {}) {
  return createPresentation({ locale }).text(`scripted:inspection.${key}`,
    Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));
}
const textOf = message => (message?.content ?? []).filter(block => block.type === 'text').map(block => block.text).join('\n');
const jsonRequest = ({ signal, ...request }) => JSON.parse(JSON.stringify(request));
const stable = value => Array.isArray(value) ? value.map(stable)
  : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map(key => [key, stable(value[key])])) : value;
const digest = request => createHash('sha256').update(JSON.stringify(stable(request))).digest('hex');

export function settlementBlocks(event) {
  if (event.type === 'assistant/message') return event.data.message.content;
  // Failed attempts retain a stream, not a committed message. Use the SDK's
  // safe visible prefix and keep completed tool declarations unexecuted.
  return [...assembleAssistantStream(event.data.stream).interruptedBlocks(),
    ...assistantStreamChunks(event.data.stream, 'block-end').filter(chunk => chunk.block.type === 'tool-call').map(chunk => chunk.block)];
}

export function settlementUsage(event) {
  return event.data.usage ?? assistantStreamChunks(event.data.stream ?? [], 'usage').at(-1)?.usage ?? null;
}

export function parseInspectionArgs(args) {
  const positional = [], options = {};
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--run') {
      if (options.run || !args[i + 1] || /[\\/]/.test(args[i + 1]) || args[i + 1].startsWith('-')) throw new Error(usage);
      options.run = args[++i];
    } else positional.push(args[i]);
  }
  if (positional.length > 2 || (positional[1] && positional[1] !== 'input')) throw new Error(usage);
  if (positional.length) {
    if (!/^[1-9]\d*(?:\.[1-9]\d*)?$/.test(positional[0])) throw new Error(usage);
    const [step, attempt] = positional[0].split('.').map(Number);
    if (!Number.isSafeInteger(step) || (attempt !== undefined && !Number.isSafeInteger(attempt))) throw new Error(usage);
    Object.assign(options, { step, attempt, input: positional[1] === 'input' });
  }
  return options;
}

export function journalRequest({ events, meta, inheritedEventCount = 0 }, throughSeq, locale = 'en') {
  if (!Number.isSafeInteger(throughSeq) || throughSeq < 0 || throughSeq >= events.length) {
    throw new Error(inspectionText(locale, 'boundaryUnavailable'));
  }
  const session = Session.fromRestore(SessionId(meta.id), events.slice(0, throughSeq + 1), meta,
    Math.min(inheritedEventCount, throughSeq + 1), 'shared-frozen');
  const header = session.requestHeader();
  if (!header) throw new Error(inspectionText(locale, 'headerUnavailable'));
  return jsonRequest({ ...header.config, messages: session.deriveMessages(), toolHistory: session.toolHistory(),
    ...(header.tools ? { tools: header.tools } : {}), sessionId: meta.id });
}

// Capture at the official SDK dispatch seam, before any streamed response.
// An exceptional transformed input is saved explicitly rather than pretending
// the journal-derived request is identical. Ordinary records contain only refs.
export function recordSessionRequests(ctx, { id, session, record }) {
  let position;
  const attempts = new Map();
  const closeEvents = ctx.on('session/event', (owner, event) => {
    if (owner.id === id && event.type === 'step/start') position = event.data;
  });
  const closeRequests = ctx.on('llm/stream', async function* (request, next) {
    if (request.sessionId !== id || !isAgentLoopRequest(request) || !position) { yield* next(); return; }
    const current = session();
    const throughSeq = current.snapshotEvents().length - 1;
    const actual = jsonRequest(request);
    const requestHash = digest(actual);
    const reconstructed = jsonRequest({ ...current.requestHeader().config, messages: current.deriveMessages(),
      toolHistory: current.toolHistory(), ...(current.requestHeader().tools ? { tools: current.requestHeader().tools } : {}), sessionId: id });
    const key = `${position.turn}.${position.step}`;
    const attempt = (attempts.get(key) ?? 0) + 1;
    attempts.set(key, attempt);
    await ctx.sessions.flush(current);
    await record({ ...position, attempt, throughSeq, requestHash,
      ...(digest(reconstructed) === requestHash ? {} : { snapshot: actual }) });
    yield* next();
  });
  return () => { closeRequests(); closeEvents(); };
}

export function recordPlayerRequests(ctx, { id, session, output }) {
  return recordSessionRequests(ctx, { id, session, record: boundary => appendFile(join(output, 'requests.jsonl'),
    JSON.stringify({ schema: 'sts-player-request-boundary-v1', ...boundary }) + '\n') });
}

export function requestFromBoundary(log, boundary, locale = 'en') {
  const request = boundary.snapshot ?? journalRequest(log, boundary.throughSeq, locale);
  if (digest(request) !== boundary.requestHash) throw new Error(inspectionText(locale, 'digestMismatch'));
  return request;
}

async function optionalJson(path) {
  try { return JSON.parse(await readFile(path, 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}

export async function readPlayerJournal(home, id, { compression = 'none' } = {}) {
  const ctx = new Context();
  let handle;
  try {
    await ctx.plugin(SessionPersistence, { root: join(home, 'sessions'), compression });
    handle = await ctx.sessionPersistence.open(SessionId(id), 'read');
    const { events } = await handle.read();
    return { events, meta: handle.header, inheritedEventCount: handle.inheritedEventCount };
  } finally {
    if (handle) await handle.close();
    await ctx.fiber.dispose();
  }
}

async function findRun(binding, repository, session, name, locale = 'en') {
  if (!binding?.id || !binding.home) throw new Error(inspectionText(locale, 'noBoundPlayer'));
  const root = join(binding.home, 'runs');
  const timestamp = name => Number(name.slice(binding.id.length + 1).split('-')[0]);
  let candidates;
  const listRuns = async () => candidates ??= (await readdir(root, { withFileTypes: true })).filter(entry => entry.isDirectory()
    && entry.name.startsWith(`${binding.id}-`) && Number.isSafeInteger(timestamp(entry.name))).map(entry => entry.name)
    .sort((a, b) => timestamp(b) - timestamp(a) || b.localeCompare(a));
  const selected = name ?? (binding.lastRun ? basename(binding.lastRun) : (await listRuns())[0]);
  if (!selected || !selected.startsWith(`${binding.id}-`) || !Number.isSafeInteger(timestamp(selected))) {
    throw new Error(inspectionText(locale, 'noMatchingRun'));
  }
  const directory = join(root, selected);
  const manifest = await optionalJson(join(directory, 'manifest.json'));
  if (!manifest || manifest.id !== binding.id || typeof manifest.session !== 'string' || typeof manifest.repository !== 'string'
      || resolve(manifest.session).toLowerCase() !== resolve(session).toLowerCase()
      || resolve(manifest.repository).toLowerCase() !== resolve(repository).toLowerCase()) {
    throw new Error(inspectionText(locale, 'foreignRun'));
  }
  const nextTime = manifest.eventStart === undefined
    ? (await listRuns()).map(timestamp).filter(time => time > timestamp(selected)).sort((a, b) => a - b)[0] ?? Infinity : Infinity;
  return { directory, manifest, createdAt: timestamp(selected), nextTime,
    result: await optionalJson(join(directory, 'result.json')),
    finalObservation: await optionalJson(join(directory, 'final-observation.json')) };
}

async function readBoundaries(directory, locale = 'en') {
  let text;
  try { text = await readFile(join(directory, 'requests.jsonl'), 'utf8'); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
  const lines = text.split('\n');
  // A running process may still be appending its last line. Only complete rows
  // are inspectable; a malformed completed row is an error, not a legacy fallback.
  lines.pop();
  return lines.filter(Boolean).map(line => {
    const row = JSON.parse(line);
    if (row.schema !== 'sts-player-request-boundary-v1') throw new Error(inspectionText(locale, 'unsupportedBoundary'));
    return row;
  });
}

export function inspectJournal(log, run, boundaries, locale = 'en') {
  const { events } = log;
  const start = run.manifest.eventStart ?? events.find(event => event.type === 'turn/start'
    && event.time >= run.createdAt && event.time < run.nextTime)?.seq;
  if (start === undefined) return { ...run, steps: [], scope: 'noRecordedTurn' };
  const end = run.result?.eventEnd ?? events.find(event => event.seq >= start && event.time >= run.nextTime)?.seq ?? events.length;
  if (!Number.isSafeInteger(start) || start < 0 || !Number.isSafeInteger(end) || end < start || end > events.length) {
    throw new Error(inspectionText(locale, 'invalidRunRange'));
  }
  const owned = events.slice(start, end);
  const starts = owned.filter(event => event.type === 'step/start');
  const steps = starts.map((event, index) => {
    const stop = starts[index + 1]?.seq ?? end;
    const members = owned.filter(member => member.seq >= event.seq && member.seq < stop);
    const settlements = members.filter(member => ['assistant/message', 'assistant/attempt'].includes(member.type));
    const references = boundaries === null ? settlements.map((settlement, i) => ({
      turn: event.data.turn, step: event.data.step, attempt: i + 1, throughSeq: settlement.seq - 1, legacy: true,
    })) : boundaries.filter(row => row.turn === event.data.turn && row.step === event.data.step);
    const requests = references.map(reference => {
      try {
        if (!Number.isSafeInteger(reference.throughSeq) || reference.throughSeq < event.seq || reference.throughSeq >= stop) {
          throw new Error(inspectionText(locale, 'boundaryOutsideStep'));
        }
        if (!reference.legacy && (!Number.isSafeInteger(reference.attempt) || reference.attempt < 1
            || typeof reference.requestHash !== 'string' || !/^[0-9a-f]{64}$/.test(reference.requestHash))) {
          throw new Error(inspectionText(locale, 'invalidAttemptHash'));
        }
        const request = reference.snapshot ?? journalRequest(log, reference.throughSeq, locale);
        if (reference.requestHash && digest(request) !== reference.requestHash) throw new Error(inspectionText(locale, 'dispatchHashMismatch'));
        const checkpointSource = requestReading(request)?.source;
        const queryRefs = checkpointSource?.queryRefs;
        const refreshedQueries = (Array.isArray(queryRefs) ? queryRefs : []).flatMap(seq => {
          const event = Number.isSafeInteger(seq) && seq >= 0 && seq <= reference.throughSeq ? events[seq] : undefined;
          const meta = event?.data?.meta;
          return event?.type === 'tool/result' && !event.data.message.isError && meta?.schema === 'sts-context-query-v1'
            && checkpointSource.task && sameGameDecisionTask(meta.task, checkpointSource.task)
            && typeof meta.command === 'string' ? [meta.command] : [];
        });
        return { ...reference, request, refreshedQueries,
          precision: reference.legacy ? 'journal-only' : reference.snapshot ? 'sdk-snapshot' : 'sdk-verified' };
      } catch (error) { return { ...reference, precision: 'unavailable', error: error.message }; }
    });
    const results = new Map(members.filter(member => member.type === 'tool/result').map(member => [member.data.message.toolCallId, member]));
    const begun = new Map(members.filter(member => member.type === 'tool/call').map(member => [member.data.callId, member.data]));
    const declared = settlements.flatMap(member => settlementBlocks(member)
      .filter(block => block.type === 'tool-call').map(block => ({ ...block, committed: member.type === 'assistant/message' })));
    const calls = declared.map(block => ({ callId: block.id, name: block.name, arguments: block.arguments,
      started: block.committed && begun.has(block.id), result: block.committed ? results.get(block.id)?.data.message : undefined }));
    for (const [id, call] of begun) if (!declared.some(block => block.committed && block.id === id)) calls.push({ ...call, started: true, result: results.get(id)?.data.message });
    return { number: index + 1, ...event.data, requests, calls, settlements };
  });
  return { ...run, steps, scope: run.manifest.eventStart === undefined ? 'timeRange' : 'eventRange' };
}

function brief(text, limit = 8, locale = 'en') {
  const lines = text.trim().split('\n');
  return lines.length > limit ? inspectionText(locale, 'moreLines', { text: lines.slice(0, limit).join('\n'), count: lines.length - limit }) : text.trim();
}

function requestReading(request) {
  return request?.messages.filter(message => message.source?.kind === 'sts-decision').at(-1);
}

function sourceLabel(source, locale = 'en') {
  return source ? inspectionText(locale, 'sourceLabel', { session: source.session_id, head: source.head_revision, state: source.state_revision }) : inspectionText(locale, 'unavailable');
}

function recordedState(observation, locale = 'en') {
  if (!observation) return inspectionText(locale, 'finalObservationUnavailable');
  const position = observation.position;
  if (position.kind === 'terminal') return inspectionText(locale, 'terminal', { outcome: JSON.stringify(position.outcome) });
  const visible = position.observation;
  const state = visible.observation;
  const run = visible.domain === 'strategic' ? state.run : visible.run_context;
  const hp = visible.domain === 'strategic' ? run.current_hp : state.player.hp;
  const max = visible.domain === 'strategic' ? run.max_hp : state.player.max_hp;
  const gold = visible.domain === 'strategic' ? run.gold : state.player.gold;
  return inspectionText(locale, 'recordedState', { act: run.act, floor: run.floor, hp: hp, maxHp: max, gold: gold, context: visible.domain === 'combat' ? 'combat' : state.context.kind });
}

export function renderInspection(report, { step, attempt, input, locale = 'en' } = {}) {
  const selected = step === undefined ? report.steps : report.steps.filter(item => item.number === step);
  if (step !== undefined && !selected.length) throw new Error(inspectionText(locale, 'stepUnavailable', { step: step }));
  const lines = [inspectionText(locale, 'player', { id: report.manifest.id }), inspectionText(locale, 'run', { directory: basename(report.directory), outcome: report.result?.outcome ?? inspectionText(locale, 'runningIncomplete'), steps: report.steps.length }), inspectionText(locale, report.scope)];
  if (input) lines.push(inspectionText(locale, 'sdkInput'));
  const seen = new Map();
  const showChanged = (key, value, label) => {
    const signature = JSON.stringify(value);
    if (step === undefined && seen.get(key) === signature) return false;
    seen.set(key, signature);
    if (label) lines.push(label);
    return true;
  };
  for (const item of selected) {
    lines.push('', inspectionText(locale, 'step', { number: item.number, turn: item.turn, step: item.step }));
    const requests = attempt === undefined ? item.requests : item.requests.filter(request => request.attempt === attempt);
    if (attempt !== undefined && !requests.length) throw new Error(inspectionText(locale, 'attemptUnavailable', { attempt: attempt, step: item.number }));
    if (!requests.length) lines.push(inspectionText(locale, 'noDispatchedRequest'));
    for (const entry of requests) {
      lines.push(inspectionText(locale, 'request', { attempt: entry.attempt, precision: entry.precision }));
      if (entry.legacy) lines.push(inspectionText(locale, 'legacyRequest'));
      if (entry.error) { lines.push(inspectionText(locale, 'inputUnavailable', { error: entry.error })); continue; }
      if (input) { lines.push(JSON.stringify(entry.request, null, 2)); continue; }
      const checkpoint = requestReading(entry.request);
      const model = [entry.request.provider, entry.request.model, entry.request.reasoningEffort, entry.request.maxTokens];
      showChanged('model', model, inspectionText(locale, 'model', { provider: model[0], model: model[1], reasoning: model[2] ?? inspectionText(locale, 'default'), maxTokens: model[3] ?? inspectionText(locale, 'default') }));
      const system = entry.request.messages.filter(message => message.role === 'system').map(message => message.content);
      showChanged('system', system, inspectionText(locale, 'system', { bytes: Buffer.byteLength(JSON.stringify(system)) }));
      const task = entry.request.messages.filter(message => message.source?.kind === 'user').at(-1);
      if (task && showChanged('task', task.content)) {
        const taskText = textOf(task);
        lines.push(inspectionText(locale, 'task', { text: step === undefined && taskText.length > 240 ? inspectionText(locale, 'taskContinues', { text: taskText.slice(0, 240) }) : taskText }));
      }
      showChanged('tools', entry.request.tools, inspectionText(locale, 'tools', { names: (entry.request.tools ?? []).map(tool => tool.name).join(', ') }));
      if (checkpoint) {
        const checkpointText = textOf(checkpoint);
        if (showChanged('reading', [checkpoint.source?.gameSource, checkpoint.content])) {
          lines.push(inspectionText(locale, 'sourceSupplied', { source: sourceLabel(checkpoint.source?.gameSource, locale) }));
          if (entry.refreshedQueries?.length) lines.push(inspectionText(locale, 'refreshedQueries'), ...entry.refreshedQueries.map(command => `  ${command}`));
          const range = checkpoint.source?.readingRange;
          if (range && (!Number.isSafeInteger(range.start) || !Number.isSafeInteger(range.end)
              || range.start < 0 || range.end <= range.start || range.end > checkpointText.length)) {
            throw new Error(inspectionText(locale, 'invalidReadingRange'));
          }
          const reading = range ? checkpointText.slice(range.start, range.end) : checkpointText;
          lines.push(inspectionText(locale, 'suppliedReading', { text: brief(reading, step === undefined ? 10 : Infinity, locale) }));
        } else lines.push(inspectionText(locale, 'sameCheckpoint'));
      }
    }
    if (input) continue;
    if (step !== undefined) for (const settlement of item.settlements) {
      const blocks = settlementBlocks(settlement);
      lines.push(inspectionText(locale, 'modelOutput'), ...blocks.filter(block => ['text', 'reasoning'].includes(block.type)).map(block => `${block.type}: ${block.text}`));
      if (settlement.type === 'assistant/attempt') lines.push(inspectionText(locale, 'uncommittedStream'));
    }
    for (const call of item.calls) {
      let args;
      try { args = typeof call.arguments === 'string' ? JSON.parse(call.arguments) : call.arguments; } catch { args = null; }
      lines.push(`${call.name}${args?.command ? ` ${args.command}` : ` ${call.arguments}`}${call.started ? '' : inspectionText(locale, 'notStarted')}`);
      lines.push(call.result ? `${inspectionText(locale, call.result.isError ? 'toolError' : 'returned')}:\n${brief(textOf(call.result), step === undefined ? 8 : Infinity, locale)}` : inspectionText(locale, 'noToolResult'));
    }
  }
  if (!input) {
    lines.push('', inspectionText(locale, 'recordedEnd', { state: recordedState(report.finalObservation, locale) }));
    if (report.finalObservation) {
      const finalSource = gameContextSource(report.finalObservation);
      const first = report.steps[0]?.requests[0]?.request;
      const initialSource = requestReading(first)?.source?.gameSource;
      lines.push(inspectionText(locale, 'finalSource', { source: sourceLabel(finalSource, locale), changeSuffix: initialSource ? ` (${sameGameContextSource(initialSource, finalSource) ? inspectionText(locale, 'sameSource') : inspectionText(locale, 'changedSource')})` : '' }));
    }
    lines.push(inspectionText(locale, 'expandHelp'));
  }
  return lines.join('\n') + '\n';
}

export async function inspectPlayer({ repository, session, binding, options = {}, locale = 'en' }) {
  const run = await findRun(binding, repository, session, options.run, locale);
  const [log, boundaries] = await Promise.all([readPlayerJournal(binding.home, binding.id), readBoundaries(run.directory, locale)]);
  const report = inspectJournal(log, run, boundaries, locale);
  return { report, text: renderInspection(report, { ...options, locale }) };
}
