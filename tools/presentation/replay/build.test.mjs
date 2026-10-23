import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, sep } from 'node:path';
import { createRecording } from './recording.mjs';
import { buildRecording } from './build.mjs';

function frame(revision) {
  const pile = () => ({ evidence: 'public_unordered_collection', cards: [] });
  return { session_id: 'preview-test', head_revision: revision,
    capabilities: { permission_envelope: { information_ceiling: 'public_only' } },
    position: { kind: 'decision', public_snapshot_id: 'public-' + revision,
      actions: [{ semantic_action: 'end', candidate: { domain: 'combat_atomic', action: { kind: 'end_turn' } } }],
      observation: { domain: 'combat', run_context: { act: 1, floor: 1, player_class: 'ironclad', encounter_id: 'Cultist' },
        observation: { information_access: 'public', player: { hp: 75, max_hp: 75, gold: 99, block: 0, relics: [], powers: [] },
          turn: { turn_count: revision, energy: 3 }, cards: { hand: { evidence: 'public_unordered_collection', cards: [
            { card_id: 'Defend', upgrades: 0, effective_cost: 1, current_block: 5 },
          ] }, draw: pile(), discard: pile(), exhaust: pile() },
          monsters: [{ enemy: { status: 'known', enemy_id: 'Cultist' }, hp: 51, max_hp: 51, block: 0, powers: [], intent: {} }] } } } };
}

test('preview refresh keeps its address and recording, while release outputs remain immutable', async t => {
  const base = await realpath(tmpdir()), root = await mkdtemp(join(base, 'spiresim-preview-'));
  t.after(async () => {
    const target = await realpath(root);
    assert.ok(target.startsWith(base + sep) && target.includes('spiresim-preview-'));
    await rm(target, { recursive: true });
  });
  const input = join(root, 'recording'), output = join(root, 'preview'), release = join(root, 'release');
  const recording = await createRecording({ directory: input, metadata: { id: 'preview-test', source_revision: 'game-producer', operator: 'developer' } });
  const observer = recording.observer({ async gameFacts() { return [{ kind: 'card',
    definition: { current: { card: 'Defend', card_type: 'Skill', cost: 1, block: 5 } },
    face_templates: [{ upgrades: 0, field: 'DESCRIPTION', damage: 0, block: 5, magic: 0 }] }]; } });
  const before = frame(0), after = frame(1), action = { command: 'act', actions: ['end'] };
  const context = await observer({ phase: 'before', before, input: action });
  await observer({ phase: 'committed', before, after, input: action, context });
  await recording.seal();
  const originalInput = await readFile(join(input, 'manifest.json'));
  const first = await buildRecording({ input, output, preview: true });
  const story = await readFile(join(output, 'story.json'));
  const css = await readFile(join(output, 'demo.css'));
  await writeFile(join(output, 'demo.css'), 'temporary preview edit');
  await writeFile(join(output, 'notes.txt'), 'keep this local note');
  const second = await buildRecording({ input, output, preview: true });
  assert.equal(first.entry, second.entry);
  assert.deepEqual(await readFile(join(output, 'story.json')), story);
  assert.deepEqual(await readFile(join(output, 'demo.css')), css);
  assert.deepEqual(await readFile(join(input, 'manifest.json')), originalInput);
  assert.equal(await readFile(join(output, 'notes.txt'), 'utf8'), 'keep this local note');
  assert.equal(JSON.parse(await readFile(join(output, 'preview.json'), 'utf8')).status, 'ready');
  await buildRecording({ input, output: release });
  await assert.rejects(buildRecording({ input, output: release }), { code: 'EEXIST' });
  await assert.rejects(buildRecording({ input, output: release, preview: true }), /created by replay preview/);
  const marker = JSON.parse(await readFile(join(output, 'preview.json'), 'utf8'));
  marker.recording = 'another-recording';
  await writeFile(join(output, 'preview.json'), JSON.stringify(marker));
  await assert.rejects(buildRecording({ input, output, preview: true }), /different recording/);
  assert.deepEqual(await readFile(join(output, 'story.json')), story);
});
