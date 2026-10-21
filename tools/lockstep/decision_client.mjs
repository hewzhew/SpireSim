// Transport and immutable view bindings only. Rust owns the workspace, edits,
// source validation and proposals. Suitable for a persistent node_repl caller.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { join } from 'node:path';

export class DecisionOperationError extends Error {
  constructor(view) {
    super(view.error ?? `decision operation ${view.status}; inspect error.view`);
    this.name = 'DecisionOperationError';
    this.status = view.status;
    // The receipt may describe a partial edit or a stopped Java successor.
    // Keep it available without treating a failed operation as normal control flow.
    Object.defineProperty(this, 'view', { value: view });
  }
}

export function openDecisionWorkspace({ repository, reference, at, binary, display, timeoutMs = 30000 }) {
  if (reference !== undefined && at !== undefined) throw new Error('select a workspace or a source coordinate');
  const args = ['workspace'];
  if (reference !== undefined) args.push(reference);
  if (at !== undefined) {
    if (!Number.isSafeInteger(at) || at < 0) throw new Error('invalid source coordinate');
    args.push('--at', String(at));
  }
  return openConnection({ repository, binary, display, timeoutMs }, args, false);
}

export function openLiveSession({ repository, binary, display, timeoutMs = 30000, native = false, at }) {
  if (typeof native !== 'boolean') throw new Error('native must be a boolean');
  if (at !== undefined && (native || !at || typeof at !== 'object' ||
      Object.keys(at).some(key => !['session', 'actions'].includes(key)) ||
      typeof at.session !== 'string' || !at.session || at.session.includes(':') ||
      !Number.isSafeInteger(at.actions) || at.actions < 1)) {
    throw new Error('use one explicit adopted source {session, actions}, without native');
  }
  return openConnection({ repository, binary, display, timeoutMs },
    ['manual-resume', ...(native ? ['--native'] : []), ...(at ? ['--at', `${at.session}:${at.actions}`] : []), '--json'], true);
}

