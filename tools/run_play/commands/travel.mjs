// Room and map commands: go (route), map (routes/grid and
// preferences), loot (routine reward claiming), info (mechanics),
// cancel (back into the room), use (switch the default session).
import { writeText, presentationFor } from '../render.mjs';
import { resolveSession } from '../session.mjs';
import { isRewardReading, assertCurrentDecision, decisionObservation, readingOptionCommands } from '../contract.mjs';
import { engineOutput } from './navigation.mjs';

export async function go(ctx, parameters) {
  const { game, printCurrentReading } = ctx;
  const presentation = await presentationFor(ctx);
  const text = (key, params = {}) => presentation.text(`workbench:command.${key}`, params);
  // Routes are picked by destination coordinate, never by display
  // index: indices shift with potion-discard entries and stale listings,
  // which caused wrong-fork picks. Room type does not change the command.
  let target = parameters[0] ?? '';
  if (parameters.length > 1 || (parameters.length && !/^\d+\s*,\s*\d+$/.test(target))) {
    throw new Error(text('goUsage'));
  }
  let reading = await game.decision({ details: ['brief'] });
  let crossedContinuation = false;
  // Finish empty rewards before resolving either an explicit or a sole route.
  // Forced event continuations still require an engine-supplied empty effect
  // list; labels such as Leave/Attack/Continue do not prove a choice is free.
  for (let guard = 0; guard < 8; guard += 1) {
    const options = reading.raw?.reading?.options;
    const rewardProceed = options?.findIndex(option => option.candidate?.action?.kind === 'proceed'
      && option.candidate.action.site === 'reward') ?? -1;
    if (rewardProceed >= 0) {
      const doc = await ctx.decisionDocument(reading);
      assertCurrentDecision(reading, doc);
      const context = decisionObservation(doc)?.context;
      // Empty Vec fields are omitted by the observation producer. A reward
      // context is required: missing context must not imply empty rewards.
      if (context?.kind !== 'reward' || (context.pending_rewards ?? []).length) break;
      const continuation = reading.options[rewardProceed];
      reading = await continuation.choose();
      crossedContinuation = true;
      ctx.recordAction?.(continuation.label);
      await (ctx.showSuccessor ?? printCurrentReading)(reading);
      continue;
    }
    if (options?.length !== 1 || reading.options?.length !== 1) break;
    const action = options[0].candidate?.action;
    const semantics = action?.public_semantics?.structured_semantics;
    if (action?.kind !== 'choose_event_option' || action.public_semantics.starts_combat
        || !Array.isArray(semantics?.effects) || semantics.effects.length
        || !['complete', 'advance_screen'].includes(semantics.transition?.kind)) break;
    const continuation = reading.options[0];
    reading = await continuation.choose();
    crossedContinuation = true;
    ctx.recordAction?.(continuation.label);
    await (ctx.showSuccessor ?? printCurrentReading)(reading);
  }
  if (!parameters.length) {
    const commands = readingOptionCommands(reading, await ctx.decisionDocument(reading));
    const routes = commands.filter(command => /^go \d+,\d+$/.test(command));
    if (routes.length > 1) {
      await (ctx.showSuccessor ?? printCurrentReading)(reading);
      return { needsChoice: true };
    }
    if (!routes.length) {
      if (crossedContinuation) {
        await (ctx.showSuccessor ?? printCurrentReading)(reading);
        return { needsChoice: true };
      }
      throw new Error(text('goNeedsMap'));
    }
    target = routes[0].split(' ')[1];
  }
  const match = /^(\d+)\s*,\s*(\d+)$/.exec(target);
  const coordinate = `${Number(match[1])},${Number(match[2])}`;
  const wanted = `(${coordinate})`;
  const resolveRoute = async reading => {
    const commands = readingOptionCommands(reading, await ctx.decisionDocument(reading));
    const index = commands.indexOf(`go ${coordinate}`);
    return { option: reading.options?.[index], commands };
  };
  reading ??= await game.decision({ details: ['brief'] });
  const route = await resolveRoute(reading);
  // Prepare departure and travel together: an invalid destination must not
  // leave the current shop or abandon rewards before reporting the error.
  if (route.option === undefined) {
    const shopLeave = reading.raw?.reading?.options?.findIndex(
      option => option.candidate?.action?.kind === 'leave_shop') ?? -1;
    let rewardProceed = -1;
    const doc = await ctx.decisionDocument(reading);
    const commands = readingOptionCommands(reading, doc);
    if (shopLeave === -1 && isRewardReading(reading)) {
      rewardProceed = commands.indexOf('proceed');
    }
    if (shopLeave >= 0 || rewardProceed >= 0) {
      const departure = shopLeave >= 0 ? shopLeave : rewardProceed;
      reading = await game.act([commands[departure], `route:${Number(match[1])}.${Number(match[2])}`]);
      ctx.recordAction?.(`${commands[departure]} / go ${wanted}`);
      await (ctx.showSuccessor ?? printCurrentReading)(reading);
      return;
    }
  }
  const { option } = route;
  if (!option) {
    if (crossedContinuation && !route.commands.some(command => /^go \d+,\d+$/.test(command))) {
      await (ctx.showSuccessor ?? printCurrentReading)(reading);
      return { needsChoice: true };
    }
    throw new Error(text('noRoute',{destination:wanted,options:route.commands.filter(Boolean).join(' | ') || text('none')}));
  }
  reading = await option.choose();
  ctx.recordAction?.(`go ${wanted}`);
  await (ctx.showSuccessor ?? printCurrentReading)(reading);
}

