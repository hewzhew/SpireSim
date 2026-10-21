import { build, context } from 'esbuild';
import { validateCatalogs } from '@sts-simulator/presentation/catalogs';
import { fileURLToPath } from 'node:url';
import { validateMapDictionaries } from './client/locale.mjs';

validateCatalogs();
validateMapDictionaries();
const options = {
  entryPoints: [fileURLToPath(new URL('./client/index.mjs', import.meta.url))],
  outfile: fileURLToPath(new URL('./client.js', import.meta.url)),
  bundle: true, format: 'iife', platform: 'browser', target: ['es2022'],
  legalComments: 'inline', charset: 'utf8', logLevel: 'warning',
};
if (process.argv.includes('--watch')) {
  const watcher = await context(options);
  await watcher.watch();
} else await build(options);
