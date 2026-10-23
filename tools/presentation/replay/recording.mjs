import { mkdir, appendFile, readFile, writeFile } from 'node:fs/promises';
import { resolve, join } from 'node:path';
import { createHash } from 'node:crypto';
import { conciseCardCommand } from '../../run_play/contract.mjs';

export function recordedClient(game, record, context) {
  return Object.freeze({ ...game, async act(actions, options) {
    const before = await game.observe();
    if (before.capabilities?.permission_envelope?.information_ceiling !== 'public_only') throw new Error('Replay capture requires a public-only connection');
    const metadata = context();
    const result = await game.act(actions, options);
    const matches = actions.length === 1 ? before.position.actions.filter(item => item.semantic_action === actions[0]
      || conciseCardCommand(item.semantic_action) === actions[0]) : [];
    await record({ ...metadata, actions, action: matches.length === 1 ? matches[0].semantic_action : null,
      before, after: result.raw.observation });
    return result;
  } });
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
    wrap(game, context = () => ({})) {
      return recordedClient(game, async row => {
        await appendFile(join(root, 'actions.jsonl'), JSON.stringify({ ...row, operator: metadata.operator, source_revision: metadata.source_revision }) + '\n');
        manifest.actions++;
        const missing = [...new Set([...publicCardIds(row.before), ...publicCardIds(row.after)])].filter(card => !known.has(card));
        if (missing.length) {
          for (const fact of await game.gameFacts(missing.map(card => ({ kind: 'card', card })))) {
            await appendFile(join(root, 'facts.jsonl'), JSON.stringify(fact) + '\n'); known.add(fact.definition.current.card);
          }
        }
      }, () => {
        if (manifest.status !== 'recording') throw new Error('Recording is already sealed');
        return context();
      });
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
