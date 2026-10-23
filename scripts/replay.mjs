import { parseArgs } from 'node:util';
import { buildRecording } from '../tools/presentation/replay/build.mjs';
const { values, positionals } = parseArgs({ options: { input: { type: 'string' }, output: { type: 'string' } }, allowPositionals: true });
if (positionals.length !== 1 || !['build', 'preview'].includes(positionals[0]) || !values.input || !values.output) {
  console.error('Usage: node scripts/replay.mjs <build|preview> --input RECORDING --output DIRECTORY'); process.exitCode = 1;
} else {
  console.log(JSON.stringify(await buildRecording({ ...values, preview: positionals[0] === 'preview' })));
}
