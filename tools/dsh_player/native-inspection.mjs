// Maintainer-only observation. The official journal owns conversation history;
// the Rust workbench owns the optional live read. Neither path runs a model.
import { readFile, readdir, mkdir, writeFile, realpath } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { createPresentation } from '@sts-simulator/presentation';
import { Context } from '@deepseek-ai/cordis';
import { Session, SessionId } from '@deepseek-ai/dsh-session';
import Projections from '@deepseek-ai/dsh-session-projection';
import TokenMeter from '@deepseek-ai/dsh-token-meter';
import { readPlayerJournal, journalRequest, requestFromBoundary, settlementBlocks, settlementUsage } from './inspection.mjs';
import { workspaceFacts } from './native.mjs';
import { recordedGameReading } from './context.mjs';

const usage = 'observe [STEP [input] | live | list] [--home DSH_HOME] [--chat ID_FRAGMENT] [--turn N]';
function nativeObservationText(locale, key, parameters = {}) {
  return createPresentation({ locale }).text(`native:inspection.${key}`,
    Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));
}
const textOf = message => (message?.content ?? []).filter(block => block.type === 'text').map(block => block.text).join('\n');
const sourceSchema = 'sts-native-observation-source-v1';
const number = value => /^[1-9]\d*$/.test(value ?? '') && Number.isSafeInteger(Number(value));
const samePath = (a, b) => typeof a === 'string' && typeof b === 'string' && (process.platform === 'win32'
  ? resolve(a).toLowerCase() === resolve(b).toLowerCase() : resolve(a) === resolve(b));

export function parseNativeObservation(args, locale = 'en') {
  const options = {}, positional = [];
  for (let i = 0; i < args.length; i++) {
    const key = args[i];
    if (['--home', '--chat', '--turn'].includes(key)) {
      const field = key.slice(2), value = args[++i];
      if (options[field] !== undefined || !value || value.startsWith('--')) throw new Error(usage);
      options[field] = value;
    } else positional.push(key);
  }
  if (options.turn !== undefined) {
    if (!number(options.turn)) throw new Error(usage);
    options.turn = Number(options.turn);
  }
  if (positional.length > 2 || (positional[1] && positional[1] !== 'input')) throw new Error(usage);
  if (positional.length) {
    if (['live', 'list'].includes(positional[0]) && positional.length === 1) options.mode = positional[0];
    else {
      if (!number(positional[0])) throw new Error(usage);
      options.step = Number(positional[0]); options.input = positional[1] === 'input';
    }
  }
  if (options.mode === 'live' && options.turn !== undefined) throw new Error(nativeObservationText(locale, 'liveNotHistorical'));
  return options;
}

