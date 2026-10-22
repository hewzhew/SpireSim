import { build, context } from 'esbuild';
import { validateCatalogs } from '@sts-simulator/presentation/catalogs';
import { fileURLToPath } from 'node:url';
import { validateHostDictionaries, mapDictionaries, workbenchDictionaries } from './client/locale.mjs';

validateCatalogs();
validateHostDictionaries(mapDictionaries, 'Map');
validateHostDictionaries(workbenchDictionaries, 'Workbench');
const options = {
  absWorkingDir: fileURLToPath(new URL('./', import.meta.url)),
  entryPoints: [fileURLToPath(new URL('./client/index.mjs', import.meta.url))],
  outfile: fileURLToPath(new URL('./client.js', import.meta.url)),
  bundle: true, format: 'iife', platform: 'browser', target: ['es2022'],
  legalComments: 'inline', charset: 'utf8', logLevel: 'warning',
};
if (process.argv.includes('--watch')) {
  const watcher = await context(options);
  await watcher.watch();
} else await build(options);
