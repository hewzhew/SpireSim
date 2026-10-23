import { readFile, writeFile, mkdir, readdir, copyFile } from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { recordedSequence } from './sequence.mjs';

const owner = dirname(fileURLToPath(import.meta.url));
export async function buildRecording({ input, output }) {
  input = resolve(input); output = resolve(output);
  const manifest = JSON.parse(await readFile(join(input, 'manifest.json'), 'utf8'));
  if (manifest.schema !== 'spiresim-recording-v1' || manifest.status !== 'sealed') throw new Error('A replay requires a sealed recording');
  if (!manifest.files?.['actions.jsonl'] || !manifest.files?.['facts.jsonl']) throw new Error('Missing recording file digests');
  for (const [name, expected] of Object.entries(manifest.files)) {
    if (!['actions.jsonl', 'facts.jsonl', 'model-record.json'].includes(name)) throw new Error('Unknown recording file');
    const actual = createHash('sha256').update(await readFile(join(input, name))).digest('hex');
    if (actual !== expected) throw new Error('Recording digest mismatch: ' + name);
  }
  const lines = async name => (await readFile(join(input, name), 'utf8')).trim().split('\n').map(JSON.parse);
  const entries = await lines('actions.jsonl'), facts = await lines('facts.jsonl');
  if (entries.length !== manifest.actions || entries.some(row => row.operator !== manifest.operator || row.source_revision !== manifest.source_revision)) throw new Error('Recording metadata mismatch');
  const scene = recordedSequence(entries, facts, manifest.id);
  scene.provenance.model = manifest.model;
  scene.provenance.reasoning = manifest.reasoning;
  scene.provenance.scope = 'continuous';
  const record = manifest.files['model-record.json'] ? JSON.parse(await readFile(join(input, 'model-record.json'), 'utf8')) : { frames: [] };
  if (manifest.operator === 'model') {
    if (!record.frames.length) throw new Error('Model recording requires actual call records');
    for (const row of entries) {
      const frame = record.frames.find(frame => frame.step === row.call?.step);
      if (!frame?.calls.some(call => call.command === row.call.command)) throw new Error('Missing source model call for a recorded action');
    }
  }
  scene.indices = record.frames.map((_frame, index) => index);
  const first = entries[0].before.position.observation;
  const run = first.domain === 'strategic' ? first.observation.run : first.run_context;
  const character = manifest.character_id ?? run.player_class;
  if (!['ironclad', 'silent', 'defect', 'watcher'].includes(character)) throw new Error('Missing recording character');
  const story = { schema: 'sts-recorded-demo-story-v1', character_id: character, act: run.act,
    scenes: [scene], recordings: [{ id: manifest.id, title_key: manifest.operator === 'model' ? 'sequence.modelRunTitle' : 'sequence.title', scene_ids: [scene.id] }] };
  const data = { frames: record.frames };
  await mkdir(output); // Output must be new; existing review pages stay intact.
  await mkdir(join(output, 'vendor'));
  const symbols = [];
  for (const name of await readdir(join(owner, 'vendor'))) {
    await copyFile(join(owner, 'vendor', name), join(output, 'vendor', name));
    if (name.endsWith('.svg')) {
      const svg = await readFile(join(owner, 'vendor', name), 'utf8');
      symbols.push(`<symbol id="i-${name.slice(0, -4)}" viewBox="0 0 24 24">${svg.slice(svg.indexOf('>') + 1, svg.lastIndexOf('</svg>'))}</symbol>`);
    }
  }
  let html = await readFile(join(owner, 'viewer-template.html'), 'utf8');
  for (const [token, value] of Object.entries({ __ICON_SYMBOLS__: symbols.join('\n'), __DEMO_DATA__: JSON.stringify(data).replaceAll('<', '\\u003c'), __STORY_DATA__: JSON.stringify(story).replaceAll('<', '\\u003c') })) html = html.replace(token, value);
  await writeFile(join(output, 'demo.html'), html);
  await writeFile(join(output, 'story.json'), JSON.stringify(story, null, 2) + '\n');
  await writeFile(join(output, 'recording.json'), JSON.stringify(manifest, null, 2) + '\n');
  await copyFile(join(owner, 'viewer.css'), join(output, 'demo.css'));
  await copyFile(join(owner, 'THIRD_PARTY_NOTICES.txt'), join(output, 'THIRD_PARTY_NOTICES.txt'));
  await promisify(execFile)(process.execPath, [join(owner, '../bundle.mjs'), '--entry', join(owner, 'viewer.mjs'), '--output', join(output, 'demo.js')], { windowsHide: true });
  return { entry: join(output, 'demo.html'), recording: manifest.id, actions: entries.length, snapshots: scene.snapshots.length };
}
