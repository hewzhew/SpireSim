// Human observation of the selected chat, through the same journal interpreter
// as `observe`. Reads never become player messages or execute game choices.
import { Remote, TypertRemoteService } from '@deepseek-ai/dsh-typert-protocol';
import { SessionId } from '@deepseek-ai/dsh-session';
import { z } from 'zod';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createPresentation } from '@sts-simulator/presentation';
import { readNativeBinding, nativeBinding, nativeGameControl, workspaceState, workspaceFacts, isWorkspaceEvent } from './native.mjs';
import { nativeJournalSummary, commandOf, readLiveGame } from './native-inspection.mjs';
import { settlementBlocks } from './inspection.mjs';

// Register as a child plugin so Cordis owns replacement and disposal on reload.
export const inject = ['sessionController', 'sessionProjections', 'typert', 'agents'];

const textOf = message => (message?.content ?? []).filter(block => block.type === 'text').map(block => block.text).join('\n');
const positive = value => Number.isSafeInteger(value) && value > 0;
const run = promisify(execFile);
function viewText(locale, key) { return createPresentation({ locale }).text(`native:observation.${key}`); }
export function observationOverview(report) {
  return { status: 'ready', turn: report.turn ?? null, phase: report.status,
    timeline: report.timeline, turns: report.turnNumbers, gameCalls: report.gameCalls,
    errors: report.timeline.flatMap(item => item.calls).filter(call => call.outcome === 'error').length,
    pending: report.pending.length, accounting: report.accounting,
    recordedMayBeStale: report.intervening.length > 0 || report.returnedAfterRecord };
}

export function observationStep(report) {
  const selected = report.selected;
  return { status: 'ready', turn: report.turn, step: selected.number,
    messages: selected.members.filter(event => ['assistant/message', 'assistant/attempt'].includes(event.type)).map(event => ({
      committed: event.type === 'assistant/message', blocks: settlementBlocks(event).filter(block => ['reasoning', 'text'].includes(block.type)),
    })),
    calls: selected.calls.map(call => ({ name: call.name, command: commandOf(call),
      outcome: !call.result ? 'unconfirmed' : call.result.data.message.isError ? 'error' : 'completed',
      text: call.result ? textOf(call.result.data.message) : null })),
    ...(report.inputs ? { inputs: report.inputs } : {}) };
}

const observedEvents = new Set(['turn/start', 'turn/end', 'step/start', 'tool/call', 'tool/result',
  'assistant/message', 'assistant/attempt', 'session/title', 'llm/retry-started', 'agent/inbox/spliced']);
export const observationTickProjection = {
  key: 'stsObservationTick', stateVersion: 1, stateSchema: z.number(), init: () => -1,
  apply: (state, event) => observedEvents.has(event.type) || isWorkspaceEvent(event) ? event.seq : state,
  wire: { viewSchema: z.number(), view: state => state },
};