function openConnection({ repository, binary, display, timeoutMs }, args, live) {
  if (!repository) throw new Error('repository is required');
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1) throw new Error('invalid timeout');
  if (display !== undefined && typeof display !== 'function') throw new Error('display must be a function');
  args.push('--repository', repository);
  const child = spawn(binary ?? join(repository, 'target', 'debug', 'sts_lockstep.exe'), args, {
    cwd: repository, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
  });
  const lines = createInterface({ input: child.stdout });
  let diagnosticTail = '';
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', chunk => { diagnosticTail = (diagnosticTail + chunk).slice(-8192); });
  let nextId = 1;
  let inFlight;
  let dead;
  let workspace;
  let lastReply;
  const statistics = { requestsSent: 0, repliesReceived: 0, responseBytes: 0 };
  const fail = error => {
    dead ??= error;
    inFlight?.reject(error);
  };
  const complete = (pending, reply) => {
    const view = makeView(reply);
    inFlight = undefined;
    if (reply.status !== 'ok') pending.reject(new DecisionOperationError(view));
    else pending.resolve(view);
  };
  child.on('error', fail);
  child.stdin.on('error', fail);
  // `exit` can precede buffered stdout. `close` guarantees the last receipt was
  // drained, and a successful close releases the writer before a caller reopens.
  child.on('close', (code, signal) => {
    if (inFlight?.closeReply) complete(inFlight, inFlight.closeReply);
    const reason = code !== 0 ? diagnosticTail.trim().split(/\r?\n/).at(-1)?.slice(0, 400) : undefined;
    fail(new Error(`connection ended (${code ?? signal})${reason ? ': ' + reason : ''}; inspect connection.diagnostics() for details`));
    lines.close();
  });
  lines.on('line', line => {
    try {
      const reply = JSON.parse(line);
      if (!inFlight || reply.id !== inFlight.id) throw new Error('unexpected workspace response; stop and reopen');
      reply.roundTripMs = performance.now() - inFlight.started;
      deepFreeze(reply);
      statistics.repliesReceived++;
      statistics.responseBytes += Buffer.byteLength(line, 'utf8');
      workspace = reply.workspace;
      lastReply = reply;
      const pending = inFlight;
      if (pending.operation === 'close') {
        pending.closeReply = reply;
        child.stdin.end();
      } else complete(pending, reply);
    } catch (error) { fail(error); child.stdin.end(); }
  });

  function makeView(reply) {
    // Each closure retains the exact returned scope. A stale view is NEVER
    // silently rebound to whichever scope happens to be current now.
    const scope = reply.view;
    const call = operation => request(operation, scope);
    const view = {
      status: reply.status, summary: reply.summary, error: reply.error,
      execution: reply.execution && Object.freeze(reply.execution),
      state: reply.state && Object.freeze(reply.state),
      workspace: reply.workspace, stop: reply.stop, selectedActions: reply.selected_actions,
      node: reply.node && Object.freeze(reply.node),
      bookmarks: reply.bookmarks && Object.freeze(reply.bookmarks.map(node => Object.freeze(node))),
      comparison: reply.comparison && Object.freeze(reply.comparison),
      research: reply.research && compactObject({ ...reply.research }, 'summary'),
      turn: reply.turn && compactObject({ ...reply.turn }, 'summary'),
      turns: reply.research && Object.freeze(reply.research.options.map(option => compactObject({
        ...option,
        ...(reply.research.after ? {} : {
          researchNextTurn: (options = {}) =>
            call({op: 'research_turns', request: turnResearchRequest(options, 0), after: option.reference}),
          researchBeforeEndTurn: (options = {}) =>
            call({op: 'research_turns', request: turnResearchRequest(options, 0),
              after: option.reference, start: 'before_end_turn'}),
        }),
        inspect: () => call({ op: 'inspect_turn', reference: option.reference }),
        choose: () => call({ op: 'choose_turn', reference: option.reference }),
      }, 'label'))),
      verifiedActions: reply.verified_actions, replayStop: reply.replay_stop,
      proposal: reply.proposal, elapsedMs: reply.elapsed_micros / 1000, roundTripMs: reply.roundTripMs,
      choices: reply.choices && Object.freeze(reply.choices.actions.map(choice => compactObject({
        label: reply.choice_labels?.[choice.handle] ?? 'Inspect choice details',
        reference: choice.handle, action: Object.freeze(choice.action),
        // Convenience data only; choose still submits the exact returned
        // reference. Equal names never merge two source-bound instances.
        card: choiceCard(choice.action),
        cell: choice.action.choice?.kind === 'match_card' ? choice.action.choice.id : undefined,
        choose: () => call({ op: 'choose', selector: { kind: 'reference', reference: choice.handle } }),
      }, 'label'))),
      inspect: (section = 'position') => {
        if (!['position', 'map', 'potions', 'shop', 'removal', 'board'].includes(section)) throw new Error('inspect position, map, potions, shop, removal or board');
        return call({ op: 'inspect', section });
      },
      choose: selection => {
        if (!selection || Object.keys(selection).some(key => !['reward', 'card', 'binding', 'reference', 'endTurn', 'cell'].includes(key))) {
          throw new Error('choose an explicit reward, card, cell, exact reference, or endTurn');
        }
        const alternatives = ['reward', 'card', 'reference', 'endTurn', 'cell'].filter(key => Object.hasOwn(selection, key));
        if (alternatives.length !== 1 || (Object.hasOwn(selection, 'binding') && !Object.hasOwn(selection, 'card'))) {
          throw new Error('choose exactly one selector');
        }
        let selector;
        if (Object.hasOwn(selection, 'reward')) {
          validateRewards([selection.reward]);
          selector = { kind: 'reward', reward: selection.reward };
        } else if (Object.hasOwn(selection, 'card')) {
          if (typeof selection.card !== 'string' || (selection.binding !== undefined && typeof selection.binding !== 'string')) {
            throw new Error('card and optional binding must be strings');
          }
          selector = { kind: 'card', card_id: selection.card, binding: selection.binding ?? null };
        } else if (Object.hasOwn(selection, 'cell')) {
          selector = { kind: 'match_cell', cell: matchCell(selection.cell) };
        } else if (Object.hasOwn(selection, 'reference')) {
          if (typeof selection.reference !== 'string') throw new Error('reference must be a string');
          selector = { kind: 'reference', reference: selection.reference };
        }
        else {
          if (selection.endTurn !== true) throw new Error('endTurn must be true');
          selector = { kind: 'end_turn' };
        }
        return call({ op: 'choose', selector });
      },
      flip: cells => {
        const selected = Array.isArray(cells) ? cells : [cells];
        if (!selected.length || selected.length > 10) throw new Error('flip one to ten explicit cells');
        return call({ op: 'flip_match_cells', cells: selected.map(matchCell) });
      },
      previewRewards: steps => {
        validateRewards(steps);
        return call({ op: 'preview_rewards', steps });
      },
      researchTurns: (options = {}) => call({ op: 'research_turns', request: turnResearchRequest(options), after: null }),
      undo: () => call({ op: 'undo' }),
      mark: name => {
        if (typeof name !== 'string') throw new Error('bookmark name must be a string');
        return call({ op: 'mark', name });
      },
      switchTo: node => call({ op: 'switch', reference: nodeReference(node) }),
      compare: node => call({ op: 'compare', reference: nodeReference(node) }),
      certify: () => call({ op: 'certify' }),
      adopt: proposal => {
        if (typeof proposal !== 'string' || !proposal) throw new Error('use a saved proposal');
        return call({ op: 'adopt', proposal });
      },
      show: () => {
        if (!display) throw new Error('set display when opening the connection');
        display('\n' + reply.summary.trimEnd() + '\n\n');
      },
      diagnostics: () => structuredClone(reply),
      showStatus: () => {
        if (!display) throw new Error('set display when opening the connection');
        display('\n' + (reply.execution
          ? `${reply.execution.state}, ${reply.execution.parity}`
          : `Rust workspace: ${reply.status}`) + '\n\n');
      },
      // Printing a view is safe by default, including serializers that ignore
      // util.inspect.custom. Metadata stays available by property or diagnostics.
      toJSON: () => '\n' + reply.summary.trimEnd() + '\n\n',
      [Symbol.for('nodejs.util.inspect.custom')]: () => '\n' + reply.summary.trimEnd() + '\n\n',
    };
    if (live) {
      for (const key of ['choose', 'flip', 'previewRewards', 'researchTurns', 'undo', 'mark', 'switchTo', 'compare', 'certify']) delete view[key];
    } else delete view.adopt;
    for (const key of Object.keys(view)) {
      if (key !== 'summary') Object.defineProperty(view, key, { enumerable: false });
    }
    return Object.freeze(view);
  }

  async function waitPending(pending, limit) {
    let timer;
    try {
      return await Promise.race([pending.promise, new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error('workspace result is pending; use connection.wait(), do not resubmit')), limit);
      })]);
    } finally { clearTimeout(timer); }
  }

  function request(operation, view) {
    if (dead) return Promise.reject(dead);
    if (inFlight) return Promise.reject(new Error('one operation is still pending; use connection.wait()'));
    const id = nextId;
    const line = JSON.stringify({ id, view, operation }) + '\n';
    nextId++;
    let resolve, reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    // A timed-out caller may await the same promise later without sending again.
    promise.catch(() => {});
    const pending = { id, operation: operation.op, promise, resolve, reject, started: performance.now() };
    inFlight = pending;
    statistics.requestsSent++;
    child.stdin.write(line, error => { if (error) fail(error); });
    return waitPending(pending, timeoutMs);
  }

  return Object.freeze({
    observe: () => request({ op: 'observe' }),
    wait: (limit = timeoutMs) => inFlight ? waitPending(inFlight, limit)
      : lastReply ? Promise.resolve().then(() => {
        const view = makeView(lastReply);
        if (view.status !== 'ok') throw new DecisionOperationError(view);
        return view;
      }) : dead ? Promise.reject(dead)
      : Promise.reject(new Error('no request is pending')),
    close: () => request({ op: 'close' }),
    get workspace() { return workspace; },
    diagnostics: () => diagnosticTail,
    statistics: () => ({ ...statistics }),
  });
}

