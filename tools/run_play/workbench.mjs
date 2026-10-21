// Shared command owner for the CLI and resident integrations. Rust owns all
// game state and rules; this module routes existing commands and their output.
import { mkdir, writeFile, appendFile } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { randomUUID } from 'node:crypto';
import { openRunPlay } from './client.mjs';
import { commandGroups, commandTokens, freshRecordDirectory } from './session.mjs';
import { printReading, writeText } from './render.mjs';
import { readingOptionCommands, expandedCombatCommands, distinctCombatCommands,
  assertCurrentDecision, resolveOptionName, decisionDocumentFromObservation, currentCommandHint } from './contract.mjs';
import { actOrPreview, board, publicPiles } from './commands/combat.mjs';
import { go, mapView, loot, info, cancel, use } from './commands/travel.mjs';
import { choose } from './commands/choose.mjs';
import { researchOrCommitOrReview, scopedPlanView } from './commands/search.mjs';
import { engineOutput, runEngine, showJumpPoint } from './commands/navigation.mjs';

export function createWorkbenchContext(options) {
  const ctx = { write: writeText, join, runEngine, ...options };
  ctx.usage ??= () => { throw new Error('act and preview each require at least one semantic action'); };
  ctx.decisionDocument = async reading => decisionDocumentFromObservation(
    reading?.raw?.observation ?? await ctx.game.observe());
  ctx.commandHint = async (reading, options) => currentCommandHint(reading, await ctx.decisionDocument(reading), options);
  ctx.printCurrentReading = async (reading, { expanded = false } = {}) => {
    const doc = reading.control === 'combat' || reading.options?.length
      ? await ctx.decisionDocument(reading) : null;
    if (doc) assertCurrentDecision(reading, doc);
    if (reading.view.includes('\nSaved combat wins')) {
      const plans = await ctx.game.plans();
      reading = { ...reading, raw: reading.raw, view: scopedPlanView(reading.view, plans) };
    }
    printReading(reading, {
      write: ctx.write, expanded, selectionCommands: true,
      commandHeading: ctx.commandPrefix === '',
      optionCommands: reading.options?.length ? readingOptionCommands(reading, doc) : null,
      combatCommands: doc ? (expanded ? expandedCombatCommands(reading, doc)
        : distinctCombatCommands(reading, doc)) : [],
    });
  };
  ctx.resolveOptionByName = async (wanted, reading) => resolveOptionName(
    wanted, reading, await ctx.decisionDocument(reading));
  return ctx;
}