const initializers = [];
export class StsObservationView extends TypertRemoteService {
  constructor(ctx, { openWorkbench, inspectSession, execute = run, resolveAgent } = {}) {
    super(ctx, 'stsObservation');
    this.openWorkbench = openWorkbench;
    this.inspectSession = inspectSession ?? (id => ctx.sessionController.inspect(SessionId(id)));
    this.execute = execute;
    this.resolveAgent = resolveAgent ?? (id => ctx.sessionController.resolveAgent(SessionId(id)));
    for (const initialize of initializers) initialize.call(this);
  }
  async journal(sessionId, locale = 'en') {
    if (typeof sessionId !== 'string' || !sessionId) throw new Error(viewText(locale, 'sessionRequired'));
    const log = await this.inspectSession(sessionId);
    if (log.meta.id !== sessionId) throw new Error(viewText(locale, 'journalMismatch'));
    const agent = { id: sessionId, session: { header: log.meta } };
    const binding = await readNativeBinding(agent);
    if (!binding) return null;
    return { binding, log, agent };
  }
  // The Remote gateway requires identifier-only parameters, without defaults.
  async read(sessionId, turn, locale) {
    locale ??= 'en';
    turn ??= null;
    if (turn !== null && !positive(turn)) throw new Error(viewText(locale, 'positiveTurn'));
    const source = await this.journal(sessionId, locale);
    if (!source) return { status: 'not-started' };
    const report = await nativeJournalSummary(source.log, source.binding, { ...(turn === null ? {} : { turn }), locale });
    const owner = this.ctx.agents?.get(SessionId(sessionId)), live = nativeGameControl(owner);
    return { ...observationOverview(report), throughSeq: source.log.events.length - 1,
      control: live ? { ...live.status(), available: true } : { paused: workspaceState(source.log.events),
        running: owner?.status === 'running', queued: null, available: !owner },
      interventions: workspaceFacts(source.log.events).slice(-20) };
  }
  async step(sessionId, turn, step, input, locale) {
    locale ??= 'en';
    input ??= false;
    if (!positive(turn) || !positive(step) || typeof input !== 'boolean') throw new Error(viewText(locale, 'validSelection'));
    const source = await this.journal(sessionId, locale);
    if (!source) return { status: 'not-started' };
    const report = await nativeJournalSummary(source.log, source.binding, { turn, step, input, locale });
    return { ...observationStep(report), throughSeq: source.log.events.length - 1 };
  }
  async live(sessionId, locale) {
    locale ??= 'en';
    const source = await this.journal(sessionId, locale);
    if (!source) return { status: 'not-started' };
    const control = nativeGameControl(this.ctx.agents?.get(SessionId(sessionId)));
    if (control) return { status: 'ready', text: await control.live() };
    const binding = await nativeBinding(source.agent, {}, { create: false });
    return { status: 'ready', ...await readLiveGame(binding, binding.repository, this.openWorkbench, locale) };
  }
  async points(sessionId, locale) {
    locale ??= 'en';
    const source = await this.journal(sessionId, locale);
    if (!source) return { status: 'not-started' };
    const control = nativeGameControl(this.ctx.agents?.get(SessionId(sessionId)));
    if (control) return { status: 'ready', ...await control.points() };
    // Cold inspection does not attach an Agent, open a game writer or wake input.
    const binding = await nativeBinding(source.agent, {}, { create: false });
    const { stdout } = await this.execute(binding.binary, ['back', '--session', binding.session, '--list', '--json'],
      { cwd: binding.repository, windowsHide: true, timeout: 15000, maxBuffer: 8 * 1024 * 1024 });
    return { status: 'ready', points: JSON.parse(stdout),
      control: { paused: workspaceState(source.log.events), running: false, queued: null } };
  }
  async controller(sessionId, locale = 'en') {
    if (!await this.journal(sessionId, locale)) throw new Error(viewText(locale, 'noGame'));
    const resolved = await this.resolveAgent(sessionId);
    if (resolved.error) throw new Error(resolved.error.message);
    if (String(resolved.agent.id) !== sessionId) throw new Error(viewText(locale, 'ownerMismatch'));
    const control = nativeGameControl(resolved.agent);
    if (!control) throw new Error(viewText(locale, 'missingControl'));
    return control;
  }
  async pause(sessionId, locale) {
    locale ??= 'en';
    const control = await this.controller(sessionId, locale), result = await control.pause();
    return { ...result, control: control.status(), throughSeq: control.agent.session.seq - 1 };
  }
  async restore(sessionId, request, locale) {
    locale ??= 'en';
    if (request?.schema !== 'run-play-return-request-v1' || !request.source || !request.target) {
      throw new Error(viewText(locale, 'completeReturnIdentity'));
    }
    const control = await this.controller(sessionId, locale), result = await control.restore(request);
    return { ...result, control: control.status(), throughSeq: control.agent.session.seq - 1 };
  }
  async resume(sessionId, locale) {
    locale ??= 'en';
    const control = await this.controller(sessionId, locale), result = await control.resume();
    return { ...result, control: control.status(), throughSeq: control.agent.session.seq - 1 };
  }
}
for (const name of ['read', 'step', 'live', 'points', 'pause', 'restore', 'resume']) Remote(StsObservationView.prototype[name], {
  kind: 'method', name, static: false, private: false, addInitializer: initialize => initializers.push(initialize),
});

export function apply(ctx) {
  new StsObservationView(ctx);
  ctx.effect(() => ctx.sessionProjections.register(observationTickProjection));
}
