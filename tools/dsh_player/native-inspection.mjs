// Maintainer-only observation. The official journal owns conversation history;
// the Rust workbench owns the optional live read. Neither path runs a model.
import { readFile, readdir, mkdir, writeFile, realpath } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { Context } from '@deepseek-ai/cordis';
import { Session, SessionId } from '@deepseek-ai/dsh-session';
import Projections from '@deepseek-ai/dsh-session-projection';
import TokenMeter from '@deepseek-ai/dsh-token-meter';
import { readPlayerJournal, journalRequest, settlementBlocks, settlementUsage } from './inspection.mjs';
import { workspaceFacts } from './native.mjs';

const usage = 'observe [STEP [input] | live | list] [--home DSH_HOME] [--chat ID_FRAGMENT] [--turn N]';
const textOf = message => (message?.content ?? []).filter(block => block.type === 'text').map(block => block.text).join('\n');
const sourceSchema = 'sts-native-observation-source-v1';
const number = value => /^[1-9]\d*$/.test(value ?? '') && Number.isSafeInteger(Number(value));
const samePath = (a, b) => typeof a === 'string' && typeof b === 'string' && (process.platform === 'win32'
  ? resolve(a).toLowerCase() === resolve(b).toLowerCase() : resolve(a) === resolve(b));

export function parseNativeObservation(args) {
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
  if (options.mode === 'live' && options.turn !== undefined) throw new Error('live reads the present game, not a historical turn.');
  return options;
}

