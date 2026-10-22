// A scoped dsh player capability over the shared workbench. No game rules,
// shell evaluation, model API or source-code editing belongs to this plugin.
import { mkdir, appendFile, readFile, writeFile, rename } from 'node:fs/promises';
import { join, isAbsolute } from 'node:path';
import { pathToFileURL } from 'node:url';
import { preparationExpertGuidance } from './strategy.mjs';
import { preparationDeckComparison } from './deck.mjs';
import { preparationCardChoice, renderPreparationCardChoice, adoptPreparationCardChoice,
  cardChoiceInstructions } from './card-choice.mjs';
import { preparationShopBudget, currentShopPlan, bindShopPlan, shopPotionReplacementScope, bindShopPotionPlan,
  shopInstructions, measureShopPotionInputBudget, shopSpendingScope, measureShopSpendingInputBudget } from './shop.mjs';
import z from '@deepseek-ai/schemastery';
import { defineTool } from '@deepseek-ai/dsh-tools';
import { scopeOf } from '@deepseek-ai/dsh-scope';
import { createPresentation } from '@sts-simulator/presentation';
import { gameContextSource, sameGameContextSource, requestBranchReset, requestDecisionReset,
  hasPendingGameContextReset, gameDecisionTask, sameGameDecisionTask,
  contextQueryCommand, decisionContextSource, decisionQueryReferences } from './context.mjs';
import { needsMaintainerReview, proposePlayerCommand, readPlayerReview,
  playerReviewOutstanding, playerPauseText, currentPlayerGuidance, currentPlayerAdoption } from './supervision.mjs';
import { preparationReading, preparationReadingSupply, preparationStageGuide, preparationInstructions, preparationTaskRoot, preparationTaskProgress,
  preparationProgressText, assertPreparationCommand, restoredPreparationDecision, preparationCommandBasis,
  preparationClassificationContext, preparationCandidateGroups, preparationConditionBindings, collectPreparationClassification,
  preparationClassificationReceipt, preparationRecordedTable } from './preparation.mjs';

export const name = 'sts-player';
export const inject = ['tools', 'systemPrompt', 'compaction'];
export const Config = z.object({
  repository: z.string().required(),
  session: z.string().required(),
  binary: z.string().required(false),
  agentLocale: z.union(['en', 'zh-CN']).required(false),
  // A changed-state attachment rebuilds the current decision context.
  resetContextOnAttach: z.boolean().required(false),
  taskScope: z.string().required(false),
  supervised: z.boolean().required(false),
  preparationRoot: z.object({
    source: z.object({ schema: z.string(), session_id: z.string(), head_revision: z.number(), state_revision: z.number() }),
    task: z.object({ location: z.string(), family: z.string(), detail: z.string() }),
    startingGold: z.number().required(false),
  }).default(undefined).required(false),
});

// Keep ordinary observation and personal planning tools when mounted globally.
// The guard also covers scoped/new tools that a visibility filter cannot hide.
const allowed = new Set(['sts', 'sts_feedback', 'read', 'read_image', 'glob', 'grep', 'skill', 'todo_write']);
export function playerGuide(locale = 'en') {
  return createPresentation({ locale }).text('scripted:guide.player');
}
export const PLAYER_GUIDE = playerGuide();

