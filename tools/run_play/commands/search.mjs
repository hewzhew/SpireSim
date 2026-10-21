// Explicit search; inspection and adoption use the saved Rust report.
import { writeText } from '../render.mjs';
const EFFORTS = ['whole_combat', 'fast_witness', 'standard', 'expanded'];

export function planId(plans, index) {
  const id = plans.candidates[index]?.planId;
  if (typeof id !== 'string') throw new Error(`candidate ${index} is not in the saved report`);
  return id;
}
function index(value, what) {
  if (!/^\d+$/.test(value ?? '') || !Number.isSafeInteger(Number(value))) {
    throw new Error(`invalid ${what}: ${value}`);
  }
  return Number(value);
}

async function requireCombat(ctx, command) {
  const reading = await ctx.game.decision({ details: ['brief'] });
  if (reading.control !== 'combat') {
    throw new Error(`${command} is a combat-only command; the current page is not a combat.\n${await ctx.commandHint(reading)}`);
  }
}
export async function researchOrCommitOrReview(ctx, command, parameters) {
  const { game, printCurrentReading } = ctx;
  if (command === 'research') {
    // A numeric first argument is the whole-combat time budget. Keep this
    // in the existing parser so CLI and resident calls share the same limits.
    if (/^\d+$/.test(parameters[0] ?? '')) parameters = ['whole_combat', ...parameters];
    const effort = parameters[0] ?? 'whole_combat';
    if (!EFFORTS.includes(effort) || ![0, 1, 2, 4].includes(parameters.length)
        || (parameters.length === 4 && (effort !== 'whole_combat' || parameters[2] !== 'work'))) {
      throw new Error('Use research whole_combat <milliseconds 1..20000> [work <transitions 1..1000000>], or research <effort> [milliseconds]. Held potions are already included; research has no potions argument.');
    }
    const maxSearchMs = parameters[1] === undefined
      ? { whole_combat: 2000, fast_witness: 2000, standard: 5000, expanded: 20000 }[effort]
      : index(parameters[1], 'search milliseconds');
    if (maxSearchMs < 1 || maxSearchMs > 20000) throw new Error('search milliseconds must be in 1..20000');
    const maxGenerationWork = parameters.length === 4 ? index(parameters[3], 'search work') : undefined;
    if (maxGenerationWork !== undefined && (maxGenerationWork < 1 || maxGenerationWork > 1000000)) {
      throw new Error('search work must be in 1..1000000');
    }
    await requireCombat(ctx, command);
    const plans = await game.research({ effort, maxSearchMs,
      ...(maxGenerationWork === undefined ? {} : { maxGenerationWork }) });
    (ctx.write ?? writeText)(`${plans.view.trimEnd()}\n`);
    return;
  }
  if (command === 'plans') {
    if (parameters.length > 1 || (parameters.length && parameters[0] !== 'full')) throw new Error('plans [full]');
    await requireCombat(ctx, command);
    const plans = await game.plans(parameters.length ? { diagnostics: true } : undefined);
    (ctx.write ?? writeText)(`${plans.view.trimEnd()}\n`);
    return;
  }
  const selected = parameters[0];
  const selectedId = /^c(0|[1-9]\d*)@[a-f0-9]{12}$/.exec(selected ?? '');
  if (!selectedId) throw new Error(`${command} requires the complete plan ID printed by research or plans, including its @ suffix. Copy the displayed ID; bare candidate indexes do not identify a report.`);
  const chosen = index(selectedId[1], 'candidate index');
  if (parameters.length > 2) throw new Error(`${command} received too many arguments`);
  const turn = command === 'review' && parameters[1] !== undefined
    ? index(parameters[1], 'turn') : undefined;
  if (turn === 0) throw new Error('turn is one-based');
  if (command === 'commit' && parameters[1] !== undefined && !EFFORTS.includes(parameters[1])) {
    throw new Error('commit <complete plan ID> [expected effort]');
  }
  await requireCombat(ctx, command);
  const plans = await game.plans();
  if (selected !== planId(plans, chosen)) {
    throw new Error('This plan ID belongs to a replaced search report. Use plans and choose a complete ID from the current report; no action was applied.');
  }
  if (command === 'commit' && parameters[1] && parameters[1] !== plans.effort) {
    throw new Error(`saved report used ${plans.effort}, not ${parameters[1]}; no new search was run`);
  }
  const candidate = plans.candidates[chosen];
  if (!candidate) throw new Error(`candidate ${chosen} is not in the saved report`);
  const reading = command === 'commit'
    ? await candidate.commit()
    : await candidate.review({ ...(turn === undefined ? { compact: true } : { turn }) });
  if (command === 'commit') ctx.recordAction?.(`commit ${selected} (${plans.effort})`);
  await (command === 'commit' ? (ctx.showSuccessor ?? printCurrentReading) : printCurrentReading)(reading);
}