export async function dispatchWorkbench(ctx, command, parameters) {
  if (command === 'deck') {
    throw new Error('deck is not a command. Use inventory for the permanent deck and resources, or inventory effects for their rules.');
  }
  if (/^play:/.test(command ?? '')) {
    throw new Error(`Use act ${[command, ...parameters].join(' ')} to play, or preview with the same actions to rehearse. Start the action line with act or preview; commas can separate its actions.`);
  }
  if (command === 'act/preview') {
    throw new Error('Use preview <actions> for a private rehearsal, or act <actions> to adopt it. act/preview is help notation, not a command. No action was submitted.');
  }
  if (['research', 'plans', 'commit', 'review'].includes(command)) return researchOrCommitOrReview(ctx, command, parameters);
  if (command === 'act' || command === 'preview') return actOrPreview(ctx, command, parameters);
  if (command === 'read') {
    if (parameters.length > 1 || (parameters.length && !['brief', 'full'].includes(parameters[0]))) {
      throw new Error('read [brief|full] reads the current decision');
    }
    return ctx.printCurrentReading(await ctx.game.decision(
      { details: parameters[0] === 'full' ? [] : ['brief'] }));
  }
  if (command === 'inventory') {
    if (parameters.length > 1 || (parameters.length && parameters[0] !== 'effects')) {
      throw new Error('inventory [effects] reads the permanent master deck and relics');
    }
    return ctx.write(await engineOutput(ctx, ['inventory', ...(parameters.length ? ['--effects'] : [])]));
  }
  if (command === 'deck-lab') {
    const invocation = 'deck-lab ENCOUNTER [scenarios N] [work N] [ms N] [potions N] [full-hp] [details] [after <preparation actions>]';
    const usage = 'deck-lab list | ' + invocation;
    if (parameters.length === 1 && parameters[0] === 'list') {
      const catalog = await engineOutput(ctx, ['deck-lab', '--list'], { signal: ctx.signal });
      return ctx.write(`${catalog.trimEnd()}\n\nRun: ${invocation}\nAppend after followed by a displayed preparation command to test rest, an upgrade, removal, purchase or card choice privately. The game stays unchanged; no need to apply or undo the preparation.\nUse the same encounter, scenarios, work, ms and potion limit when comparing preparations.\n`);
    }
    if (!parameters.length || parameters[0] === 'list' || parameters[0].startsWith('-')) throw new Error(usage);
    const args = ['deck-lab', '--encounter', parameters[0]];
    const seen = new Set();
    const flags = { scenarios: '--scenarios', work: '--work', ms: '--ms', potions: '--max-potion-uses' };
    for (let i = 1; i < parameters.length; i++) {
      const key = parameters[i];
      if (seen.has(key)) throw new Error(usage);
      seen.add(key);
      if (key === 'after') {
        const line = parameters.slice(i + 1);
        if (!line.length || line.some(action => !action.trim() || action.startsWith('--') || action === 'after')) throw new Error(usage);
        args.push('--after', line.join('/'));
        break;
      }
      if (key === 'full-hp') { args.push('--full-hp'); continue; }
      if (key === 'details') { args.push('--details'); continue; }
      const value = parameters[++i];
      if (!Object.hasOwn(flags, key) || !/^\d+$/.test(value ?? '')
          || !Number.isSafeInteger(Number(value)) || (key !== 'potions' && Number(value) === 0)) {
        throw new Error(usage);
      }
      args.push(flags[key], value);
    }
    const output = join(ctx.recordDirectory, `deck-lab-${randomUUID()}.json`);
    args.push('--output', output);
    return ctx.write(await engineOutput(ctx, args, { signal: ctx.signal }));
  }
  if (command === 'board') return board(ctx, parameters);
  if (command === 'go') return go(ctx, parameters);
  if (command === 'map') return mapView(ctx, parameters);
  if (command === 'loot') return loot(ctx, parameters);
  if (command === 'info') return info(ctx, parameters);
  if (command === 'piles') {
    if (!parameters.length) return publicPiles(ctx);
    if (parameters.length !== 1 || parameters[0] !== 'sl') throw new Error('Use piles for public unordered draw, discard and exhaust contents. Exact inspection uses piles sl in assisted play.');
    return ctx.write((await ctx.game.slPiles()).view);
  }
  if (command === 'jump') return showJumpPoint(ctx);
  if (command === 'options') return ctx.printCurrentReading(await ctx.game.decision({ details: ['brief', 'options'] }), { expanded: true });
  if (command === 'cancel') return cancel(ctx, parameters);
  if (command === 'use') return use(ctx, parameters);
  return choose(ctx, command, parameters);
}

// Coalesce mutation successors; explicit current-page queries replace the
// last automatic page, while requested query results remain. Every action still
// obtains and binds its own fresh reading. Applied actions are only displayed
// if a later command fails; the transport already records successful requests.
export async function executeWorkbenchCommands(ctx, groups, dispatch = dispatchWorkbench) {
  const write = ctx.write, previousSuccessor = ctx.showSuccessor, previousRecord = ctx.recordAction;
  const chunks = [], applied = [];
  let successorIndex, successorIsMap = false;
  ctx.write = text => chunks.push(text);
  ctx.recordAction = action => applied.push(action);
  ctx.showSuccessor = async reading => {
    // Newly revealed random results are facts about an executed action, not
    // another automatic page. Keep them when later actions/queries replace it.
    if (reading.raw?.revealed_changes) ctx.write(reading.raw.revealed_changes);
    const start = chunks.length;
    await ctx.printCurrentReading(reading);
    if (successorIndex !== undefined) chunks[successorIndex] = '';
    const view = chunks.splice(start).join('');
    successorIndex = chunks.push(view) - 1;
    successorIsMap = (reading.raw?.reading?.options ?? []).some(option =>
      option.candidate?.action?.kind === 'choose_route_node');
  };
  try {
    for (const [command, parameters] of groups) {
      const beforeQuery = successorIndex;
      const result = await dispatch(ctx, command, parameters);
      const replacesPage = ['read', 'options', 'board'].includes(command)
        || (command === 'map' && parameters[0] !== 'set' && successorIsMap);
      if (beforeQuery !== undefined && replacesPage) {
        chunks[beforeQuery] = '';
        successorIndex = undefined;
      }
      if (result?.needsChoice) break;
    }
    return chunks.join('');
  } catch (error) {
    const output = chunks.join('');
    const prefix = applied.length ? `Applied before error:\n${applied.map(action => `  ${action}`).join('\n')}\n` : '';
    if (prefix || output) {
      const wrapped = new Error(`${error.message}\n${prefix}${output ? `Earlier results:\n${output}` : ''}`.trimEnd(), { cause: error });
      Object.assign(wrapped, error);
      throw wrapped;
    }
    throw error;
  } finally {
    ctx.write = write;
    ctx.showSuccessor = previousSuccessor;
    ctx.recordAction = previousRecord;
  }
}