export async function mapView(ctx, parameters) {
  const { repository, join } = ctx;
  const write = ctx.write ?? writeText;
  const mapArguments = ['map', '--locale', ctx.agentLocale ?? 'en'];
  if (!parameters.length) {
    return write(await engineOutput(ctx, mapArguments));
  }
  if (parameters[0] === 'elites' && parameters.length === 1) {
    return write(await engineOutput(ctx, [...mapArguments, 'elites']));
  }
  if (parameters[0] === 'route' && parameters.length === 3 && parameters.slice(1).every(p => /^\d+$/.test(p))) {
    return write(await engineOutput(ctx, [...mapArguments, ...parameters]));
  }
  if (parameters.length && !(
    (parameters.length === 1 && ['full', 'flip', 'normal'].includes(parameters[0]))
    || (parameters.length === 2 && parameters[0] === 'set' && ['flip', 'normal'].includes(parameters[1]))
  )) {
    throw new Error('map [full | flip | normal | set flip | set normal | elites | route X Y]');
  }
  // Rust owns the grid, names, legend and structural row labels as well as
  // grouped routes. Node only supplies the existing direction preference.
  // `map flip` renders small-y-on-top once (forward travel reads
  // downward — nicer for forward planning and for the strong-player
  // habit of planning backward from the boss); `map set flip|normal`
  // stores the full grid's default; grouped routes come from Rust.
  const mapPrefsFile = join(repository, '.oracle-lab', 'run-play-records', 'map-prefs');
  const { readFile: readPrefs, writeFile: writePrefs } = await import('node:fs/promises');
  let defaultFlipped = false;
  try { defaultFlipped = (await readPrefs(mapPrefsFile, 'utf8')).trim() === 'flip'; } catch { /* no preference yet */ }
  let flipped = defaultFlipped;
  if (parameters[0] === 'flip') flipped = !defaultFlipped;
  if (parameters[0] === 'normal') flipped = false;
  if (parameters[0] === 'set') {
    if (parameters[1] !== 'flip' && parameters[1] !== 'normal') {
      throw new Error((await presentationFor(ctx)).text('workbench:command.mapSetUsage'));
    }
    await writePrefs(mapPrefsFile, parameters[1]).catch(() => {});
    write(`map set ${parameters[1]}\n`);
    return;
  }
  write(await engineOutput(ctx, [...mapArguments, 'full', ...(flipped ? ['--flipped'] : [])]));
  // The grid already carries every connection. Do not repeat grouped routes
  // or the room's unrelated decisions after an explicit full-map request.
}

