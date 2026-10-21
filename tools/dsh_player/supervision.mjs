// Coached play: native game state remains authoritative; model commands are
// proposals until a maintainer adopts them at their unchanged source.
import { readFile, writeFile, rename } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { gameContextSource, sameGameContextSource, gameDecisionTask, sameGameDecisionTask } from './context.mjs';

const reviewFile = session => join(session, 'dsh-review.json');
const bindingFile = session => join(session, 'dsh-player.json');

async function readJson(path) {
  try { return JSON.parse(await readFile(path, 'utf8')); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}

async function writeJson(path, value) {
  const temporary = `${path}.${randomUUID()}.tmp`;
  await writeFile(temporary, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
  await rename(temporary, path);
}

export function needsMaintainerReview(groups) {
  return groups.some(([command, args]) => {
    if (['read', 'options', 'inventory', 'map', 'info', 'piles', 'preview', 'research', 'plans', 'review', 'board', 'deck-lab'].includes(command)) return false;
    if (command === 'back' && args.length === 1 && args[0] === 'list') return false;
    if (command === 'mark' && !args.length) return false;
    // Gold collection and opening a card page make no strategic choice.
    if (/^claim:Gold(?:#\d+)?$/.test(command) || /^open-card(?:#\d+)?$/.test(command)) return false;
    return true;
  });
}

export async function capturePlayerFrame(game, previousTask) {
  const before = await game.observe();
  const result = await game.execute(before.position.kind === 'terminal' ? 'read' : 'read, inventory');
  const after = await game.observe();
  if (!sameGameContextSource(before, after)) throw new Error('Game changed while reading the coached-play frame; inspect again.');
  return { source: gameContextSource(after), task: gameDecisionTask(after, previousTask), text: result.text };
}

export async function readPlayerReview(session) {
  const review = await readJson(reviewFile(session));
  if (review && review.schema !== 'sts-dsh-review-v1') throw new Error('Unsupported coached-play review record.');
  return review;
}

export async function readPlayerBinding(session) {
  const binding = await readJson(bindingFile(session));
  if (binding && (binding.schema !== 'sts-dsh-player-binding-v1'
      || resolve(binding.session).toLowerCase() !== resolve(session).toLowerCase())) {
    throw new Error('Coached player binding does not match this game session.');
  }
  return binding;
}

export async function bindCoachedPlayer(session, binding) {
  await writeJson(bindingFile(session), { ...binding, schema: 'sts-dsh-player-binding-v1', session });
}

export function playerIsRunning(binding) {
  if (binding?.running !== true || !Number.isSafeInteger(binding.pid)) return false;
  try { process.kill(binding.pid, 0); return true; }
  catch (error) { if (error.code === 'ESRCH') return false; throw error; }
}

export function playerReviewOutstanding(review) {
  return ['pending', 'executing', 'execution-error'].includes(review?.status);
}

export function playerPauseText(review) {
  return review.status === 'pending'
    ? `Pending proposal (not executed): ${review.command}${review.reason ? `\nReason: ${review.reason}` : ''}\nThe maintainer will review this choice. No further command was executed.`
    : `Maintainer review required: ${review.status}. Inspect the actual game state and outstanding execution before continuing. No further command was executed.`;
}

export async function proposePlayerCommand(session, game, command, reason = '') {
  const previous = await readPlayerReview(session);
  if (playerReviewOutstanding(previous)) throw new Error('A prior proposal still requires maintainer handling.');
  const frame = await capturePlayerFrame(game, previous?.frameAfter?.task ?? previous?.frame?.task ?? previous?.guidance?.task);
  const proposal = { schema: 'sts-dsh-review-v1', id: randomUUID(), status: 'pending',
    at: new Date().toISOString(), command, reason, source: frame.source, frame,
    ...(previous?.guidance ? { guidance: previous.guidance } : {}) };
  await writeJson(reviewFile(session), proposal);
  return proposal;
}

export function renderPlayerReview(frame, review) {
  const lines = [frame.text.trimEnd()];
  if (review?.status === 'pending') {
    lines.push('', `Pending proposal (not executed): ${review.command}`);
    if (review.reason) lines.push(`Flash reason: ${review.reason}`);
    if (!sameGameContextSource(review.source, frame.source)) lines.push('Proposal is stale; accepting or replacing it will be refused.');
    lines.push('player accept | player replace <command> | player reject <guidance>');
  } else if (review) {
    lines.push('', `Last review: ${review.status}${review.selectedCommand ? ` · ${review.selectedCommand}` : ''}`);
    if (review.error) lines.push(`Error: ${review.error}`);
  }
  return lines.join('\n') + '\n';
}

export async function setPlayerGuidance(session, frame, text, { reject = false } = {}) {
  if (!text.trim()) throw new Error('Provide a concise maintainer instruction.');
  if (playerIsRunning(await readPlayerBinding(session))) throw new Error('Wait for the player to stop before changing guidance.');
  const previous = await readPlayerReview(session);
  if (previous?.status === 'executing' && !reject) throw new Error('Inspect the uncertain execution, then explicitly reject it before continuing.');
  const review = { ...(previous ?? { schema: 'sts-dsh-review-v1', status: 'guidance' }),
    ...(reject ? { status: 'rejected', rejectedAtSource: frame.source } : {}),
    guidance: { task: frame.task, text, at: new Date().toISOString() } };
  await writeJson(reviewFile(session), review);
  return review;
}

export function currentPlayerGuidance(review, frame) {
  return review?.guidance && sameGameDecisionTask(review.guidance.task, frame.task)
    ? `Maintainer guidance for this decision task (check against the latest game reading):\n${review.guidance.text}` : '';
}

export function currentPlayerAdoption(review, frame) {
  if (review?.status !== 'adopted' || !sameGameContextSource(review.sourceAfter, frame.source)) return '';
  if (!sameGameDecisionTask(review.frame?.task, frame.task)) {
    return `The maintainer completed the previous ${review.frame?.task?.family ?? 'game'} decision. The supplied reading is its actual successor; the displayed ${frame.task.family} choices are still available.`;
  }
  const correction = review.selectedCommand !== review.command ? ` Your proposed command was ${review.command}; the maintainer replaced it.` : '';
  const noteIsCurrentGuidance = review.guidance && review.guidance.text === review.maintainerNote
    && sameGameDecisionTask(review.guidance.task, frame.task);
  return `The maintainer already executed this choice in the current ${frame.task.family} task: ${review.selectedCommand}. The supplied reading is after that execution; do not repeat it.${correction}${review.maintainerNote && !noteIsCurrentGuidance ? `\nMaintainer reason: ${review.maintainerNote}` : ''}`;
}

export async function adoptPlayerProposal(session, game, { command, note = '' } = {}) {
  const binding = await readPlayerBinding(session);
  if (playerIsRunning(binding)) throw new Error('Wait for the player to stop before adopting a proposal.');
  const proposal = await readPlayerReview(session);
  if (proposal?.status !== 'pending') throw new Error('No pending proposal to adopt.');
  const before = await game.observe();
  if (!sameGameContextSource(proposal.source, before)) throw new Error('Proposal source changed. Inspect and reject the stale proposal before asking Flash again.');
  const selectedCommand = command ?? proposal.command;
  if (!selectedCommand.trim()) throw new Error('The selected command must not be empty.');
  const maintainerNote = note || (proposal.guidance && sameGameDecisionTask(proposal.guidance.task, gameDecisionTask(before))
    ? proposal.guidance.text : '');
  const executing = { ...proposal, status: 'executing', selectedCommand, maintainerNote };
  await writeJson(reviewFile(session), executing);
  try {
    const result = await game.execute(selectedCommand);
    const frame = await capturePlayerFrame(game, proposal.frame.task);
    const review = { ...executing, status: 'adopted', sourceAfter: frame.source, frameAfter: frame };
    await writeJson(reviewFile(session), review);
    return { ...result, frame, review };
  } catch (error) {
    // A compound command may have applied a prefix, or a transport may be
    // uncertain. Never return this proposal to pending or retry automatically.
    await writeJson(reviewFile(session), { ...executing, status: 'execution-error', error: error.message });
    throw error;
  }
}
