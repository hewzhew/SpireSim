// Explicit search; inspection and adoption use the saved Rust report.
import { writeText, presentationFor } from '../render.mjs';
const EFFORTS = ['whole_combat', 'fast_witness', 'standard', 'expanded'];

function index(value, field, presentation) {
  if (!/^\d+$/.test(value ?? '') || !Number.isSafeInteger(Number(value))) {
    throw new Error(presentation.text('workbench:command.invalidInteger', {
      field: presentation.text(`workbench:command.field.${field}`), value: String(value),
    }));
  }
  return Number(value);
}

async function requireCombat(ctx, command) {
  const reading = await ctx.game.decision({ details: ['brief'] });
  if (reading.control !== 'combat') {
    throw new Error((await presentationFor(ctx)).text('workbench:command.combatOnly', {
      command, hint: await ctx.commandHint(reading),
    }));
  }
}
export async function researchOrCommitOrReview(ctx, command, parameters) {
  const { game, printCurrentReading } = ctx;
  const presentation = await presentationFor(ctx);
  const text = (key, params = {}) => presentation.text(`workbench:command.${key}`, params);
  if (command === 'research') {
    // A numeric first argument is the whole-combat time budget. Keep this
    // in the existing parser so CLI and resident calls share the same limits.
    if (/^\d+$/.test(parameters[0] ?? '')) parameters = ['whole_combat', ...parameters];
    const effort = parameters[0] ?? 'whole_combat';
    if (!EFFORTS.includes(effort) || ![0, 1, 2, 4].includes(parameters.length)
        || (parameters.length === 4 && (effort !== 'whole_combat' || parameters[2] !== 'work'))) {
      throw new Error(text('researchUsage'));
    }
    const maxSearchMs = parameters[1] === undefined
      ? { whole_combat: 2000, fast_witness: 2000, standard: 5000, expanded: 20000 }[effort]
      : index(parameters[1], 'time', presentation);
    if (maxSearchMs < 1 || maxSearchMs > 20000) throw new Error(text('timeRange'));
    const maxGenerationWork = parameters.length === 4 ? index(parameters[3], 'work', presentation) : undefined;
    if (maxGenerationWork !== undefined && (maxGenerationWork < 1 || maxGenerationWork > 1000000)) {
      throw new Error(text('workRange'));
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
  if (!selectedId) throw new Error(text('planIdRequired', { command }));
  const chosen = index(selectedId[1], 'candidate', presentation);
  if (parameters.length > 2) throw new Error(text('tooManyArguments', { command }));
  const turn = command === 'review' && parameters[1] !== undefined
    ? index(parameters[1], 'turn', presentation) : undefined;
  if (turn === 0) throw new Error(text('turnOneBased'));
  if (command === 'commit' && parameters[1] !== undefined && !EFFORTS.includes(parameters[1])) {
    throw new Error(text('commitUsage'));
  }
  await requireCombat(ctx, command);
  const plans = await game.plans();
  const candidate = plans.candidates[chosen];
  if (typeof candidate?.planId !== 'string') throw new Error(text('candidateAbsent', { index: chosen }));
  if (selected !== candidate.planId) {
    throw new Error(text('reportReplaced'));
  }
  if (command === 'commit' && parameters[1] && parameters[1] !== plans.effort) {
    throw new Error(text('effortMismatch', { actual: plans.effort, expected: parameters[1] }));
  }
  const reading = command === 'commit'
    ? await candidate.commit()
    : await candidate.review({ ...(turn === undefined ? { compact: true } : { turn }) });
  if (command === 'commit') ctx.recordAction?.(`commit ${selected} (${plans.effort})`);
  await (command === 'commit' ? (ctx.showSuccessor ?? printCurrentReading) : printCurrentReading)(reading);
}
