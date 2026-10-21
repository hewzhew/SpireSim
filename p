#!/usr/bin/env node
// Two-character entry to tools/run_play/play.mjs for interactive sessions:
//   ./p <session> act play:Strike+0@1*3
// The workbench remains the single owner; this file only forwards argv.
// Errors print as one friendly block; P_DEBUG=1 keeps the stack for
// tooling work.
import { main } from './tools/run_play/play.mjs';
try {
  await main();
} catch (error) {
  if (process.env.P_DEBUG) throw error;
  const message = error && error.message ? error.message : String(error);
  const code = error?.code ? `[${error.code}] ` : '';
  process.stderr.write(`${message.startsWith('✗') ? '' : '✗ '}${code}${message}\n`);
  process.exitCode = 1;
}
