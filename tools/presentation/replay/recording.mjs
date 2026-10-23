import { mkdir, appendFile, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { conciseCardCommand } from '../../run_play/contract.mjs';

export function recordedTransition(before, input, candidate) {
  const options = before.position.actions;
  const matches = input.command === 'act' && input.actions.length === 1
    ? options.filter(item => item.semantic_action === input.actions[0] || conciseCardCommand(item.semantic_action) === input.actions[0])
    : input.command === 'choose' && candidate && !input.following_rewards?.length
      ? options.filter(item => isDeepStrictEqual(item.candidate, candidate)) : [];
  const action = matches.length === 1 ? matches[0].semantic_action : null;
  return { input, actions: input.command === 'act' ? input.actions : action ? [action] : [], action };
}

function publicCardIds(frame) {
  const view = frame.position?.observation;
  if (!view) return [];
  const state = view.observation;
  if (view.domain === 'combat') return ['hand', 'draw', 'discard', 'exhaust'].flatMap(zone => state.cards[zone].cards.map(card => card.card_id));
  return [...state.cards.map(card => card.card), ...(state.context.pending_rewards ?? [])
    .filter(reward => reward.kind === 'card_reward').flatMap(reward => reward.cards.map(card => card.card))];
}

export async function createRecording({ directory, metadata }) {
  if (!metadata?.id || !metadata.source_revision || !['developer', 'model'].includes(metadata.operator)) throw new Error('Recording needs an ID, producer revision and operator');
  const root = resolve(directory);
  await mkdir(root); // A capture never appends into another recording.
  const manifest = { ...metadata, schema: 'spiresim-recording-v1', status: 'recording', actions: 0, files: {} };
  await writeFile(join(root, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n', { flag: 'wx' });
  const known = new Set();
  return Object.freeze({
    observer(game, context = () => ({})) {
      return async event => {
        if (event.phase === 'before') {
          if (manifest.status !== 'recording') throw new Error('Recording is already sealed');
          if (event.before.capabilities?.permission_envelope?.information_ceiling !== 'public_only') throw new Error('Replay capture requires a public-only connection');
          return { ...context(), ...recordedTransition(event.before, event.input, event.candidate) };
        }
        if (event.phase !== 'committed') throw new Error('Unknown recording transition phase');
        const row = { ...event.context, before: event.before, after: event.after };
        await appendFile(join(root, 'actions.jsonl'), JSON.stringify({ ...row, operator: metadata.operator, source_revision: metadata.source_revision }) + '\n');
        manifest.actions++;
        const missing = [...new Set([...publicCardIds(row.before), ...publicCardIds(row.after)])].filter(card => !known.has(card));
        if (missing.length) {
          for (const fact of await game.gameFacts(missing.map(card => ({ kind: 'card', card })))) {
            await appendFile(join(root, 'facts.jsonl'), JSON.stringify(fact) + '\n'); known.add(fact.definition.current.card);
          }
        }
      };
    },
    async seal({ modelRecord } = {}) {
      if (manifest.status !== 'recording') throw new Error('Recording is already sealed');
      if (!manifest.actions) throw new Error('Cannot seal an empty recording');
      const names = ['actions.jsonl', 'facts.jsonl'];
      if (modelRecord) { await writeFile(join(root, 'model-record.json'), JSON.stringify(modelRecord, null, 2) + '\n', { flag: 'wx' }); names.push('model-record.json'); }
      for (const name of names) manifest.files[name] = createHash('sha256').update(await readFile(join(root, name))).digest('hex');
      manifest.status = 'sealed';
      await writeFile(join(root, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
      return manifest;
    },
  });
}
