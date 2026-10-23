// Transport and returned-object bindings only. Rust owns state, replay and adoption.
import { spawn } from 'node:child_process';
import { mkdir, writeFile, appendFile, realpath } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { createInterface } from 'node:readline';
import { inspect } from 'node:util';
import { platform } from 'node:os';
import { createPresentation, resolveLocale } from '@sts-simulator/presentation';

const clientViews = Object.fromEntries(['en', 'zh-CN'].map(locale => [locale, createPresentation({ locale })]));
const clientText = (locale, key, parameters = {}) => clientViews[resolveLocale(locale)].text(`workbench:transport.${key}`,
  Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));

// One transport per filesystem session in this Node host, including module
// reloads. This registry never owns simulator state or resumes a closed process.
const registryKey = Symbol.for('sts.run-play.connections.v1');
const connections = globalThis[registryKey] ??= new Map();

export class RunPlayCommandError extends Error {
  constructor(error, request, recordDirectory, locale = 'en') {
    super(error?.message ?? clientText(locale, 'commandFailed'));
    this.name = 'RunPlayCommandError';
    this.code = error?.code ?? 'RUN_PLAY_COMMAND_FAILED';
    this.command = request.command;
    this.requestId = request.request_id;
    this.recordDirectory = recordDirectory;
  }
}

async function sessionKey(session, locale = 'en') {
  if (!session) throw new Error(clientText(locale, 'sessionRequired'));
  const path = await realpath(resolve(session));
  return platform() === 'win32' ? path.toLowerCase() : path;
}

export async function getRunPlay(session, { locale = 'en' } = {}) {
  const entry = connections.get(await sessionKey(session, locale));
  if (!entry) throw new Error(clientText(locale, 'noOpenConnection'));
  if (!entry.client) throw new Error(clientText(locale, 'connectionOpening'));
  return entry.client;
}

export async function openRunPlay(options) {
  const locale = options.locale ?? 'en';
  const key = await sessionKey(options.session, locale);
  if (connections.has(key)) throw new Error(clientText(locale, 'connectionExists'));
  const entry = {};
  connections.set(key, entry);
  const release = () => { if (connections.get(key) === entry) connections.delete(key); };
  try {
    entry.client = await connectRunPlay(options, release);
    return entry.client;
  } catch (error) {
    release();
    throw error;
  }
}