async function optionalJson(path) {
  try { return JSON.parse(await readFile(path, 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}

async function nativeBindings(repository, locale = 'en') {
  const root = join(repository, '.oracle-lab/run-play');
  let entries;
  try { entries = await readdir(root, { withFileTypes: true }); }
  catch (error) { if (error.code === 'ENOENT') return []; throw error; }
  const bindings = [];
  for (const entry of entries.filter(entry => entry.isDirectory() && /^dsh-native-[a-f0-9]{24}$/.test(entry.name))) {
    const session = join(root, entry.name), binding = await optionalJson(join(session, 'dsh-native.json'));
    if (!binding) continue;
    const digest = typeof binding.agent === 'string' && createHash('sha256').update(binding.agent).digest('hex').slice(0, 24);
    if (binding.schema !== 'sts-dsh-native-binding-v1' || !samePath(binding.repository, repository)
        || !samePath(binding.session, session) || entry.name !== `dsh-native-${digest}`) {
      throw new Error(nativeObservationText(locale, 'invalidBinding', { session: session }));
    }
    bindings.push(binding);
  }
  return bindings;
}

export async function nativeJournalSummary(log, binding, { turn: wantedTurn, step: wantedStep, input = false, locale = 'en' } = {}) {
  if (log.meta.id !== binding.agent || !samePath(log.meta.cwd, binding.repository)) throw new Error(nativeObservationText(locale, 'journalBindingMismatch'));
  for (const event of log.events.filter(event => event.type === 'tool/result' && event.data.meta?.schema === 'sts-native-game-v1')) {
    if (event.data.meta.agent !== binding.agent || !samePath(event.data.meta.session, binding.session)) {
      throw new Error(nativeObservationText(locale, 'foreignToolResult', { event: event.seq }));
    }
  }
  const { events } = log, starts = events.filter(event => event.type === 'turn/start');
  const start = wantedTurn === undefined ? starts.at(-1) : starts.find(event => event.data.turn === wantedTurn);
  if (!start && wantedTurn !== undefined) throw new Error(nativeObservationText(locale, 'turnUnavailable', { turn: wantedTurn }));
  const end = start && events.find(event => event.seq > start.seq && event.type === 'turn/end' && event.data.turn === start.data.turn);
  const stop = end ? end.seq + 1 : events.length;
  const owned = start ? events.slice(start.seq, stop) : [];
  const steps = owned.filter(event => event.type === 'step/start').map((event, i, starts) => {
    const members = owned.filter(member => member.seq >= event.seq && member.seq < (starts[i + 1]?.seq ?? stop));
    const results = new Map(members.filter(member => member.type === 'tool/result').map(member => [member.data.message.toolCallId, member]));
    return { number: event.data.step, seq: event.seq, members,
      calls: members.filter(member => member.type === 'tool/call').map(call => ({ ...call.data, seq: call.seq, result: results.get(call.data.callId) })) };
  });
  if (wantedStep !== undefined && !steps.some(step => step.number === wantedStep)) throw new Error(nativeObservationText(locale, 'stepUnavailable', { step: wantedStep }));
  const calls = steps.flatMap(step => step.calls), gameCalls = calls.filter(call => call.name === 'sts');
  const gameResults = events.filter(event => event.seq < stop && event.type === 'tool/result'
    && event.data.meta?.schema === 'sts-native-game-v1' && !event.data.message.isError);
  const latest = gameResults.at(-1);
  const ctx = new Context();
  let accounting;
  try {
    await ctx.plugin(Projections); await ctx.plugin(TokenMeter);
    const session = Session.fromRestore(SessionId(log.meta.id), events.slice(0, stop), log.meta,
      Math.min(log.inheritedEventCount, stop), 'shared-frozen');
    const projection = ctx.sessionProjections.checkpoint(session).tokenUsage?.val;
    const reported = events.slice(0, stop).some(event => ['assistant/message', 'assistant/attempt'].includes(event.type)
      && (event.data.usage || event.data.stream?.some(chunk => chunk.type === 'usage')));
    accounting = reported ? { totals: projection.totals, last: projection.last?.buckets ?? null } : null;
  } finally { await ctx.fiber.dispose(); }
  const title = events.filter(event => event.type === 'session/title' && event.seq < stop).at(-1)?.data.title ?? binding.agent;
  const report = { binding, title, turn: start?.data.turn, status: end?.data.reason?.kind ?? (start ? 'open' : 'empty'),
    steps: steps.length, timeline: steps.map(step => {
      const messages = step.members.filter(event => ['assistant/message', 'assistant/attempt'].includes(event.type));
      const calls = step.calls.map(call => ({ name: call.name, command: commandOf(call),
        outcome: !call.result ? 'unconfirmed' : call.result.data.message.isError ? 'error' : 'completed' }));
      const ended = step.members.find(event => event.type === 'step/end');
      const begun = step.members[0];
      const output = messages.flatMap(settlementBlocks).find(block => block.type === 'text')?.text;
      return { step: step.number, seq: step.seq, revision: step.members.at(-1).seq, calls,
        label: calls.length ? calls.map(call => call.name === 'sts' ? call.command : call.name).join(', ')
          : output?.split('\n')[0].slice(0, 120) || null,
        outcome: calls.some(call => call.outcome === 'error') ? 'error' : calls.some(call => call.outcome === 'unconfirmed') ? 'unconfirmed'
          : calls.length ? 'completed' : messages.some(event => event.type === 'assistant/message') ? 'model' : 'unconfirmed',
        durationMs: ended && ended.time >= begun.time ? ended.time - begun.time : null,
        usage: messages.map(settlementUsage).findLast(usage => usage !== null) ?? null };
    }), turnNumbers: starts.map(event => event.data.turn),
    gameCalls: gameCalls.length, accounting,
    errors: gameCalls.filter(call => call.result?.data.message.isError),
    pending: calls.filter(call => !call.result),
    intervening: gameCalls.filter(call => call.seq > (latest?.seq ?? -1)),
    returnedAfterRecord: workspaceFacts(events).some(fact => fact.receipt && fact.seq > (latest?.seq ?? -1)),
    recorded: latest ? { seq: latest.seq, turn: latest.data.turn, step: latest.data.step,
      text: textOf(latest.data.message), currentReading: recordedGameReading(latest.data) } : null };
  if (wantedStep !== undefined) {
    const step = steps.find(step => step.number === wantedStep);
    const before = gameResults.filter(event => event.seq < step.seq).at(-1);
    report.selected = { ...step, before, intervening: events.filter(event => event.seq > (before?.seq ?? -1)
      && event.seq < step.seq && event.type === 'tool/call' && event.data.name === 'sts') };
    report.following = steps[steps.indexOf(step) + 1] ?? null;
    if (input) {
      const captured = step.members.filter(event => event.type === 'plugin:sts/request-boundary');
      const deliveries = step.members.filter(event => event.type === 'session-log-deepseek/delivery-accepted');
      report.inputs = captured.length ? captured.map(event => {
        const boundary = event.data;
        if (boundary.schema !== 'sts-native-request-boundary-v1' || boundary.sessionId !== log.meta.id
            || boundary.turn !== report.turn || boundary.step !== step.number
            || !Number.isSafeInteger(boundary.attempt) || boundary.attempt < 1
            || !Number.isSafeInteger(boundary.throughSeq) || boundary.throughSeq < step.seq || boundary.throughSeq >= event.seq) {
          throw new Error(nativeObservationText(locale, 'invalidCapturedBoundary', { event: event.seq }));
        }
        return { eventSeq: event.seq, throughSeq: boundary.throughSeq, attempt: boundary.attempt,
          precision: boundary.snapshot ? 'captured-sdk-request' : 'digest-checked-journal-at-sdk-dispatch',
          requestHash: boundary.requestHash, request: requestFromBoundary(log, boundary, locale) };
      }) : deliveries.map(event => {
        const { throughSeq, sessionId, sessionFormatVersion } = event.data;
        if (sessionId !== log.meta.id || sessionFormatVersion !== log.meta.version
            || !Number.isSafeInteger(throughSeq) || throughSeq < step.seq || throughSeq >= event.seq) {
          throw new Error(nativeObservationText(locale, 'invalidDeliveryBoundary', { event: event.seq }));
        }
        return { eventSeq: event.seq, throughSeq, precision: 'journal-derived-at-recorded-delivery', request: journalRequest(log, throughSeq, locale) };
      });
    }
  }
  return report;
}

export function commandOf(call) {
  try { return JSON.parse(call.arguments).command ?? call.arguments; }
  catch { return call.arguments; }
}

export function renderNativeObservation(report, locale = 'en') {
  const phase = { open: nativeObservationText(locale, 'openTurn'), empty: nativeObservationText(locale, 'noModelTurn') }[report.status] ?? report.status;
  const { binding } = report;
  const lines = [nativeObservationText(locale, 'chat', { title: report.title, agent: binding.agent, session: binding.session }),
    nativeObservationText(locale, 'turn', { turn: report.turn ?? nativeObservationText(locale, 'none'), phase: phase, steps: report.steps, gameCalls: report.gameCalls, errors: report.errors.length })];
  if (report.selected) {
    const selected = report.selected;
    lines.push(nativeObservationText(locale, 'step', { number: selected.number }));
    if (report.inputs) {
      if (report.inputs.length) lines.push(nativeObservationText(locale, report.inputs.every(item =>
        ['captured-sdk-request', 'digest-checked-journal-at-sdk-dispatch'].includes(item.precision)) ? 'verifiedInput' : 'legacyInput'));
      if (!report.inputs.length) lines.push(nativeObservationText(locale, 'noDeliveryBoundary'));
      for (const item of report.inputs) lines.push(nativeObservationText(locale, 'delivery', { event: item.eventSeq, through: item.throughSeq }), JSON.stringify(item.request, null, 2));
    } else {
      lines.push(selected.before ? nativeObservationText(locale, 'lastResultBefore', { event: selected.before.seq, text: textOf(selected.before.data.message) }) : nativeObservationText(locale, 'noEarlierResult'));
      if (selected.intervening.length) lines.push(nativeObservationText(locale, 'interveningCalls'));
      for (const event of selected.members.filter(event => ['assistant/message', 'assistant/attempt'].includes(event.type))) {
        if (event.type === 'assistant/attempt') lines.push(nativeObservationText(locale, 'uncommittedAttempt'));
        for (const block of settlementBlocks(event).filter(block => ['reasoning', 'text'].includes(block.type))) lines.push(`${block.type}:\n${block.text}`);
      }
      for (const call of selected.calls) lines.push(`${call.name} ${commandOf(call)}\n${call.result ? textOf(call.result.data.message) : nativeObservationText(locale, 'noResult')}`);
      const following = report.following;
      if (following) {
        lines.push(nativeObservationText(locale, 'nextStep', { number: following.number }));
        const messages = following.members.filter(event => ['assistant/message', 'assistant/attempt'].includes(event.type));
        if (!messages.length && !following.calls.length) lines.push(nativeObservationText(locale, 'noOutputYet'));
        for (const event of messages) {
          if (event.type === 'assistant/attempt') lines.push(nativeObservationText(locale, 'uncommittedAttempt'));
          for (const block of settlementBlocks(event).filter(block => block.type === 'text')) lines.push(block.text);
        }
        for (const call of following.calls) lines.push(nativeObservationText(locale, 'command', { name: call.name, command: commandOf(call) }));
      } else lines.push(nativeObservationText(locale, 'noFollowingStep'));
    }
    return lines.join('\n') + '\n';
  }
  if (report.accounting) {
    const a = report.accounting.totals, input = a.uncachedInputTokens + a.cacheReadTokens + a.cacheWriteTokens;
    lines.push(nativeObservationText(locale, 'totals', { total: input + a.outputTokens, uncachedInput: a.uncachedInputTokens, cacheRead: a.cacheReadTokens, cacheWrite: a.cacheWriteTokens, output: a.outputTokens }));
    const last = report.accounting.last;
    if (last) lines.push(nativeObservationText(locale, 'lastInput', { tokens: last.uncachedInputTokens + last.cacheReadTokens + last.cacheWriteTokens }));
  } else lines.push(nativeObservationText(locale, 'usageUnreported'));
  if (report.recorded) {
    const compact = report.recorded.currentReading?.summary
      ?? report.recorded.text.split('\n').slice(0, 8).join('\n');
    lines.push(nativeObservationText(locale, 'lastRecordedResult', { step: report.recorded.step, event: report.recorded.seq, text: compact.trimEnd() }));
  } else lines.push(nativeObservationText(locale, 'noSuccessfulResult'));
  if (report.intervening.length) lines.push(nativeObservationText(locale, 'recordedMayBeStale'));
  if (report.returnedAfterRecord) lines.push(nativeObservationText(locale, 'returnedAfterRecord'));
  if (report.errors.length) {
    lines.push(nativeObservationText(locale, 'recentErrors'));
    for (const call of report.errors.slice(-8)) {
      const rawLines = textOf(call.result.data.message).split('\n');
      lines.push(nativeObservationText(locale, 'recentError', { step: call.step, command: commandOf(call), text: rawLines.slice(0, 8).join('\n') }));
      if (rawLines.length > 8) lines.push(nativeObservationText(locale, 'moreErrorLines', { count: rawLines.length - 8, step: call.step }));
    }
    if (report.errors.length > 8) lines.push(nativeObservationText(locale, 'earlierErrorsOmitted', { count: report.errors.length - 8 }));
  }
  if (report.pending.length) lines.push(nativeObservationText(locale, 'unconfirmedCalls', { calls: report.pending.map(call => nativeObservationText(locale, 'callAtStep', { name: call.name, step: call.step })).join(', ') }));
  lines.push(nativeObservationText(locale, 'expandHelp'));
  return lines.join('\n') + '\n';
}

export async function readLiveGame(binding, repository, open, locale = 'en') {
  open ??= (await import('@sts-simulator/run-play')).openWorkbench;
  const game = await open({ repository, session: binding.session, manual: binding.manual ?? false, agentLocale: locale, commandPrefix: '' });
  try {
    const before = await game.observe();
    const reading = await game.execute(before.position?.kind === 'terminal' ? 'read' : 'read, inventory');
    const after = await game.observe();
    if (before.head_revision !== after.head_revision || before.state_revision !== after.state_revision
        || before.position?.public_snapshot_id !== after.position?.public_snapshot_id) {
      throw new Error(nativeObservationText(locale, 'liveChanged'));
    }
    return { headRevision: after.head_revision, stateRevision: after.state_revision,
      publicSnapshotId: after.position?.public_snapshot_id ?? null, text: reading.text };
  } finally { await game.close(); }
}

export async function observeNative({ repository, parameters = [], locale = 'en' }, { openWorkbench } = {}) {
  repository = await realpath(repository);
  const options = parseNativeObservation(parameters, locale);
  const file = join(repository, '.oracle-lab/run-play-records/native-observation-source.json');
  const saved = await optionalJson(file);
  if (saved && (saved.schema !== sourceSchema || typeof saved.home !== 'string' || typeof saved.chat !== 'string')) {
    throw new Error(nativeObservationText(locale, 'unsupportedSource'));
  }
  const configuredHome = options.home ?? saved?.home ?? process.env.DSH_HOME;
  if (!configuredHome) throw new Error(nativeObservationText(locale, 'selectHome', { usage: usage }));
  const home = await realpath(resolve(repository, configuredHome));
  const bindings = await nativeBindings(repository, locale);
  if (options.mode === 'list') {
    const rows = [];
    for (const binding of bindings) {
      try {
        const log = await readPlayerJournal(home, binding.agent, { compression: 'zstd' });
        const last = log.events.filter(event => event.type === 'turn/start').at(-1);
        const title = log.events.filter(event => event.type === 'session/title').at(-1)?.data.title ?? binding.agent;
        rows.push(`${binding.agent} | ${title} | ${last ? nativeObservationText(locale, 'lastTurn', { turn: last.data.turn }) : nativeObservationText(locale, 'noModelTurn')}`);
      } catch (error) { if (error.name !== 'SessionPersistenceNotFoundError') throw error; }
    }
    return { text: rows.length ? rows.join('\n') + '\n' : nativeObservationText(locale, 'noChats') };
  }
  const selected = options.chat ?? (saved?.home && samePath(home, saved.home) ? saved.chat : null);
  if (!selected) throw new Error(nativeObservationText(locale, 'selectChat'));
  const matches = bindings.filter(binding => binding.agent === selected || binding.agent.includes(selected));
  if (matches.length !== 1) throw new Error(nativeObservationText(locale, 'ambiguousChat', { count: matches.length, matches: matches.map(binding => binding.agent).join(', ') }));
  const binding = matches[0], log = await readPlayerJournal(home, binding.agent, { compression: 'zstd' });
  const report = await nativeJournalSummary(log, binding, { ...options, locale });
  const live = options.mode === 'live' ? await readLiveGame(binding, repository, openWorkbench, locale) : null;
  const text = renderNativeObservation(report, locale) + (live ? nativeObservationText(locale, 'liveReading', { head: live.headRevision, state: live.stateRevision, text: live.text }) : '');
  // Explicit source selection remembers only how to observe; gameplay focus,
  // the journal, the player prompt and game state are never updated here.
  if (options.home || options.chat) {
    await mkdir(join(repository, '.oracle-lab/run-play-records'), { recursive: true });
    await writeFile(file, JSON.stringify({ schema: sourceSchema, home: resolve(home), chat: binding.agent }, null, 2) + '\n');
  }
  return { report, text };
}
