// Retained decision navigation belongs to Rust. The workbench only exposes
// its selectors and reopens the connection after a successful jump.
import { writeText } from '../render.mjs';
import { spawn } from 'node:child_process';

// Settle only on close, including cancellation: the session must not be reused
// while its previous subprocess can still be running. Never retry an operation.
export async function runEngine(binary, args, { signal, ...options }) {
  signal?.throwIfAborted();
  return new Promise((resolve, reject) => {
    const child = spawn(binary, args, { ...options, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '', stderr = '', failure, aborted = false;
    child.stdout.setEncoding('utf8'); child.stderr.setEncoding('utf8');
    child.stdout.on('data', text => { stdout += text; });
    child.stderr.on('data', text => { stderr += text; });
    const abort = () => { aborted = true; child.kill(); };
    signal?.addEventListener('abort', abort, { once: true });
    child.on('error', error => { failure = error; });
    child.on('close', (status, exitSignal) => {
      signal?.removeEventListener('abort', abort);
      if (aborted) reject(signal.reason);
      else if (failure) reject(failure);
      else resolve({ status, signal: exitSignal, stdout, stderr });
    });
    if (signal?.aborted) abort();
  });
}

export async function engineOutput(ctx, args, { signal } = {}) {
  const { binary, session, repository } = ctx;
  const result = await (ctx.runEngine ?? runEngine)(binary, [args[0], '--session', session, ...args.slice(1)],
    { cwd: repository, encoding: 'utf8', windowsHide: true, ...(signal ? { signal } : {}) });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error([result.stdout, result.stderr].filter(Boolean).join('\n').trim()
      || `${args[0]} failed (${result.signal ?? result.status ?? 'unknown status'})`);
  }
  return result.stdout;
}

export async function jumpTo(ctx, selector) {
  await engineOutput(ctx, ['jump', selector]);
}

export function jumpPoint(reading, status) {
  if (reading.control === 'terminal') return null;
  // Normal combat readings intentionally omit the choice source. Use the
  // existing diagnostic show surface, then bind it to the typed snapshot in
  // a fresh reading. Never infer a node from a head revision or an action count.
  const node = /^(?:status|run-play)\b.*\bnode=(n\S+)/m.exec(status)?.[1];
  const snapshot = /^depth=.*\bsnapshot=(\S+)/m.exec(status)?.[1];
  const info = reading.raw?.reading;
  const observed = info?.state?.information?.snapshot_id;
  if (!node || !snapshot || typeof observed !== 'string') {
    throw new Error('cannot read a return point from the current status and decision');
  }
  const source = info.source;
  if (!observed.split(':').at(-1).endsWith(snapshot)) {
    throw new Error('decision changed while reading the return point; read ./p jump again');
  }
  if (source && (!source.archive_node_id.endsWith(node.slice(1)) || source.public_snapshot_id !== observed)) {
    throw new Error('decision changed while reading the return point; read ./p jump again');
  }
  // The short selector is issued by Rust itself; Rust also rejects an
  // absent or ambiguous selector on jump instead of choosing another node.
  return node;
}

export async function showJumpPoint(ctx) {
  const write = ctx.write ?? writeText;
  const status = await engineOutput(ctx, ['show']);
  const reading = await ctx.game.decision({ details: ['brief', 'state'] });
  const selector = jumpPoint(reading, status);
  await ctx.printCurrentReading(reading);
  if (selector === null) {
    write('No current decision at this terminal state. Earlier retained decisions do not require a prior jump call.\n'
      + 'Use ./p back to restore the previous retained decision, or ./p jump <a previously returned node>.\n');
    return;
  }
  write(`Return point (${ctx.sessionLabel}): ./p jump ${selector}\n`);
}
