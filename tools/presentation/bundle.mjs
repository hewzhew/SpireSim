import { build } from 'esbuild';
import { parseArgs } from 'node:util';
import { resolve } from 'node:path';
import { validateCatalogs } from './index.mjs';

const { values } = parseArgs({ options: { entry: { type: 'string' }, output: { type: 'string' } } });
if (!values.entry || !values.output) throw new Error('Usage: node tools/presentation/bundle.mjs --entry FILE --output FILE');
validateCatalogs();
await build({
  entryPoints: [resolve(values.entry)], outfile: resolve(values.output),
  bundle: true, format: 'iife', platform: 'browser', target: ['es2022'],
  legalComments: 'inline', charset: 'utf8', logLevel: 'warning',
});
