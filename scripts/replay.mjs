import { parseArgs } from 'node:util';
import { buildRecording } from '../tools/presentation/replay/build.mjs';
const { values, positionals } = parseArgs({ options: { input: { type: 'string' }, output: { type: 'string' } }, allowPositionals: true });
if (positionals[0] !== 'build' || !values.input || !values.output) {
  console.error('Usage: node scripts/replay.mjs build --input RECORDING --output NEW_DIRECTORY'); process.exitCode = 1;
} else {
  console.log(JSON.stringify(await buildRecording(values)));
}