export async function loot(ctx) {
  const { game, printCurrentReading } = ctx;
  // Collect gold and potions that all fit. Competing potion rewards need
  // a player choice; reward order must not choose the last slot's occupant.
  let reading = await game.decision({ details: ['brief'] });
  for (let guard = 0; guard < 8; guard += 1) {
    if (!isRewardReading(reading)) break;
    const doc = await ctx.decisionDocument(reading);
    assertCurrentDecision(reading, doc);
    const actions = reading.raw.reading.options.map(o => o.candidate?.action);
    const context = decisionObservation(doc)?.context;
    if (context?.kind !== 'reward') throw new Error('reward reading lacks typed reward context');
    // Rust omits this Vec when empty; omission is its documented empty value.
    const pending = context.pending_rewards === undefined ? [] : context.pending_rewards;
    if (!Array.isArray(pending)) throw new Error('reward reading lacks typed pending rewards');
    // Use the bound legal action's typed reward, regardless of row order or
    // rendered name. Rust omits potion claims when capacity/rules forbid them.
    const gold = actions.findIndex(a => a?.kind === 'claim_reward'
      && ['gold', 'stolen_gold'].includes(a.reward?.kind));
    const potion = actions.findIndex(a => a?.kind === 'claim_reward' && a.reward?.kind === 'potion');
    const potionRewards = pending.filter(reward => reward.kind === 'potion').length;
    const observation = decisionObservation(doc);
    const slots = observation?.potions;
    // Under Sozu, the legal claim clears the reward without filling a slot.
    const potionGainBlocked = observation?.relics?.some(relic => relic.relic === 'Sozu');
    if (potion >= 0 && !Array.isArray(slots)) throw new Error('reward reading lacks typed potion slots');
    const freeSlots = slots?.filter(slot => !slot.potion).length ?? 0;
    if (gold < 0 && potion >= 0 && !potionGainBlocked && potionRewards > freeSlots) {
      break;
    }
    const routine = gold >= 0 ? gold : potion;
    if (routine >= 0) {
      const option = reading.options[routine];
      reading = await option.choose();
      ctx.recordAction?.(option.label);
      if (ctx.showSuccessor) await ctx.showSuccessor(reading);
      continue;
    }
    // Relics may require a target or compete with the Sapphire key. Leave
    // those choices to the player, before a card screen masks their options.
    // The returned reward reading already shows what remains to be chosen.
    if (pending.some(reward => reward.kind !== 'card_reward')) {
      break;
    }
    const card = actions.findIndex(a => a?.kind === 'open_card_reward');
    if (card >= 0) {
      const opener = reading.options[card];
      reading = await opener.choose();
      ctx.recordAction?.(opener.label);
      if (ctx.showSuccessor) await ctx.showSuccessor(reading);
    }
    break;
  }
  await (ctx.showSuccessor ?? printCurrentReading)(reading);
}

export async function info(ctx, parameters = []) {
  const write = ctx.write ?? writeText;
  if (parameters.length && ctx.game) {
    write((await ctx.game.info(parameters.join(' '), { locale: ctx.agentLocale ?? 'en' })).view);
    return;
  }
  if (parameters.length) {
    write(await engineOutput(ctx, ['info', '--locale', ctx.agentLocale ?? 'en', ...parameters]));
    return;
  }
  const reading = await ctx.game.decision({ details: ['options'] });
  await ctx.printCurrentReading(reading, { expanded: true });
}

export async function cancel(ctx) {
  const { game, printCurrentReading } = ctx;
  // Map overlay back into the room: the reading's cancel option, so
  // looking at the map stays reversible the way the game's own map is.
  let reading = await game.decision({ details: ['options'] });
  const cancelIndex = (reading.raw?.reading?.options ?? [])
    .findIndex(option => ['strategic', 'combat_atomic'].includes(option.candidate?.domain)
      && option.candidate.action?.kind === 'cancel');
  if (cancelIndex === -1) {
    throw new Error((await presentationFor(ctx)).text('workbench:command.noCancel',{options:(reading.options ?? []).map(o=>o.label).join(' | ')}));
  }
  reading = await reading.options[cancelIndex].choose();
  ctx.recordAction?.('cancel');
  await (ctx.showSuccessor ?? printCurrentReading)(reading);
}

export async function use(ctx, parameters) {
  const { stateFile } = ctx;
  // Switch the current session (the default target for every bare
  // command); short names resolve like session arguments do.
  if (parameters.length < 1) throw new Error((await presentationFor(ctx)).text('workbench:command.useUsage'));
  const target = resolveSession(parameters[0]);
  const { writeFile: writeState } = await import('node:fs/promises');
  await writeState(stateFile, target).catch(() => {});
  process.stdout.write(`⇢ use ${target}\n`);
}