async function connectRunPlay({ repository, session, recordDirectory, binary, manual = false, locale = 'en', timeoutMs = 30000, closeTimeoutMs = 2000, onTransition }, release) {
  if (!repository || !session || !recordDirectory) throw new Error(clientText(locale, 'connectionArgumentsRequired'));
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1) throw new Error(clientText(locale, 'invalidTimeout'));
  if (!Number.isSafeInteger(closeTimeoutMs) || closeTimeoutMs < 1) throw new Error(clientText(locale, 'invalidCloseTimeout'));
  if (!['en', 'zh-CN'].includes(locale)) throw new Error(clientText(locale, 'unsupportedLocale'));
  const records = resolve(recordDirectory);
  await mkdir(records); // Never mix a new connection into an old transcript.
  const child = spawn(binary ?? join(repository, 'target/dev-opt/run_play.exe'),
    ['agent-shell', '--session', resolve(session), '--locale', locale, ...(manual ? ['--manual'] : [])], {
      cwd: resolve(repository), windowsHide: true, shell: false,
      stdio: ['pipe', 'pipe', 'pipe'],
    });
  let pending, failure, closing = false, nextId = 0, epoch = 0, diagnosticTail = '';
  let closedResolve, closeTimer, closeError, recordError;
  let stderrReceived = false, stderrRecording = Promise.resolve();
  const closed = new Promise(resolve => { closedResolve = resolve; });
  const fail = error => {
    error.transportFailed = true;
    error.code ??= 'RUN_PLAY_CONNECTION_ERROR';
    error.recordDirectory ??= records;
    error.outcomeUncertain ??= Boolean(pending?.sent);
    if (pending) {
      error.command ??= pending.request.command;
      error.requestId ??= pending.id;
    }
    failure ??= error;
    if (pending) {
      clearTimeout(pending.timer);
      pending.reject(failure);
      pending = undefined;
    }
    child.kill();
  };
  child.on('error', fail);
  child.stdin.on('error', fail);
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', chunk => {
    diagnosticTail = (diagnosticTail + chunk).slice(-8192);
    const first = !stderrReceived;
    stderrReceived = true;
    stderrRecording = stderrRecording.then(() => first
      ? writeFile(join(records, 'stderr.txt'), chunk, { flag: 'wx' })
      : appendFile(join(records, 'stderr.txt'), chunk)).catch(error => {
        recordError ??= error;
        fail(error);
      });
  });
  const lines = createInterface({ input: child.stdout });
  child.on('close', async code => {
    clearTimeout(closeTimer);
    if (!closing || code !== 0 || pending) {
      const diagnostic = diagnosticTail.trim();
      const summary = diagnostic ? `: ${diagnostic.slice(-1024)}` : '';
      fail(Object.assign(new Error(clientText(locale, 'connectionClosed', { code: code, summary: summary })), {
        exitCode: code, stderrTail: diagnosticTail,
      }));
    }
    try {
      await stderrRecording;
      if (!stderrReceived) await writeFile(join(records, 'stderr.txt'), '', { flag: 'wx' });
    } catch (error) { recordError ??= error; }
    finally {
      release();
      closedResolve();
    }
  });
  lines.on('line', async line => {
    const active = pending;
    if (!active || active.received) {
      fail(new Error(clientText(locale, 'unexpectedResponse')));
      return;
    }
    active.received = true;
    try {
      // Keep the exact wire, even if protocol validation fails. Never strip
      // terminal escapes to make malformed machine output look valid.
      await appendFile(join(records, `${active.id}.jsonl`), line + '\n');
      let response;
      try { response = JSON.parse(line); }
      catch { throw new Error(clientText(locale, 'invalidJson')); }
      if (response.schema !== 'sts-run-play-agent-response-v1' || response.request_id !== active.id) {
        throw new Error(clientText(locale, 'responseIdentityMismatch'));
      }
      if (!['ok', 'error'].includes(response.status)) throw new Error(clientText(locale, 'invalidResponseStatus'));
      if (pending !== active) return; // A timeout already made the outcome uncertain.
      clearTimeout(active.timer);
      pending = undefined;
      if (response.status === 'error') {
        active.reject(new RunPlayCommandError(response.error, active.request, records, locale));
      } else {
        active.resolve(response.result);
      }
    } catch (error) { fail(error); }
  });

  function ready() {
    if (failure) throw failure;
    if (closing) throw new Error(clientText(locale, 'connectionClosing'));
    if (pending) throw new Error(clientText(locale, 'operationPending'));
  }

  async function request(command, fields = {}, expectedWorkMs = 0) {
    ready();
    const id = `exchange-${String(nextId++).padStart(3, '0')}`;
    const request = { schema: 'sts-run-play-agent-request-v1', request_id: id, command, ...fields };
    return new Promise((resolve, reject) => {
      const active = { id, request, resolve, reject, received: false, sent: false,
        timer: setTimeout(() => fail(Object.assign(new Error(clientText(locale, 'responseTimeout')), { code: 'RUN_PLAY_TIMEOUT' })), timeoutMs + expectedWorkMs) };
      pending = active;
      const wire = JSON.stringify(request) + '\n';
      // Record before sending, including requests whose response never arrives.
      writeFile(join(records, `${id}.jsonl`), wire, { flag: 'wx' }).then(() => {
        if (pending === active) {
          active.sent = true;
          child.stdin.write(wire);
        }
      }).catch(fail);
    });
  }

  // Observe both semantic actions and source-bound choices at their shared
  // execution boundary. The optional observer never replaces a native reply.
  async function observedRequest(command, fields, metadata = {}) {
    if (!onTransition) return request(command, fields);
    const before = freeze((await request('observe')).observation);
    const input = freeze(JSON.parse(JSON.stringify({ command, ...fields })));
    const context = await onTransition({ phase: 'before', before, input, ...metadata });
    const result = await request(command, fields);
    try {
      const after = freeze(result.observation ?? (await request('observe')).observation);
      await onTransition({ phase: 'committed', before, after, input, context, ...metadata });
    } catch (error) {
      // A capture failure must not be mistaken for an unexecuted action.
      error.actionCommitted = true;
      throw error;
    }
    return result;
  }

  function frame(result) {
    freeze(result);
    const reading = result.reading;
    const view = reading?.view ?? result.view;
    if (typeof view !== 'string' || /[\x00-\x08\x0b-\x1f\x7f]/.test(view)) {
      throw new Error(clientText(locale, 'invalidReading'));
    }
    const value = { view };
    Object.defineProperty(value, 'raw', { value: result });
    Object.defineProperty(value, inspect.custom, { value: () => view.trimEnd() });
    if (reading) {
      value.control = reading.control;
      value.summary = reading.summary;
      value.options = Object.freeze(reading.options.map(option => Object.freeze({
        label: option.label,
        [inspect.custom]: () => option.label,
        async choose({ retainContext = false } = {}) {
          // Retain the original source even after a later view is returned.
          ready();
          epoch++;
          return frame(await observedRequest('choose', {
            selection: { source: reading.source, choice: option.choice },
            ...(retainContext ? { known_context: reading.context } : {}),
          }, { candidate: option.candidate }));
        },
      })));
      if (reading.can_collect_rewards) {
        value.collectRewards = async (chosen, { retainContext = false } = {}) => {
          if (!Array.isArray(chosen) || chosen.length < 1 || chosen.length > 8) {
            throw new Error(clientText(locale, 'rewardArrayRequired'));
          }
          const indices = chosen.map(option => value.options.indexOf(option));
          if (indices.includes(-1)) throw new Error(clientText(locale, 'foreignReward'));
          if (new Set(indices).size !== indices.length) throw new Error(clientText(locale, 'duplicateReward'));
          ready();
          epoch++;
          return frame(await observedRequest('choose', {
            selection: { source: reading.source, choice: reading.options[indices[0]].choice },
            following_rewards: indices.slice(1).map(i => reading.options[i].choice),
            ...(retainContext ? { known_context: reading.context } : {}),
          }, { candidates: indices.map(i => reading.options[i].candidate) }));
        };
      }
      value.selections = Object.freeze((reading.selections ?? []).map(selection => {
        const members = Object.freeze(selection.members.map(member => Object.freeze({
          label: member.label, [inspect.custom]: () => member.label,
        })));
        return Object.freeze({
          label: selection.label, min: selection.min, max: selection.max, members,
          [inspect.custom]: () => selection.label,
          async select(chosen) {
            const indices = selectionIndices(chosen, members, selection.min, selection.max, locale);
            ready();
            epoch++;
            return frame(await observedRequest('choose', {
              selection: { source: reading.source, choice: {
                ordinal_path: [...selection.choice.ordinal_path, ...indices.map(i => selection.members[i].ordinal)],
              } },
            }));
          },
        });
      }));
    }
    return Object.freeze(value);
  }

  function draftCheck() {
    const sourceEpoch = epoch;
    return () => {
      ready();
      if (epoch !== sourceEpoch) throw new Error(clientText(locale, 'draftExpired'));
    };
  }

  function draftOptions(options, frameRef, check) {
    return Object.freeze(options.map(option => Object.freeze({
      ref: option.option_ref, label: option.label, boundary: option.boundary,
      actionCount: option.action_count,
      successor: option.successor,
      ...(option.preferred === undefined ? {} : { preferred: option.preferred }),
      ...(option.equivalence === undefined ? {} : { equivalence: option.equivalence }),
      [inspect.custom]: () => option.label,
      async review() {
        check();
        return frame(await request('combat_draft_inspect', {
          frame_ref: frameRef, option_ref: option.option_ref,
        }));
      },
      async choose() {
        check();
        epoch++;
        return draftFrame(await request('combat_draft_choose', {
          frame_ref: frameRef, option_ref: option.option_ref,
        }));
      },
    })));
  }

  async function advanceDraft({ generationWork = 1024, engineSteps = 65536 } = {}) {
    ready();
    if (!Number.isSafeInteger(generationWork) || generationWork < 0 || generationWork > 65536 ||
        !Number.isSafeInteger(engineSteps) || engineSteps < 0 || engineSteps > 33554432) {
      throw new Error(clientText(locale, 'invalidDraftBudget'));
    }
    epoch++;
    return request('combat_draft_advance', {
      generation_work: generationWork, engine_steps: engineSteps,
    });
  }

  function selectionFrame(result) {
    const base = frame(result);
    const check = draftCheck();
    const value = { ...base, raw: base.raw, [inspect.custom]: () => base.view.trimEnd() };
    value.sources = Object.freeze(result.sources.map(source => Object.freeze({
      label: source.label, actionCount: source.action_count,
      [inspect.custom]: () => source.label,
      async open() {
        check();
        return selectionFrame(await request('combat_draft_selection', { source_ref: source.source_ref }));
      },
    })));
    value.families = Object.freeze(result.families.map(family => {
      const members = Object.freeze(family.members.map(member => Object.freeze({
        label: member.label, [inspect.custom]: () => member.label,
      })));
      return Object.freeze({
        label: family.label, min: family.min, max: family.max, members,
        [inspect.custom]: () => family.label,
        async prefer(chosen) {
          check();
          const indices = selectionIndices(chosen, members, family.min, family.max, locale);
          epoch++;
          return selectionFrame(await request('combat_draft_selection', { edit: {
            source_ref: result.source_ref,
            preferences: [{ family: family.family, choices: indices.map(i => family.members[i].ordinal) }],
          } }));
        },
      });
    }));
    value.outcomes = draftOptions(result.outcomes, result.frame_ref, check);
    if (result.source_ref) {
      value.clearPreferences = async () => {
        check();
        epoch++;
        return selectionFrame(await request('combat_draft_selection', {
          edit: { source_ref: result.source_ref, preferences: [] },
        }));
      };
      value.advance = async budget => {
        check();
        await advanceDraft(budget);
        return selectionFrame(await request('combat_draft_selection', { source_ref: result.source_ref, offset: 0 }));
      };
    }
    if (result.next_offset !== undefined) {
      value.next = async () => {
        check();
        return selectionFrame(await request('combat_draft_selection', {
          ...(result.source_ref ? { source_ref: result.source_ref } : {}), offset: result.next_offset,
        }));
      };
    }
    return Object.freeze(value);
  }

  function draftFrame(result) {
    const base = frame(result);
    const check = draftCheck();
    const value = { ...base, raw: base.raw, [inspect.custom]: () => base.view.trimEnd() };
    value.options = draftOptions(result.options ?? [], result.frame_ref, check);
    if (result.frame_ref || result.draft_present === false) {
      value.advance = async (budget) => {
        check();
        return draftFrame(await advanceDraft(budget));
      };
    }
    if (result.frame_ref) {
      value.selections = async () => {
        check();
        return selectionFrame(await request('combat_draft_selection'));
      };
    }
    if (result.root_head_revision !== undefined) {
      value.undo = async () => {
        check();
        epoch++;
        return draftFrame(await request('combat_draft_undo'));
      };
    }
    if (result.commit_candidate_ref) {
      value.commit = async () => {
        check();
        epoch++;
        return frame(await request('commit_combat_plan', {
          candidate_ref: result.commit_candidate_ref, effort: 'standard',
        }));
      };
    }
    return Object.freeze(value);
  }

  function plansFrame(result, effort, sourceEpoch, canDraft = false) {
    if (typeof result.report_id !== 'string' || !result.report_id) {
      throw new Error(clientText(locale, 'missingReportIdentity'));
    }
    const identity = { report_id: result.report_id };
    const base = frame(result);
    const check = () => {
      if (epoch !== sourceEpoch) throw new Error(clientText(locale, 'candidateExpired'));
    };
    async function review(chosen, { turn, includeSteps = false, compact = false } = {}) {
      check();
      if (!Array.isArray(chosen) || chosen.length < 1 || chosen.length > 8) {
        throw new Error(clientText(locale, 'reviewArrayRequired'));
      }
      for (const candidate of chosen) {
        if (!candidates.includes(candidate)) throw new Error(clientText(locale, 'foreignCandidate'));
      }
      if (new Set(chosen).size !== chosen.length) throw new Error(clientText(locale, 'duplicateReviewCandidate'));
      return frame(await request('combat_review', {
        candidate_refs: chosen.map(candidate => candidate.ref), effort, include_steps: includeSteps,
        ...identity,
        ...(turn === undefined ? {} : { turn }),
        ...(compact ? { compact: true } : {}),
      }));
    }
    const candidates = Object.freeze(result.candidates.map(candidate => {
      if (typeof candidate.plan_id !== 'string' || !/^c(?:0|[1-9]\d*)@[a-f0-9]{12}$/.test(candidate.plan_id)
          || candidate.plan_id.split('@')[0] !== candidate.candidate_ref) {
        throw new Error(clientText(locale, 'candidatePlanIdentityMismatch'));
      }
      const option = {
        ref: candidate.candidate_ref,
        planId: candidate.plan_id,
        actionCount: candidate.plan_steps,
        minimumObservedHp: candidate.minimum_observed_hp,
        nextIncenseTurn: candidate.next_incense_turn,
        outcome: candidate.outcome,
        meetsRequestedSatisfaction: candidate.meets_requested_satisfaction,
        [inspect.custom]: () => candidate.plan_id,
        async review(options) {
          return review([option], options);
        },
        async draft({ turn, replace = false } = {}) {
          check();
          ready();
          if (!canDraft) throw new Error(clientText(locale, 'liveResearchRequired'));
          if (!Number.isSafeInteger(turn) || turn < 1 || turn > 0xffffffff || typeof replace !== 'boolean') {
            throw new Error(clientText(locale, 'invalidDraftOpening'));
          }
          const opened = await request('combat_draft_open', {
            candidate_ref: candidate.candidate_ref, effort, turn, replace,
          });
          epoch++;
          return draftFrame(opened);
        },
        async commit() {
          check();
          ready();
          epoch++;
          return frame(await request('commit_combat_plan', { candidate_ref: candidate.candidate_ref, effort, ...identity }));
        },
      };
      return Object.freeze(option);
    }));
    return Object.freeze({ ...base, candidates, review, effort, raw: base.raw, [inspect.custom]: () => base.view.trimEnd() });
  }

  return Object.freeze({
    async info(query, { locale: infoLocale = locale } = {}) {
      if (typeof query !== 'string' || !query.trim()) {
        throw new Error(clientText(locale, 'mechanicsQueryRequired'));
      }
      if (!['en', 'zh-CN'].includes(infoLocale)) throw new Error(clientText(locale, 'unsupportedLocale'));
      return frame(await request('info', { query, locale: infoLocale }));
    },
    async slPiles() {
      return frame(await request('sl_piles'));
    },
    async gameFacts(entities) {
      if (!Array.isArray(entities) || entities.length < 1 || entities.length > 128) {
        throw new Error(clientText(locale, 'typedFactsQueriesRequired'));
      }
      const result = await request('game_facts', { entities });
      if (result?.kind !== 'game_facts' || !Array.isArray(result.facts)) {
        throw new Error(clientText(locale, 'missingGameFacts'));
      }
      return freeze(result.facts);
    },
    async observe() {
      const result = await request('observe');
      if (result?.kind !== 'observation' || !result.observation) {
        throw new Error(clientText(locale, 'missingObservation'));
      }
      return freeze(result.observation);
    },
    async draft() {
      return draftFrame(await request('combat_draft_status'));
    },
    async decision({ details = [], knownContext } = {}) {
      return frame(await request('decision', { details, ...(knownContext ? { known_context: knownContext } : {}) }));
    },
    // Semantic action line in the same language as the act CLI; one
    // transactional request, so an illegal step leaves the head unchanged.
    // Multi-action lines must advance by exactly one public action on the
    // server, so that mode is the default whenever more than one action is
    // given. Returns the reading after the line committed.
    async act(actions, { advance } = {}) {
      ready();
      if (!Array.isArray(actions) || actions.length < 1
        || actions.some(action => typeof action !== 'string' || action === '')) {
        throw new Error(clientText(locale, 'actionsRequired'));
      }
      if (advance === undefined && actions.length > 1) advance = 'one_public_action';
      if (advance !== undefined && typeof advance !== 'string') {
        throw new Error(clientText(locale, 'invalidAdvance'));
      }
      epoch++;
      const acted = await observedRequest('act', {
        actions,
        ...(advance === undefined ? {} : { advance }),
      });
      if (acted?.kind !== 'acted' || !acted.reading) {
        throw new Error(clientText(locale, 'missingActedEnvelope'));
      }
      return frame(acted);
    },
    async research({ effort = 'whole_combat', maxSearchMs = 10000, maxGenerationWork, maxPotionsUsed } = {}) {
      ready();
      if (!Number.isSafeInteger(maxSearchMs) || maxSearchMs < 1 || maxSearchMs > 20000) {
        throw new Error(clientText(locale, 'invalidSearchTime'));
      }
      if (maxGenerationWork !== undefined && (effort !== 'whole_combat'
          || !Number.isSafeInteger(maxGenerationWork) || maxGenerationWork < 1 || maxGenerationWork > 1000000)) {
        throw new Error(clientText(locale, 'invalidGenerationWork'));
      }
      if (maxPotionsUsed !== undefined && (!Number.isSafeInteger(maxPotionsUsed)
          || maxPotionsUsed < 0 || maxPotionsUsed > 4294967295)) {
        throw new Error(clientText(locale, 'invalidPotionBudget'));
      }
      const sourceEpoch = ++epoch;
      const result = await request('combat_research', { effort, max_search_ms: maxSearchMs,
        ...(maxGenerationWork === undefined ? {} : { max_generation_work: maxGenerationWork }),
        ...(maxPotionsUsed === undefined ? {} : { max_potions_used: maxPotionsUsed }) }, maxSearchMs);
      return plansFrame(result, effort, sourceEpoch, true);
    },
    async plans({ diagnostics = false } = {}) {
      ready();
      if (typeof diagnostics !== 'boolean') throw new Error(clientText(locale, 'invalidDiagnostics'));
      const sourceEpoch = ++epoch;
      const result = await request('combat_plans', diagnostics ? { diagnostics } : {});
      return plansFrame(result, result.effort, sourceEpoch);
    },
    async preview(actions) {
      if (!Array.isArray(actions) || actions.length < 1 || actions.some(a => typeof a !== 'string' || !a)) {
        throw new Error(clientText(locale, 'previewActionsRequired'));
      }
      const result = await request('probe', { lines: [actions] });
      if (result.durable_head_unchanged !== true || result.lines?.length !== 1) {
        throw new Error(clientText(locale, 'invalidPreviewResponse'));
      }
      const line = result.lines[0];
      const stop = line.stop;
      const nonnegativeInteger = value => Number.isSafeInteger(value) && value >= 0;
      if (!stop || !['complete', 'terminal', 'unresolved'].includes(stop.kind)
        || (stop.kind !== 'complete' && !nonnegativeInteger(stop.unapplied_actions))
        || (stop.kind === 'unresolved' && (!Number.isSafeInteger(stop.requested_step)
          || stop.requested_step < 1 || typeof stop.error !== 'string'))) {
        throw new Error(clientText(locale, 'invalidPreviewStop'));
      }
      return Object.freeze({ ...frame({ view: line.view }), endpoint: freeze(line.endpoint),
        stop: freeze(line.stop), preview: true });
    },
    async close() {
      if (pending) throw new Error(clientText(locale, 'pendingBeforeClose'));
      if (!failure && !closing) {
        closing = true;
        child.stdin.end();
        closeTimer = setTimeout(() => {
          closeError = new Error(clientText(locale, 'closeTimeout'));
          child.kill();
        }, closeTimeoutMs);
      }
      await closed;
      if (recordError) throw recordError;
      if (closeError) throw closeError;
    },
  });
}

function freeze(value) {
  if (value && typeof value === 'object') {
    for (const child of Object.values(value)) freeze(child);
    Object.freeze(value);
  }
  return value;
}

function selectionIndices(chosen, members, min, max, locale = 'en') {
  if (!Array.isArray(chosen)) throw new Error(clientText(locale, 'selectionArrayRequired'));
  const indices = chosen.map(member => members.indexOf(member));
  if (indices.includes(-1)) throw new Error(clientText(locale, 'foreignSelectionMember'));
  if (new Set(indices).size !== indices.length) throw new Error(clientText(locale, 'duplicateSelectionMember'));
  if (indices.length < min || indices.length > max) throw new Error(clientText(locale, 'selectionCountOutOfBounds'));
  return indices;
}