export async function apply(ctx, config) {
  const presentation = createPresentation({ locale: config.agentLocale ?? 'en' });
  const text = (key, parameters) => presentation.text(`scripted:${key}`, parameters);
  const runtimeText = (key, parameters = {}) => presentation.text(`scripted:runtime.${key}`,
    Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));
  const preparation = preparationInstructions(presentation.locale);
  const cardInstructions = cardChoiceInstructions(presentation.locale);
  const shop = shopInstructions(presentation.locale);
  const scope = scopeOf(ctx);
  if (!scope) throw new Error(runtimeText('scopeRequired'));
  if (typeof ctx.compaction.resetDecisionContext !== 'function') {
    throw new Error(runtimeText('compactionRequired'));
  }
  for (const key of ['repository', 'session']) {
    if (!isAbsolute(config[key])) throw new Error(runtimeText('absolutePath', { key: key }));
  }
  if (config.binary && !isAbsolute(config.binary)) throw new Error(runtimeText('absoluteBinary'));
  const taskScope = config.taskScope ?? 'run';
  if (!['run', 'preparation'].includes(taskScope)) throw new Error(runtimeText('invalidScope'));
  if (config.supervised && taskScope !== 'run') throw new Error(runtimeText('coachedScope'));
  const taskTools = taskScope === 'preparation' ? new Set(['sts', 'sts_feedback']) : allowed;
  const { openWorkbench } = await import(pathToFileURL(join(config.repository, 'tools/run_play/workbench.mjs')));
  const { freshRecordDirectory, commandTokens, commandGroups } = await import(pathToFileURL(join(config.repository, 'tools/run_play/session.mjs')));
  const normalizeContextQuery = command => {
    try {
      const tokens = commandTokens(command);
      return contextQueryCommand(commandGroups(tokens[0], tokens.slice(1)));
    } catch { /* Invalid or unrecognized commands are never replayed. */ }
  };
  const { strategicOptionCommand } = await import(pathToFileURL(join(config.repository, 'tools/run_play/contract.mjs')));
  const { normalizeActionTokens } = await import(pathToFileURL(join(config.repository, 'tools/run_play/tokens.mjs')));
  const preparationChoiceGroup = choice => {
    const tokens = commandTokens(choice), groups = commandGroups(tokens[0], tokens.slice(1));
    if (groups.length !== 1 || ['classify', 'choose', 'read', 'options'].includes(groups[0][0])) {
      throw new Error(runtimeText('oneChoiceCommand'));
    }
    return groups[0][0] === 'act' ? ['act', normalizeActionTokens(groups[0][1])] : groups[0];
  };
  const records = freshRecordDirectory(config.repository, config.session.split(/[\\/]/).at(-1), 'dsh');
  let owner, opening, player, decisionTask, disposed = false, attempt = 0;
  let preparationRoot = config.preparationRoot, taskProgress = '', adoptedDecision, classification, classificationDraft, preparationFailures = 0;
  let preparationReviewCandidate, preparationContext;
  let shopBudget;
  let cardChoiceFrame;
  let rulesKey, rulesText = '';
  const shopPlanFile = join(config.session, 'dsh-shop-plan.json');
  async function prepareReading(raw, game, signal) {
    const reading = { ...preparationReading(raw, config.rewardProgress) };
    if (reading.preparation_reading) {
      const supply = preparationReadingSupply(reading, strategicOptionCommand, presentation.locale);
      reading.candidateReadings = supply.candidates;
      reading.ownedRelicReadings = supply.relicReadings;
    }
    if (reading.position?.observation?.observation?.context?.kind !== 'shop'
        || (preparationRoot && preparationTaskProgress(preparationRoot, reading, presentation.locale) !== 'pending')) return reading;
    if (!shopBudget || !sameGameContextSource(shopBudget.source, reading)) {
      shopBudget = await preparationShopBudget(reading, actions => game.preview(actions, { signal }), strategicOptionCommand, presentation.locale);
      if (shopBudget && !sameGameContextSource(shopBudget.source, await game.observe({ signal }))) {
        throw new Error(runtimeText('shopChanged'));
      }
    }
    reading.shopBudget = shopBudget;
    try { reading.shopPlan = currentShopPlan(JSON.parse(await readFile(shopPlanFile, 'utf8')), reading); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    return reading;
  }
  const progressText = () => cardChoiceFrame && taskProgress === 'pending'
    ? preparation.text(cardChoiceFrame.route.kind === 'review' ? 'cardReview' : 'cardPending')
    : preparationContext?.shopSpending && taskProgress === 'pending'
      ? preparation.text('shopTask', { instruction: classification ? preparation.text('shopRecorded')
        : classificationDraft ? preparation.text('shopPending', { commands: classificationDraft.pendingCommands.join(', '),
          choice: classificationDraft.pendingChoice ? preparation.text('shopSaved', { choice: classificationDraft.pendingChoice }) : preparation.text('shopChoose') })
        : preparation.text('shopRate') })
    : preparationProgressText(taskProgress, adoptedDecision, preparationRoot, classification, classificationDraft, presentation.locale);
  const cardGuide = () => ({ automatic: cardInstructions.automatic, model: cardInstructions.guide,
    review: cardInstructions.review })[cardChoiceFrame.route.kind];
  const guideText = () => (taskScope === 'preparation' && preparationContext?.shopSpending
    ? shop.naming : text('guide.naming')) + '\n\n' + (taskScope === 'preparation'
    ? (cardChoiceFrame && taskProgress === 'pending' ? cardGuide()
      : preparationStageGuide(taskProgress, classification, preparationContext, classificationDraft, presentation.locale))
    : (config.supervised ? text('guide.coached') + '\n\n' : '') + text('guide.player'));
  const updateTaskProgress = (reading, command, basis) => {
    if (taskScope !== 'preparation') return;
    const context = reading.position?.observation?.observation?.context;
    preparationContext = context ? { ...context, potionReplacement: shopPotionReplacementScope(reading, strategicOptionCommand, presentation.locale),
      shopSpending: shopSpendingScope(reading, strategicOptionCommand, presentation.locale) } : context;
    preparationRoot ??= preparationTaskRoot(reading, presentation.locale);
    taskProgress = preparationTaskProgress(preparationRoot, reading, presentation.locale);
    if (classification && !sameGameContextSource(classification.source, reading)) classification = undefined;
    if (classificationDraft && !sameGameContextSource(classificationDraft.source, reading)) {
      classificationDraft = undefined;
      preparationFailures = 0;
    }
    if (taskProgress !== 'pending' && command && !['read', 'options'].includes(command)) adoptedDecision ??= { command, basis };
  };
  function resetBranch(agent) {
    decisionTask = undefined;
    requestBranchReset(agent);
  }
  const recording = () => mkdir(records, { recursive: true });
  function authorize(exec) {
    if (disposed) throw new Error(runtimeText('disposed'));
    if (!exec.agent) throw new Error(runtimeText('initiatingAgentRequired'));
    if (!owner) {
      owner = exec.agent;
      if (taskScope === 'preparation') adoptedDecision ??= restoredPreparationDecision(owner.session);
      // Ordinary attachment always supplies fresh state below, while eligible
      // queries can be refreshed within their task. Explicit rewinds discard
      // them through resetBranch; preparation keeps its existing reset policy.
      if (taskScope === 'preparation' && config.resetContextOnAttach !== false) requestBranchReset(owner);
    }
    if (owner !== exec.agent) throw new Error(runtimeText('alreadyBound'));
    exec.signal.throwIfAborted();
  }
  async function openPlayer() {
    opening ??= (async () => {
      await recording();
      player = await openWorkbench({ ...config, binary: config.binary || undefined,
        recordDirectory: join(records, `game-${attempt++}`), commandPrefix: '' });
      return player;
    })();
    try { return await opening; }
    catch (error) { opening = undefined; throw error; }
  }
  async function getPlayer(exec) {
    authorize(exec);
    return openPlayer();
  }
  ctx.effect(() => async () => {
    disposed = true;
    try { if (opening) await opening; }
    catch { /* a failed open has no usable game connection */ }
    if (player) await player.close();
  }, 'sts-player.close');
  // Native tools avoid granting a code runtime merely to call the game.
  const initialReading = await refreshRules();
  updateTaskProgress(initialReading);
  cardChoiceFrame = taskScope === 'preparation' && taskProgress === 'pending'
    ? preparationCardChoice(initialReading, strategicOptionCommand, presentation.locale) : undefined;
  ctx.tools.presentAs('native');
  ctx.tools.restrict({ allow: ctx.tools.schemas().filter(tool => taskTools.has(tool.name)).map(tool => tool.name) });
  ctx.tools.guard(exec => taskTools.has(exec.name) ? undefined : text('tool.guard'));
  const preparationDecision = () => adoptedDecision ? { command: adoptedDecision.command,
    ...(adoptedDecision.basis !== undefined ? { basis: adoptedDecision.basis } : {}) } : undefined;
  // Acknowledged execution can precede a failed observation. Persist its
  // identity on that error receipt too, independently of rendered prose.
  ctx.on('tools/execute', async (exec, next) => {
    const result = await next();
    return exec.name === 'sts' && exec.agent === owner && result.isError && preparationDecision()
      ? { ...result, meta: { ...result.meta, schema: 'sts-preparation-result-v1', preparationDecision: preparationDecision() } }
      : result;
  });
  ctx.systemPrompt.section({ name: 'sts:player-guide', order: 300,
    text: guideText, interpolate: false });
  ctx.tools.register(defineTool({
    name: 'sts',
    description: taskScope === 'preparation'
      ? (cardChoiceFrame ? cardInstructions.description : preparation.description)
      : text('tool.run'),
    parameters: taskScope === 'preparation' ? (cardChoiceFrame ? cardInstructions.parameters
      : preparationContext?.shopSpending ? shop.parameters
        : initialReading.position.observation.observation?.context?.kind === 'shop'
          ? preparation.shopParameters : preparation.parameters) : {
      command: { type: 'string', required: true, description: text('tool.command') },
      ...(config.supervised ? { reason: { type: 'string', description: text('tool.reason') } } : {}),
    },
    output: { schema: { type: 'object', additionalProperties: false,
      properties: { text: { type: 'string', required: true }, query: { type: 'object', additionalProperties: true },
        currentReading: { type: 'object', additionalProperties: true } } },
      render: (_args, value) => [{ type: 'text', text: value.text }],
      presentationMeta: (_args, value) => value.query || value.currentReading || preparationDecision() ? {
        ...(value.query ? { schema: 'sts-context-query-v1', ...value.query }
          : { schema: preparationDecision() ? 'sts-preparation-result-v1' : 'sts-game-result-v1' }),
        ...(value.currentReading ? { currentReading: value.currentReading } : {}),
        ...(preparationDecision() ? { preparationDecision: preparationDecision() } : {}),
      } : null },
    async execute({ command, reason, basis, choice, comparison: submittedComparison }, exec) {
      let preparationExecution;
      try {
        const game = await getPlayer(exec);
        if (config.supervised) {
          const outstanding = await readPlayerReview(config.session, presentation.locale);
          if (playerReviewOutstanding(outstanding)) {
            exec.concludeTurn();
            return { text: playerPauseText(outstanding, presentation.locale) };
          }
          const tokens = commandTokens(command);
          if (needsMaintainerReview(commandGroups(tokens[0], tokens.slice(1)))) {
            const proposal = await proposePlayerCommand(config.session, game, command, reason, presentation.locale);
            exec.concludeTurn();
            return { text: playerPauseText(proposal, presentation.locale) };
          }
        }
        if (taskScope === 'preparation') {
          const tokens = commandTokens(command), groups = commandGroups(tokens[0], tokens.slice(1));
          if (groups.length > 1 && groups.some(group => ['classify', 'choose'].includes(group[0]))) {
            throw new Error(runtimeText('singleClassificationCall'));
          }
          const texts = [];
          // Check each command at its actual successor. A leave followed by
          // go in one batch cannot cross the operator's preparation boundary.
          for (const suppliedGroup of groups) {
            let group = suppliedGroup[0] === 'act'
              ? ['act', normalizeActionTokens(suppliedGroup[1])] : suppliedGroup;
            let adoptionBasis = submittedComparison ?? basis;
            let adoptionChoice = choice;
            let choiceComparison = submittedComparison;
            if (preparationReviewCandidate && !['read', 'options'].includes(group[0])) {
              throw new Error(runtimeText('reviewMissingFact', { preparationReviewCandidate: preparationReviewCandidate }));
            }
            const reading = await prepareReading(await game.observe({ signal: exec.signal }), game, exec.signal);
            updateTaskProgress(reading);
            try {
              const frame = taskProgress === 'pending' ? preparationCardChoice(reading, strategicOptionCommand, presentation.locale) : undefined;
              if (frame) {
                if (['read', 'options'].includes(group[0])) {
                  cardChoiceFrame = preparationCardChoice(reading, strategicOptionCommand, presentation.locale);
                  return { text: renderPreparationCardChoice(cardChoiceFrame, presentation.locale) };
                }
                if (group[0] !== 'choose') throw new Error(runtimeText('cardChoiceRequired'));
                await recording();
                const { adopted, successor } = await adoptPreparationCardChoice(game, frame, reading, {
                  choice, owner: 'flash-card-choice', signal: exec.signal, locale: presentation.locale,
                  record: row => appendFile(join(records, 'adoptions.jsonl'), JSON.stringify(row) + '\n'),
                });
                preparationFailures = 0;
                updateTaskProgress(successor, adopted.command, adopted.renderedBasis);
                if (taskProgress === 'complete') exec.concludeTurn();
                else requestBranchReset(exec.agent);
                return { text: `Completed: ${adopted.command}\n${progressText()}` };
              }
              if (taskProgress === 'pending' && (shopPotionReplacementScope(reading, strategicOptionCommand, presentation.locale) || shopSpendingScope(reading, strategicOptionCommand, presentation.locale))
                  && ['read', 'options'].includes(group[0])) {
                const supply = preparationReadingSupply(reading, strategicOptionCommand, presentation.locale);
                const context = preparationClassificationContext(reading, strategicOptionCommand, classificationDraft, presentation.locale);
                return { text: `${supply.reading}\n${context}` };
              }
              if (group[0] === 'choose') {
                if (!classification) throw new Error(runtimeText('classificationRequired'));
              } else assertPreparationCommand(group, taskProgress, reading, strategicOptionCommand, presentation.locale);
              if (group[0] === 'classify') {
                if (!classification) {
                  let normalizedChoice, choiceIssue;
                  if (choice?.trim()) {
                    try {
                      const [verb, args] = preparationChoiceGroup(choice);
                      normalizedChoice = [verb, ...args].join(' ');
                    } catch (error) { choiceIssue = error.message; }
                  }
                  const collected = collectPreparationClassification(basis, reading, strategicOptionCommand, classificationDraft,
                    { choice: normalizedChoice, choiceIssue, comparison: submittedComparison }, presentation.locale);
                  await recording();
                  await appendFile(join(records, 'classification-submissions.jsonl'), JSON.stringify({
                    schema: 'sts-preparation-classification-submission-v1', at: new Date().toISOString(),
                    ...collected, basis, choice, comparison: submittedComparison, stage: taskProgress, taskRoot: preparationRoot,
                  }) + '\n');
                  classificationDraft = collected;
                  preparationFailures = collected.acceptedCommands.length ? 0 : preparationFailures + 1;
                  const receipt = preparationClassificationReceipt(collected, presentation.locale);
                  if (!collected.complete) {
                    requestBranchReset(exec.agent);
                    if (preparationFailures >= 3) {
                      exec.concludeTurn();
                      return { text: runtimeText('classificationStopped', { receipt: receipt, progress: progressText() }) };
                    }
                    return { text: `${receipt}\n\n${progressText()}` };
                  }
                  const rows = collected.rows;
                  const recorded = { at: new Date().toISOString(), source: gameContextSource(reading), basis, rows,
                    ...(collected.pendingChoice ? { pendingChoice: collected.pendingChoice,
                      ...(collected.pendingComparison !== undefined ? { pendingComparison: collected.pendingComparison } : {}) } : {}) };
                  await appendFile(join(records, 'classifications.jsonl'), JSON.stringify({
                    schema: 'sts-preparation-classification-v4',
                    ...recorded,
                    stage: taskProgress, taskRoot: preparationRoot,
                    observation: reading,
                    criteria: preparationContext?.potionReplacement ? shop.potionGuide
                      : preparationContext?.shopSpending ? shop.spendingGuide : preparation.procedure,
                    ratingMeaning: preparationContext?.potionReplacement ? 'shop-bottle-tradeoff-v1'
                      : preparationContext?.shopSpending ? 'shop-spending-tradeoff-v1' : 'current-window-recommendation-v1',
                    expertGuidance: preparationExpertGuidance(reading, strategicOptionCommand),
                    deckComparison: preparationDeckComparison(reading, strategicOptionCommand, presentation.locale),
                    shopBudget: reading.shopBudget, shopPlan: reading.shopPlan,
                    classificationContext: preparationClassificationContext(reading, strategicOptionCommand, undefined, presentation.locale),
                    runRules: rulesText,
                    groups: preparationCandidateGroups(reading, strategicOptionCommand, presentation.locale),
                    conditionBindings: preparationConditionBindings(reading, strategicOptionCommand, presentation.locale),
                  }) + '\n');
                  classification = recorded;
                  classificationDraft = undefined;
                  preparationFailures = 0;
                  if (collected.choiceIssue) {
                    requestBranchReset(exec.agent);
                    throw new Error(collected.choiceIssue);
                  }
                  adoptionChoice = collected.pendingChoice;
                  choiceComparison = collected.pendingComparison;
                }
              }
              if (['classify', 'choose'].includes(group[0])) {
                if (!adoptionChoice?.trim()) {
                  requestBranchReset(exec.agent);
                  return { text: `${preparationRecordedTable(classification, reading, strategicOptionCommand, presentation.locale)}\n\n${progressText()}` };
                }
                group = preparationChoiceGroup(adoptionChoice);
                assertPreparationCommand(group, taskProgress, reading, strategicOptionCommand, presentation.locale);
                adoptionBasis = choiceComparison;
              }
              const comparison = preparationCommandBasis(group, taskProgress, adoptionBasis, reading, classification, strategicOptionCommand, presentation.locale);
              const adoptedCommand = comparison?.command ?? [group[0], ...group[1]].join(' ');
              if (comparison) {
                await recording();
                await appendFile(join(records, 'adoptions.jsonl'), JSON.stringify({
                  schema: 'sts-preparation-adoption-v2', at: new Date().toISOString(), source: gameContextSource(reading),
                  classificationAt: classification.at, selected: comparison.command,
                  stage: taskProgress, taskRoot: preparationRoot,
                  comparison: adoptionBasis, submittedChoice: choice, adoptedChoice: adoptionChoice, ...comparison,
                }) + '\n');
                if (!sameGameContextSource(reading, await game.observe({ signal: exec.signal }))) {
                  throw new Error(runtimeText('adoptionChanged'));
                }
              }
              const result = await game.execute(adoptedCommand, { signal: exec.signal });
              if (comparison || (taskProgress === 'selection' && !['read', 'options'].includes(group[0]))) {
                preparationExecution = { adopted: { command: adoptedCommand, renderedBasis: comparison?.renderedBasis }, receipt: result };
              }
              if (comparison || taskProgress === 'selection') preparationFailures = 0;
              texts.push(result.text);
              const successor = await game.observe({ signal: exec.signal });
              if (preparationExecution) preparationExecution.successor = successor;
              updateTaskProgress(successor, adoptedCommand, comparison?.renderedBasis);
              if (reading.position.observation.observation?.context?.kind === 'shop' && comparison) {
                const plan = bindShopPotionPlan(shopPotionReplacementScope(reading, strategicOptionCommand, presentation.locale),
                  comparison.command, successor, strategicOptionCommand) ?? bindShopPlan(comparison.shopContinuation, successor);
                const temporary = `${shopPlanFile}.tmp`;
                await writeFile(temporary, JSON.stringify(plan ?? null, null, 2));
                await rename(temporary, shopPlanFile);
              }
              if (comparison && taskProgress === 'selection') requestBranchReset(exec.agent);
            } catch (error) {
              if (texts.length) error.message = `${texts.join('\n')}\n${progressText()}\nError: ${error.message}`;
              throw error;
            }
          }
          if (taskProgress === 'complete') exec.concludeTurn();
          return { text: `${texts.join('\n')}\n\n${progressText()}` };
        }
        const queryCommand = normalizeContextQuery(command);
        const beforeQuery = queryCommand ? await game.observe({ signal: exec.signal }) : undefined;
        const result = await game.execute(command, { signal: exec.signal });
        if (beforeQuery && !sameGameContextSource(beforeQuery, await game.observe({ signal: exec.signal }))) {
          throw new Error(runtimeText('queryChanged'));
        }
        if (result.branchChanged) resetBranch(exec.agent);
        return { text: result.text, ...(result.currentReading ? { currentReading: result.currentReading } : {}),
          ...(beforeQuery ? { query: { command: queryCommand,
          task: gameDecisionTask(beforeQuery, decisionTask ?? decisionContextSource(owner.session)?.task) } } : {}) };
      } catch (error) {
        const acknowledged = error.preparationExecution ?? preparationExecution;
        if (acknowledged) {
          const { adopted, successor } = acknowledged;
          adoptedDecision ??= { command: adopted.command, basis: adopted.renderedBasis };
          if (successor) updateTaskProgress(successor, adopted.command, adopted.renderedBasis);
          error.message += '\n' + preparation.text('progressAdopted', { command: adopted.command }).trimEnd()
            + (adopted.renderedBasis ? '\n' + preparation.text('progressBasis', { basis: adopted.renderedBasis }).trimEnd() : '');
        }
        if (cardChoiceFrame && taskProgress === 'pending') requestBranchReset(exec.agent);
        if (error.preparationReviewCandidate) preparationReviewCandidate = error.preparationReviewCandidate;
        if (error.branchChanged) resetBranch(exec.agent);
        if (taskScope === 'preparation' && taskProgress !== 'complete' && ++preparationFailures >= 3) {
          exec.concludeTurn();
          // dsh's error materialization omits concludesTurn. A successful
          // stop receipt preserves the failure text and official turn end.
          return { text: runtimeText('commandErrorsStopped', { error: error.message, progress: progressText() }) };
        }
        throw error;
      }
    },
  }));
  ctx.tools.register(defineTool({
    name: 'sts_feedback',
    description: text('tool.feedback'),
    parameters: { message: { type: 'string', required: true, description: text('tool.feedbackMessage') } },
    output: { schema: { type: 'string' }, render: (_args, value) => [{ type: 'text', text: value }] },
    async execute({ message }, exec) {
      authorize(exec);
      if (!message.trim()) throw new Error(text('tool.feedbackRequired'));
      await recording();
      await appendFile(join(records, 'feedback.jsonl'), JSON.stringify({
        at: new Date().toISOString(), agent: exec.agent.id, message,
        ...(preparationReviewCandidate ? { preparationReviewCandidate } : {}),
      }) + '\n');
      if (taskScope === 'preparation' && (preparationReviewCandidate || cardChoiceFrame)) {
        exec.concludeTurn();
        return text('feedback.paused', { candidate: preparationReviewCandidate ?? text('feedback.currentCard') });
      }
      return taskScope === 'preparation'
        ? text('feedback.progress', { progress: progressText() })
        : text('feedback.continue');
    },
  }));
  // Public Act/Boss/difficulty/goal and held keys refresh shared constraints
  // and preparation links. Exact enemy moves remain in combat readings and
  // explicit info queries, rather than every ordinary model request.
  // Ordinary deck changes leave this system prefix stable for provider caching.
  async function refreshRules(signal) {
    const game = await openPlayer();
    const rawReading = await game.observe({ signal });
    const reading = taskScope === 'preparation' ? await prepareReading(rawReading, game, signal) : rawReading;
    const { position } = reading;
    if (position?.kind === 'terminal') {
      rulesText = '';
      rulesKey = undefined;
      return reading;
    }
    const observed = position?.observation;
    const combat = observed?.domain === 'combat';
    const strategic = observed?.domain === 'strategic';
    if (position?.kind !== 'decision' || (!combat && !strategic)) {
      throw new Error(runtimeText('typedObservationRequired'));
    }
    const state = observed.observation;
    if (taskScope === 'preparation' && (preparationCardChoice(reading, strategicOptionCommand, presentation.locale)
        || shopPotionReplacementScope(reading, strategicOptionCommand, presentation.locale) || shopSpendingScope(reading, strategicOptionCommand, presentation.locale))) {
      rulesText = '';
      rulesKey = undefined;
      return reading;
    }
    const run = strategic ? state.run : observed.run_context;
    const key = JSON.stringify([
      strategic ? state.run.ascension_level : state.player.ascension_level,
      run?.act ?? null, (strategic ? state.public_map : run?.public_map)?.boss ?? null,
      (strategic ? state.run_goal : run?.run_goal) ?? null,
      run?.keys ?? null,
      run?.pending_act_entry ?? null,
      strategic && run?.pending_act_entry ? [run.current_hp, run.max_hp] : null,
      strategic && state.context?.kind === 'run_transition' ? state.context.stage : null,
    ]);
    if (key !== rulesKey) {
      const rules = await game.execute('info rules preparation', { signal });
      rulesText = rules.text;
      rulesKey = key;
    }
    return reading;
  }
  const currentClassificationText = reading => cardChoiceFrame || !['pending', 'selection'].includes(taskProgress) ? '' : classification
    ? preparationRecordedTable(classification, reading, strategicOptionCommand, presentation.locale) : preparationClassificationContext(reading, strategicOptionCommand, classificationDraft, presentation.locale);
  let classificationText = taskScope === 'preparation' ? currentClassificationText(initialReading) : '';
  ctx.systemPrompt.section({ name: 'sts:run-rules', order: 310,
    text: () => rulesText, interpolate: false });
  if (taskScope === 'preparation') ctx.systemPrompt.section({ name: 'sts:task-progress', order: 330,
    text: progressText, interpolate: false });
  if (taskScope === 'preparation') ctx.systemPrompt.section({ name: 'sts:classification', order: 320,
    text: () => classificationText, interpolate: false });
  // Assembly precedes agent/pre-step in dsh. Refresh here so the first request
  // after an Act change or rewind receives the current definitions immediately.
  ctx.on('system-prompt/assemble', async (assembly, context, next) => {
    const reading = await refreshRules(context.signal);
    updateTaskProgress(reading);
    cardChoiceFrame = taskScope === 'preparation' && taskProgress === 'pending'
      ? preparationCardChoice(reading, strategicOptionCommand, presentation.locale) : undefined;
    if (taskScope === 'preparation') classificationText = currentClassificationText(reading);
    if (context.agent) {
      authorize({ agent: context.agent, signal: context.signal });
      const task = taskScope === 'preparation' ? preparationRoot.task
        : gameDecisionTask(reading, decisionTask ?? decisionContextSource(owner.session)?.task);
      if (!sameGameDecisionTask(decisionTask, task) || hasPendingGameContextReset(owner)) {
        const game = await openPlayer();
        const command = reading.position.kind === 'terminal' ? 'read' : 'read, inventory effects';
        const fresh = taskScope === 'preparation' && reading.position.kind !== 'terminal'
          ? null : await game.execute(command, { signal: context.signal });
        const queries = [], queryRefs = [];
        for (const query of taskScope === 'run' ? decisionQueryReferences(owner, task, normalizeContextQuery) : []) {
          try {
            const refreshed = await game.execute(query.command, { signal: context.signal });
            const answer = refreshed.text.trim();
            const text = answer && fresh.text.includes(answer) ? runtimeText('includedInReading')
              : answer && queries.some(row => row.text === answer) ? runtimeText('includedInQuery') : refreshed.text;
            queries.push({ command: query.command, text });
            queryRefs.push(query.seq);
          } catch (error) {
            context.signal.throwIfAborted();
            queries.push({ command: query.command, text: runtimeText('queryRefreshFailed', { message: error.message }) });
          }
        }
        if (fresh && !sameGameContextSource(reading, await game.observe({ signal: context.signal }))) {
          throw new Error(runtimeText('contextChanged'));
        }
        const supply = taskScope === 'preparation' && reading.position.kind !== 'terminal'
          ? preparationReadingSupply(reading, strategicOptionCommand, presentation.locale) : { reading: fresh.text };
        if (supply.candidates) {
          reading.candidateReadings = supply.candidates;
          reading.ownedRelicReadings = supply.relicReadings;
          if (cardChoiceFrame) cardChoiceFrame = preparationCardChoice(reading, strategicOptionCommand, presentation.locale);
          classificationText = currentClassificationText(reading);
        }
        if (taskScope === 'preparation' && (shopPotionReplacementScope(reading, strategicOptionCommand, presentation.locale) || shopSpendingScope(reading, strategicOptionCommand, presentation.locale))) {
          const spending = shopSpendingScope(reading, strategicOptionCommand, presentation.locale);
          const inputBudget = (spending ? measureShopSpendingInputBudget : measureShopPotionInputBudget)([supply.reading, guideText(), classificationText, progressText()]);
          await recording();
          await appendFile(join(records, 'input-budgets.jsonl'), JSON.stringify({
            schema: 'sts-preparation-input-budget-v2', source: gameContextSource(reading),
            kind: spending ? 'shop-spending-comparison' : 'shop-potion-comparison', ...inputBudget,
          }) + '\n');
        }
        let contextReading = cardChoiceFrame ? renderPreparationCardChoice(cardChoiceFrame, presentation.locale) : supply.reading;
        if (config.supervised) {
          const review = await readPlayerReview(config.session, presentation.locale);
          const frame = { source: gameContextSource(reading), task };
          const handoff = [currentPlayerAdoption(review, frame, presentation.locale), currentPlayerGuidance(review, frame, presentation.locale)].filter(Boolean);
          if (handoff.length) contextReading = `${handoff.join('\n\n')}\n\n${contextReading}`;
        }
        requestDecisionReset(owner, contextReading, gameContextSource(reading), { task, queries, queryRefs, agentLocale: presentation.locale });
        decisionTask = task;
      }
    }
    const result = await next();
    return { ...result, sections: result.sections.map(section =>
      section.name === 'sts:player-guide' ? { ...section, text: guideText() }
        : section.name === 'sts:run-rules' ? { ...section, text: rulesText }
        : section.name === 'sts:classification' ? { ...section, text: classificationText }
        : section.name === 'sts:task-progress' ? { ...section, text: progressText() } : section) };
  });
}