export function navigationArguments(command, parameters) {
  if (command === 'undo') {
    if (parameters.length) throw new Error('undo takes no arguments');
    return [];
  }
  if (command === 'jump') return parameters;
  if (command === 'mark') {
    if (parameters.length === 0) return [];
    if (parameters[0] === 'drop' && parameters.length === 2 && parameters[1]) return ['--drop', parameters[1]];
    if (parameters.length === 1 && parameters[0] !== 'drop' && parameters[0]) return ['--name', parameters[0]];
    throw new Error('mark [name | drop name]; quote names containing spaces');
  }
  if (parameters[0] === 'list' && parameters.length === 1) return ['--list'];
  if (parameters[0] === 'floor' && parameters.length === 2 && /^\d+$/.test(parameters[1])
      && Number.isSafeInteger(Number(parameters[1])) && Number(parameters[1]) <= 2147483647) return ['--floor', parameters[1]];
  if (parameters[0] === 'mark' && parameters.length === 2 && parameters[1]) return ['--mark', parameters[1]];
  const steps = parameters[0] ?? '1';
  if (parameters.length > 1 || !/^\d+$/.test(steps) || !Number.isSafeInteger(Number(steps)) || Number(steps) < 1) {
    throw new Error('back [positive steps | floor number | mark name | list]; use back list for retained targets');
  }
  return ['--steps', steps];
}

