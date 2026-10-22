// Coached play: native game state remains authoritative; model commands are
// proposals until a maintainer adopts them at their unchanged source.
import { readFile, writeFile, rename } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createPresentation } from '@sts-simulator/presentation';
import { gameContextSource, sameGameContextSource, gameDecisionTask, sameGameDecisionTask } from './context.mjs';

const reviewFile = session => join(session, 'dsh-review.json');
const bindingFile = session => join(session, 'dsh-player.json');

function supervisionText(locale, key, parameters = {}) {
  return createPresentation({ locale }).text(`scripted:supervision.${key}`,
    Object.fromEntries(Object.entries(parameters).map(([name, value]) => [name, String(value)])));
}

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

export async function capturePlayerFrame(game, previousTask, locale = 'en') {
  const before = await game.observe();
  const result = await game.execute(before.position.kind === 'terminal' ? 'read' : 'read, inventory');
  const after = await game.observe();
  if (!sameGameContextSource(before, after)) throw new Error(supervisionText(locale, 'frameChanged'));
  return { source: gameContextSource(after), task: gameDecisionTask(after, previousTask), text: result.text };
}

export async function readPlayerReview(session, locale = 'en') {
  const review = await readJson(reviewFile(session));
  if (review && review.schema !== 'sts-dsh-review-v1') throw new Error(supervisionText(locale, 'unsupportedReview'));
  return review;
}

export async function readPlayerBinding(session, locale = 'en') {
  const binding = await readJson(bindingFile(session));
  if (binding && (binding.schema !== 'sts-dsh-player-binding-v1'
      || resolve(binding.session).toLowerCase() !== resolve(session).toLowerCase())) {
    throw new Error(supervisionText(locale, 'bindingMismatch'));
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

export function playerPauseText(review, locale = 'en') {
  return review.status === 'pending'
    ? supervisionText(locale, 'pendingPause', { command: review.command, reason: review.reason ? supervisionText(locale, 'reason', { reason: review.reason }) : '' })
    : supervisionText(locale, 'reviewPause', { status: review.status });
}

export async function proposePlayerCommand(session, game, command, reason = '', locale = 'en') {
  const previous = await readPlayerReview(session, locale);
  if (playerReviewOutstanding(previous)) throw new Error(supervisionText(locale, 'outstandingProposal'));
  const frame = await capturePlayerFrame(game, previous?.frameAfter?.task ?? previous?.frame?.task ?? previous?.guidance?.task, locale);
  const proposal = { schema: 'sts-dsh-review-v1', id: randomUUID(), status: 'pending',
    at: new Date().toISOString(), command, reason, source: frame.source, frame,
    ...(previous?.guidance ? { guidance: previous.guidance } : {}) };
  await writeJson(reviewFile(session), proposal);
  return proposal;
}

export function renderPlayerReview(frame, review, locale = 'en') {
  const lines = [frame.text.trimEnd()];
  if (review?.status === 'pending') {
    lines.push('', supervisionText(locale, 'pendingProposal', { command: review.command }));
    if (review.reason) lines.push(supervisionText(locale, 'flashReason', { reason: review.reason }));
    if (!sameGameContextSource(review.source, frame.source)) lines.push(supervisionText(locale, 'staleProposal'));
    lines.push(supervisionText(locale, 'reviewCommands'));
  } else if (review) {
    lines.push('', supervisionText(locale, 'lastReview', { status: review.status, commandSuffix: review.selectedCommand ? ` · ${review.selectedCommand}` : '' }));
    if (review.error) lines.push(supervisionText(locale, 'error', { error: review.error }));
  }
  return lines.join('\n') + '\n';
}

export async function setPlayerGuidance(session, frame, text, { reject = false, locale = 'en' } = {}) {
  if (!text.trim()) throw new Error(supervisionText(locale, 'guidanceRequired'));
  if (playerIsRunning(await readPlayerBinding(session, locale))) throw new Error(supervisionText(locale, 'waitToGuide'));
  const previous = await readPlayerReview(session, locale);
  if (previous?.status === 'executing' && !reject) throw new Error(supervisionText(locale, 'uncertainExecution'));
  const review = { ...(previous ?? { schema: 'sts-dsh-review-v1', status: 'guidance' }),
    ...(reject ? { status: 'rejected', rejectedAtSource: frame.source } : {}),
    guidance: { task: frame.task, text, at: new Date().toISOString() } };
  await writeJson(reviewFile(session), review);
  return review;
}

export function currentPlayerGuidance(review, frame, locale = 'en') {
  return review?.guidance && sameGameDecisionTask(review.guidance.task, frame.task)
    ? supervisionText(locale, 'currentGuidance', { guidance: review.guidance.text }) : '';
}

export function currentPlayerAdoption(review, frame, locale = 'en') {
  if (review?.status !== 'adopted' || !sameGameContextSource(review.sourceAfter, frame.source)) return '';
  if (!sameGameDecisionTask(review.frame?.task, frame.task)) {
    return supervisionText(locale, 'previousDecisionCompleted', { previousTask: review.frame?.task?.family ?? 'game', currentTask: frame.task.family });
  }
  const correction = review.selectedCommand !== review.command ? supervisionText(locale, 'commandReplaced', { command: review.command }) : '';
  const noteIsCurrentGuidance = review.guidance && review.guidance.text === review.maintainerNote
    && sameGameDecisionTask(review.guidance.task, frame.task);
  return supervisionText(locale, 'adoptedChoice', { task: frame.task.family, command: review.selectedCommand, correction: correction, reason: review.maintainerNote && !noteIsCurrentGuidance ? supervisionText(locale, 'maintainerReason', { reason: review.maintainerNote }) : '' });
}

export async function adoptPlayerProposal(session, game, { command, note = '', locale = 'en' } = {}) {
  const binding = await readPlayerBinding(session, locale);
  if (playerIsRunning(binding)) throw new Error(supervisionText(locale, 'waitToAdopt'));
  const proposal = await readPlayerReview(session, locale);
  if (proposal?.status !== 'pending') throw new Error(supervisionText(locale, 'noPendingProposal'));
  const before = await game.observe();
  if (!sameGameContextSource(proposal.source, before)) throw new Error(supervisionText(locale, 'sourceChanged'));
  const selectedCommand = command ?? proposal.command;
  if (!selectedCommand.trim()) throw new Error(supervisionText(locale, 'emptyCommand'));
  const maintainerNote = note || (proposal.guidance && sameGameDecisionTask(proposal.guidance.task, gameDecisionTask(before))
    ? proposal.guidance.text : '');
  const executing = { ...proposal, status: 'executing', selectedCommand, maintainerNote };
  await writeJson(reviewFile(session), executing);
  try {
    const result = await game.execute(selectedCommand);
    const frame = await capturePlayerFrame(game, proposal.frame.task, locale);
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