function compactObject(value, visible) {
  const text = value[visible];
  Object.assign(value, {
    toJSON: () => text,
    [Symbol.for('nodejs.util.inspect.custom')]: () => text,
  });
  for (const key of Object.keys(value)) {
    if (key !== visible) Object.defineProperty(value, key, { enumerable: false });
  }
  return Object.freeze(value);
}

function deepFreeze(value) {
  if (!value || typeof value !== 'object' || Object.isFrozen(value)) return value;
  for (const child of Object.values(value)) deepFreeze(child);
  return Object.freeze(value);
}

function choiceCard(action) {
  if (action.card_id) return action.card_id;
  if (action.kind === 'submit_selection' && action.choices?.length === 1) return action.choices[0].attributes?.card_id;
  if (action.kind === 'select' && action.choice?.kind === 'deck_card') return action.choice.attributes?.card_id;
  if (action.kind === 'select' && ['reward_card', 'discovery_card', 'shop_card'].includes(action.choice?.kind)) return action.choice.id;
  return undefined;
}

// Only wire-shape validation here. Rust owns legal order, uniqueness, reward
// availability, capacity, replay and the stopping boundary.
function validateRewards(steps) {
  if (!Array.isArray(steps) || steps.some(step => !['potion', 'gold', 'cards'].includes(step))) {
    throw new Error('rewards must be an array of potion, gold or cards');
  }
}