// A fixed session, with serial commands and an owned text sink. Every reconnect
// gets a fresh transport transcript; it never changes the shared CLI focus.
export async function openWorkbench({ repository, session, binary = join(repository, 'target/dev-opt/run_play.exe'),
  recordDirectory, commandPrefix = './p', manual = false, ...options }, { connect = openRunPlay, run = runEngine } = {}) {
  if (typeof manual !== 'boolean') throw new Error('manual must be a boolean');
  const sessionLabel = session.split(/[\\/]/).at(-1);
  const records = recordDirectory ?? freshRecordDirectory(repository, sessionLabel, 'resident');
  await mkdir(dirname(records), { recursive: true });
  await mkdir(records); // A resident's recording scope is new, not an old session log.
  await writeFile(join(records, 'workbench.json'), JSON.stringify({ repository, session, binary, manual,
    openedAt: new Date().toISOString() }, null, 2) + '\n', { flag: 'wx' });
  let connectionNumber = 0, game, closed = false, closing = false, tail = Promise.resolve(), chunks;
  const formatOutput = text => commandPrefix === './p' ? text
    : text.replaceAll('./p ', commandPrefix ? `${commandPrefix} ` : '');
  const ctx = createWorkbenchContext({ repository, session, binary, sessionLabel, recordDirectory: records, runEngine: run, commandPrefix,
    write: text => chunks.push(formatOutput(text)),
  });
  async function reconnect() {
    game = await connect({ ...options, repository, session, binary, manual,
      recordDirectory: join(records, `connection-${connectionNumber++}`) });
    ctx.game = game;
  }
  await reconnect();
  function serial(task) {
    const result = tail.then(task);
    tail = result.catch(() => {});
    return result;
  }
  async function recorded(operation, input, task) {
    const id = randomUUID(), started = performance.now();
    const log = event => appendFile(join(records, 'commands.jsonl'), JSON.stringify({ id,
      at: new Date().toISOString(), ...event }) + '\n');
    await log({ phase: 'started', operation, input });
    let result;
    try { result = await task(); }
    catch (error) {
      await log({ phase: 'failed', elapsedMs: performance.now() - started, error: error.message,
        branchChanged: error.branchChanged ?? false });
      throw error;
    }
    await log({ phase: 'completed', elapsedMs: performance.now() - started, result });
    return result;
  }
  function requireAssistance() {
    if (manual) throw new Error('Public manual play disables search, private previews, deck-lab and exact hidden piles. Use act for actual actions and back for retained returns.');
  }
  async function navigate(operation) {
    if (game) await game.close();
    game = null; ctx.game = null;
    let result, navigationError;
    try { result = await operation(); }
    catch (error) { navigationError = error; }
    try { await reconnect(); }
    catch (error) {
      if (navigationError) throw new Error(`${navigationError.message}\nReconnecting also failed: ${error.message}`, { cause: error });
      throw error;
    }
    if (navigationError) throw navigationError;
    return result;
  }
  return Object.freeze({
    recordDirectory: records,
    manual,
    observe({ signal } = {}) {
      if (closing || closed) return Promise.reject(new Error('workbench is closed'));
      return serial(async () => {
        signal?.throwIfAborted();
        if (!game) await reconnect();
        return game.observe();
      });
    },
    preview(actions, { signal } = {}) {
      if (closing || closed) return Promise.reject(new Error('workbench is closed'));
      return serial(() => recorded('preview', actions, async () => {
        signal?.throwIfAborted();
        requireAssistance();
        if (!game) await reconnect();
        return game.preview(actions);
      }));
    },
    execute(line, { signal } = {}) {
      if (closing || closed) return Promise.reject(new Error('workbench is closed'));
      return serial(() => recorded('execute', line, async () => {
        signal?.throwIfAborted();
        const tokens = Array.isArray(line) ? line : commandTokens(line);
        const groups = commandGroups(tokens[0], tokens.slice(1));
        // Reject session/process controls before any prefix command executes.
        for (const [command, args] of groups) {
          if (['preview', 'research', 'plans', 'review', 'commit', 'deck-lab'].includes(command)
              || (command === 'piles' && args[0] === 'sl')) requireAssistance();
          if (['new', 'use', 'refresh', 'exe', 'scout', 'pick'].includes(command)) {
            throw new Error(`${command} is not available in a bound game session`);
          }
          if (command === 'map' && args[0] === 'set') throw new Error('Use map flip or map normal; shared preferences are not changed by this player');
          if (['back', 'undo', 'jump', 'mark'].includes(command)) navigationArguments(command, args);
        }
        chunks = [];
        ctx.signal = signal;
        let branchChanged = false;
        try {
          const output = await executeWorkbenchCommands(ctx, groups, async (ctx, command, args) => {
            signal?.throwIfAborted();
            if (!game) await reconnect();
            if ((command === 'back' && args[0] === 'list') || (command === 'mark' && !args.length)) {
              ctx.write(await engineOutput(ctx, [command, ...navigationArguments(command, args)]));
            } else if (command === 'back' || command === 'undo' || command === 'mark' || (command === 'jump' && args.length)) {
              await navigate(async () => {
                await engineOutput(ctx, [command, ...navigationArguments(command, args)]);
                if (command !== 'mark') branchChanged = true;
                ctx.recordAction(`${command}${args.length ? ` ${args.join(' ')}` : ''}`);
              });
              await ctx.showSuccessor(await game.decision({ details: ['brief'] }));
            } else return dispatchWorkbench(ctx, command, args);
          });
          ctx.write(output);
          return { text: chunks.join(''), ...(branchChanged ? { branchChanged: true } : {}) };
        } catch (caught) {
          // AbortSignal's default reason is a DOMException with a read-only
          // message. Format our own error while retaining transport metadata.
          const error = Object.assign(new Error(formatOutput(caught.message), { cause: caught }), caught);
          error.name = caught.name;
          if (branchChanged) error.branchChanged = true;
          let recovery = '';
          if (error.transportFailed && game) {
            const failed = game;
            game = null; ctx.game = null;
            try { await failed.close(); }
            catch (closeError) { recovery = `\nClosing the failed connection also failed: ${closeError.message}`; }
            recovery += '\nThe failed connection was released. Use read to reconnect and inspect the actual saved state before retrying; no game command was retried.';
          }
          const prefix = chunks.join('');
          if (prefix || recovery) {
            const wrapped = new Error(`${prefix ? `${prefix}\nError: ` : ''}${error.message}${recovery}\nRead the current state before continuing.`, { cause: error });
            Object.assign(wrapped, error);
            throw wrapped;
          }
          throw error;
        } finally { chunks = null; ctx.signal = undefined; }
      }));
    },
    returnPoints() {
      if (closing || closed) return Promise.reject(new Error('workbench is closed'));
      return serial(async () => JSON.parse(await engineOutput(ctx, ['back', '--list', '--json'])));
    },
    restorePoint(request) {
      if (closing || closed) return Promise.reject(new Error('workbench is closed'));
      return serial(() => recorded('restorePoint', request, async () => {
        const operation = `return-${randomUUID()}`;
        const requestFile = join(records, `${operation}-request.json`);
        await writeFile(requestFile, JSON.stringify(request), { flag: 'wx' });
        let receipt;
        try {
          await navigate(async () => {
            receipt = JSON.parse(await engineOutput(ctx, ['back', '--request', requestFile]));
            await writeFile(join(records, `${operation}-receipt.json`), JSON.stringify(receipt), { flag: 'wx' });
          });
          const points = JSON.parse(await engineOutput(ctx, ['back', '--list', '--json']));
          return { receipt, points };
        } catch (error) {
          if (receipt) Object.assign(error, { receipt, branchChanged: true });
          throw error;
        }
      }));
    },
    close() {
      closing = true;
      return serial(async () => {
        if (closed) return;
        try { if (game) await game.close(); }
        finally { closed = true; }
      });
    },
  });
}