async function optionalJson(path) {
  try { return JSON.parse(await readFile(path, 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}

async function nativeBindings(repository) {
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
      throw new Error(`Invalid native chat binding: ${session}`);
    }
    bindings.push(binding);
  }
  return bindings;
}

export async function nativeJournalSummary(log, binding, { turn: wantedTurn, step: wantedStep, input = false } = {}) {
  if (log.meta.id !== binding.agent || !samePath(log.meta.cwd, binding.repository)) throw new Error('Journal and game binding belong to different chats or workspaces.');
  for (const event of log.events.filter(event => event.type === 'tool/result' && event.data.meta?.schema === 'sts-native-game-v1')) {
    if (event.data.meta.agent !== binding.agent || !samePath(event.data.meta.session, binding.session)) {
      throw new Error(`Native tool result ${event.seq} belongs to another game.`);
    }
  }
  const { events } = log, starts = events.filter(event => event.type === 'turn/start');
  const start = wantedTurn === undefined ? starts.at(-1) : starts.find(event => event.data.turn === wantedTurn);
  if (!start && wantedTurn !== undefined) throw new Error(`No recorded turn ${wantedTurn}.`);
  const end = start && events.find(event => event.seq > start.seq && event.type === 'turn/end' && event.data.turn === start.data.turn);
  const stop = end ? end.seq + 1 : events.length;
  const owned = start ? events.slice(start.seq, stop) : [];
  const steps = owned.filter(event => event.type === 'step/start').map((event, i, starts) => {
    const members = owned.filter(member => member.seq >= event.seq && member.seq < (starts[i + 1]?.seq ?? stop));
    const results = new Map(members.filter(member => member.type === 'tool/result').map(member => [member.data.message.toolCallId, member]));
    return { number: event.data.step, seq: event.seq, members,
      calls: members.filter(member => member.type === 'tool/call').map(call => ({ ...call.data, seq: call.seq, result: results.get(call.data.callId) })) };
  });
  if (wantedStep !== undefined && !steps.some(step => step.number === wantedStep)) throw new Error(`No step ${wantedStep} in this turn.`);
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
  const report = { binding, title, turn: start?.data.turn, status: end?.data.reason?.kind ?? (start ? 'running or interrupted without a recorded end' : 'no model turn'),
    steps: steps.length, timeline: steps.map(step => {
      const messages = step.members.filter(event => ['assistant/message', 'assistant/attempt'].includes(event.type));
      const calls = step.calls.map(call => ({ name: call.name, command: commandOf(call),
        outcome: !call.result ? 'unconfirmed' : call.result.data.message.isError ? 'error' : 'completed' }));
      const ended = step.members.find(event => event.type === 'step/end');
      const begun = step.members[0];
      const output = messages.flatMap(settlementBlocks).find(block => block.type === 'text')?.text;
      return { step: step.number, seq: step.seq, revision: step.members.at(-1).seq, calls,
        label: calls.length ? calls.map(call => call.name === 'sts' ? call.command : call.name).join(', ')
          : output?.split('\n')[0].slice(0, 120) || '模型输出',
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
    recorded: latest ? { seq: latest.seq, turn: latest.data.turn, step: latest.data.step, text: textOf(latest.data.message) } : null };
  if (wantedStep !== undefined) {
    const step = steps.find(step => step.number === wantedStep);
    const before = gameResults.filter(event => event.seq < step.seq).at(-1);
    report.selected = { ...step, before, intervening: events.filter(event => event.seq > (before?.seq ?? -1)
      && event.seq < step.seq && event.type === 'tool/call' && event.data.name === 'sts') };
    report.following = steps[steps.indexOf(step) + 1] ?? null;
    if (input) {
      const deliveries = step.members.filter(event => event.type === 'session-log-deepseek/delivery-accepted');
      report.inputs = deliveries.map(event => {
        const { throughSeq, sessionId, sessionFormatVersion } = event.data;
        if (sessionId !== log.meta.id || sessionFormatVersion !== log.meta.version
            || !Number.isSafeInteger(throughSeq) || throughSeq < step.seq || throughSeq >= event.seq) {
          throw new Error(`Invalid delivery boundary at event ${event.seq}.`);
        }
        return { eventSeq: event.seq, throughSeq, precision: 'journal-derived-at-recorded-delivery', request: journalRequest(log, throughSeq) };
      });
    }
  }
  return report;
}

export function commandOf(call) {
  try { return JSON.parse(call.arguments).command ?? call.arguments; }
  catch { return call.arguments; }
}

export function renderNativeObservation(report) {
  const { binding } = report;
  const lines = [`${report.title}\nChat ${binding.agent}\nGame ${binding.session}`,
    `Turn ${report.turn ?? 'none'}: ${report.status} | ${report.steps} model steps | ${report.gameCalls} game calls | ${report.errors.length} errors`];
  if (report.selected) {
    const selected = report.selected;
    lines.push(`\nStep ${selected.number}`);
    if (report.inputs) {
      lines.push('Input reconstructed from the official journal at its recorded delivery boundary; no SDK checksum or provider HTTP payload was saved.');
      if (!report.inputs.length) lines.push('No delivery boundary recorded; the original input is unavailable.');
      for (const item of report.inputs) lines.push(`Delivery event ${item.eventSeq}, through event ${item.throughSeq}:`, JSON.stringify(item.request, null, 2));
    } else {
      lines.push(selected.before ? `Last successful game result before this step (event ${selected.before.seq}):\n${textOf(selected.before.data.message)}` : 'No earlier successful game result.');
      if (selected.intervening.length) lines.push('Intervening game calls may have changed the game; this earlier result is not a verified execution boundary.');
      for (const event of selected.members.filter(event => ['assistant/message', 'assistant/attempt'].includes(event.type))) {
        if (event.type === 'assistant/attempt') lines.push('Uncommitted assistant attempt:');
        for (const block of settlementBlocks(event).filter(block => ['reasoning', 'text'].includes(block.type))) lines.push(`${block.type}:\n${block.text}`);
      }
      for (const call of selected.calls) lines.push(`${call.name} ${commandOf(call)}\n${call.result ? textOf(call.result.data.message) : 'No result recorded; execution is unconfirmed.'}`);
      const following = report.following;
      if (following) {
        lines.push(`\nNext recorded model step ${following.number}:`);
        const messages = following.members.filter(event => ['assistant/message', 'assistant/attempt'].includes(event.type));
        if (!messages.length && !following.calls.length) lines.push('No model output or command recorded yet.');
        for (const event of messages) {
          if (event.type === 'assistant/attempt') lines.push('Uncommitted assistant attempt:');
          for (const block of settlementBlocks(event).filter(block => block.type === 'text')) lines.push(block.text);
        }
        for (const call of following.calls) lines.push(`Command: ${call.name} ${commandOf(call)}`);
      } else lines.push('\nNo following model step recorded in this turn.');
    }
    return lines.join('\n') + '\n';
  }
  if (report.accounting) {
    const a = report.accounting.totals, input = a.uncachedInputTokens + a.cacheReadTokens + a.cacheWriteTokens;
    lines.push(`Session totals through this turn: ${input + a.outputTokens} tokens | uncached input ${a.uncachedInputTokens} | cached input ${a.cacheReadTokens} | cache write ${a.cacheWriteTokens} | output ${a.outputTokens}`);
    const last = report.accounting.last;
    if (last) lines.push(`Last reported request input: ${last.uncachedInputTokens + last.cacheReadTokens + last.cacheWriteTokens} tokens`);
  } else lines.push('Token usage was not reported.');
  if (report.recorded) {
    const compact = report.recorded.text.split('\n').filter(line => /^(Act \d|Turn \d|Hand:|Deck:|Potions:|Relics:)/.test(line));
    lines.push(`Last recorded successful game result (step ${report.recorded.step}, event ${report.recorded.seq}; not a live read):\n${compact.length ? compact.join('\n') : report.recorded.text.split('\n')[0]}`);
  } else lines.push('No successful game result recorded.');
  if (report.intervening.length) lines.push('Later game calls failed or have no result; the recorded page may no longer describe the current game. Use observe live to check.');
  if (report.returnedAfterRecord) lines.push('The game was returned after this recorded page. Use observe live for the actual current state.');
  if (report.errors.length) {
    lines.push('Recent errors:');
    for (const call of report.errors.slice(-8)) {
      const text = textOf(call.result.data.message), reasons = text.split('\n').filter(line => line.startsWith('Error:'));
      lines.push(`  Step ${call.step}: ${commandOf(call)}\n    ${reasons.at(-1) ?? text.split('\n')[0]}${text.includes('Applied before error:') ? '\n    Earlier actions were reported as applied.' : ''}`);
    }
    if (report.errors.length > 8) lines.push(`${report.errors.length - 8} earlier errors omitted from this summary; select their step for full output.`);
  }
  if (report.pending.length) lines.push(`Unconfirmed tool calls: ${report.pending.map(call => `${call.name} at step ${call.step}`).join(', ')}`);
  lines.push('Full step: observe N | input: observe N input | current game and deck: observe live');
  return lines.join('\n') + '\n';
}

export async function readLiveGame(binding, repository, open) {
  open ??= (await import('../run_play/workbench.mjs')).openWorkbench;
  const game = await open({ repository, session: binding.session, manual: binding.manual ?? false, commandPrefix: '' });
  try {
    const before = await game.observe();
    const reading = await game.execute(before.position?.kind === 'terminal' ? 'read' : 'read, inventory');
    const after = await game.observe();
    if (before.head_revision !== after.head_revision || before.state_revision !== after.state_revision
        || before.position?.public_snapshot_id !== after.position?.public_snapshot_id) {
      throw new Error('Game changed during the live read; the combined page and inventory are not a consistent snapshot.');
    }
    return { headRevision: after.head_revision, stateRevision: after.state_revision,
      publicSnapshotId: after.position?.public_snapshot_id ?? null, text: reading.text };
  } finally { await game.close(); }
}

export async function observeNative({ repository, parameters = [] }, { openWorkbench } = {}) {
  repository = await realpath(repository);
  const options = parseNativeObservation(parameters);
  const file = join(repository, '.oracle-lab/run-play-records/native-observation-source.json');
  const saved = await optionalJson(file);
  if (saved && (saved.schema !== sourceSchema || typeof saved.home !== 'string' || typeof saved.chat !== 'string')) {
    throw new Error('Unsupported observation source configuration.');
  }
  const configuredHome = options.home ?? saved?.home ?? process.env.DSH_HOME;
  if (!configuredHome) throw new Error(`Select a dsh home with --home and a chat with --chat. ${usage}`);
  const home = await realpath(resolve(repository, configuredHome));
  const bindings = await nativeBindings(repository);
  if (options.mode === 'list') {
    const rows = [];
    for (const binding of bindings) {
      try {
        const log = await readPlayerJournal(home, binding.agent, { compression: 'zstd' });
        const last = log.events.filter(event => event.type === 'turn/start').at(-1);
        const title = log.events.filter(event => event.type === 'session/title').at(-1)?.data.title ?? binding.agent;
        rows.push(`${binding.agent} | ${title} | ${last ? `last turn ${last.data.turn}` : 'no model turn'}`);
      } catch (error) { if (error.name !== 'SessionPersistenceNotFoundError') throw error; }
    }
    return { text: rows.length ? rows.join('\n') + '\n' : 'No native game chats in this dsh home.\n' };
  }
  const selected = options.chat ?? (saved?.home && samePath(home, saved.home) ? saved.chat : null);
  if (!selected) throw new Error('Select a native game chat with --chat ID_FRAGMENT; observe list --home DSH_HOME lists them.');
  const matches = bindings.filter(binding => binding.agent === selected || binding.agent.includes(selected));
  if (matches.length !== 1) throw new Error(`Chat selector matched ${matches.length} native games; use a unique ID. Matches: ${matches.map(binding => binding.agent).join(', ')}`);
  const binding = matches[0], log = await readPlayerJournal(home, binding.agent, { compression: 'zstd' });
  const report = await nativeJournalSummary(log, binding, options);
  const live = options.mode === 'live' ? await readLiveGame(binding, repository, openWorkbench) : null;
  const text = renderNativeObservation(report) + (live ? `\nLive game read at h${live.headRevision}, state ${live.stateRevision}:\n${live.text}` : '');
  // Explicit source selection remembers only how to observe; gameplay focus,
  // the journal, the player prompt and game state are never updated here.
  if (options.home || options.chat) {
    await mkdir(join(repository, '.oracle-lab/run-play-records'), { recursive: true });
    await writeFile(file, JSON.stringify({ schema: sourceSchema, home: resolve(home), chat: binding.agent }, null, 2) + '\n');
  }
  return { report, text };
}