function nodeReference(node) {
  if (!node || typeof node.reference !== 'string') throw new Error('use a node object returned by this workspace');
  return node.reference;
}

function matchCell(cell) {
  if (typeof cell !== 'string' || !/^R[1-3]C[1-4]$/.test(cell)) throw new Error('use a fixed board cell such as R1C1');
  return (Number(cell[1]) - 1) * 4 + Number(cell[3]) - 1;
}

function turnResearchRequest(options, defaultLimit) {
  if (!options || typeof options !== 'object' || Array.isArray(options) ||
      Object.keys(options).some(key => !['work', 'engineSteps', 'potionSlots', 'offset', 'limit', 'outcome', 'minimumHp', 'minimumBlock', 'enemyHpAtMost', 'playerPowers'].includes(key))) {
    throw new Error('unknown turn trial option');
  }
  const { work = 256, engineSteps = 65536, potionSlots = [], offset = 0,
    outcome = null, minimumHp = null, minimumBlock = null, enemyHpAtMost = null, playerPowers = [],
    limit = defaultLimit ?? (outcome !== null || minimumHp !== null || minimumBlock !== null || enemyHpAtMost !== null || playerPowers?.length > 0 ? 6 : 0) } = options;
  const validHp = hp => Number.isInteger(hp) && hp >= -2147483648 && hp <= 2147483647;
  if (!Number.isSafeInteger(work) || work < 0 || work > 65536 ||
      !Number.isSafeInteger(engineSteps) || engineSteps < 0 || engineSteps > 1048576 ||
      !Number.isSafeInteger(offset) || offset < 0 ||
      !Number.isSafeInteger(limit) || limit < 0 || limit > 8 ||
      !Array.isArray(potionSlots) || new Set(potionSlots).size !== potionSlots.length ||
      potionSlots.some(slot => !Number.isSafeInteger(slot) || slot < 0 || slot >= 64) ||
      (outcome !== null && !['next_turn', 'victory', 'defeat', 'escape'].includes(outcome)) ||
      (minimumHp !== null && !validHp(minimumHp)) ||
      (minimumBlock !== null && (!validHp(minimumBlock) || minimumBlock < 0)) ||
      !Array.isArray(playerPowers) || playerPowers.length > 8 ||
      playerPowers.some(filter => !filter || typeof filter !== 'object' || Array.isArray(filter) ||
        Object.keys(filter).some(key => !['power', 'present', 'minimumAmount'].includes(key)) ||
        typeof filter.power !== 'string' || !filter.power.trim() ||
        (filter.present !== undefined && typeof filter.present !== 'boolean') ||
        (filter.minimumAmount !== undefined && (!validHp(filter.minimumAmount) || filter.present === false))) ||
      (enemyHpAtMost !== null && (typeof enemyHpAtMost !== 'object' || Array.isArray(enemyHpAtMost) ||
        Object.keys(enemyHpAtMost).some(key => !['slot', 'hp'].includes(key)) ||
        !Number.isInteger(enemyHpAtMost.slot) || enemyHpAtMost.slot < 0 || enemyHpAtMost.slot > 255 ||
        !validHp(enemyHpAtMost.hp)))) {
    throw new Error('invalid turn trial budget, page or outcome/resource filter');
  }
  return { work, engine_steps: engineSteps,
    potion_slots: potionSlots, offset, limit, outcome, minimum_hp: minimumHp, minimum_block: minimumBlock,
    enemy_hp_at_most: enemyHpAtMost,
    player_powers: playerPowers.map(({power, present = true, minimumAmount = null}) =>
      ({power, present, minimum_amount: minimumAmount})) };
}
