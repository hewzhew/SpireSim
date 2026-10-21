// Session argument resolution: an absolute path, a repository-relative
// path, or a short name that uniquely matches one directory under
// .oracle-lab/run-play.
import { readdirSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';

export const repository = fileURLToPath(new URL('../../', import.meta.url));

const COMMANDS = new Set(['new', 'read', 'act', 'preview', 'go', 'use', 'loot', 'board', 'map',
  'info', 'inventory', 'deck-lab', 'deck', 'piles', 'options', 'cancel', 'undo', 'back', 'jump', 'mark', 'refresh', 'exe', 'research', 'plans', 'commit', 'review', 'pick', 'scout', 'player']);

export function freshRecordDirectory(repo, label, operation = '') {
  const safeLabel = label.replaceAll(/[^a-zA-Z0-9._-]/g, '_');
  const tag = operation ? `-${operation}` : '';
  return join(repo, '.oracle-lab', 'run-play-records',
    `${safeLabel}${tag}-${Date.now()}-${randomUUID()}`);
}

// Quotes group a token, but no shell expansion or escape evaluation occurs.
// A separator may touch the preceding token when followed by whitespace or
// end-of-input. Internal commas stay in coordinates and card selections.
export function commandTokens(line) {
  if (typeof line !== 'string') throw new Error('game command must be text');
  const tokens = [];
  let token = '', quote = null, started = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (quote !== null) {
      if (ch === quote) quote = null;
      else token += ch;
    } else if ((ch === '"' || ch === "'") && (!started || token.endsWith(':'))) {
      quote = ch; started = true;
    } else if ((ch === ',' || ch === ';') && (i + 1 === line.length || /\s/.test(line[i + 1]))) {
      if (started) { tokens.push(token); token = ''; started = false; }
      tokens.push(ch);
    } else if (/\s/.test(ch)) {
      if (started) { tokens.push(token); token = ''; started = false; }
    } else { token += ch; started = true; }
  }
  if (quote !== null) throw new Error('unclosed quote in game command');
  if (started) tokens.push(token);
  return tokens;
}

// Commas continue an act/preview line until an explicit command starts.
// A semicolon always ends the line. Keep unknown action text in the same
// transaction so Rust rejects it before any prefix is published.
// Navigation and mark changes are validated before any command can execute.
export function commandGroups(command, parameters) {
  const groups = [{ tokens: [], separator: null }];
  for (const arg of [command, ...parameters]) {
    if (arg === ';' || arg === ',') { groups.push({ tokens: [], separator: arg }); continue; }
    groups[groups.length - 1].tokens.push(arg);
  }
  const commands = [];
  for (const { tokens, separator } of groups) {
    if (!tokens.length) continue;
    const previous = commands.at(-1);
    if (separator === ',' && previous && ['act', 'preview'].includes(previous[0])
        && (tokens[0] === 'go' || !COMMANDS.has(tokens[0]))) previous[1].push(...tokens);
    else commands.push([tokens[0], tokens.slice(1)]);
  }
  if (commands.length === 0) commands.push([command, parameters]);
  if (commands.some(([name]) => name === 'jump')) {
    if (commands.length !== 1 || commands[0][1].length > 1) {
      throw new Error('usage: ./p jump [nNODE | source:hN | target:hN]; run jump as a separate command');
    }
  }
  if (commands.length !== 1 && commands.some(([name, args]) => name === 'undo'
      || (name === 'back' && !(args.length === 1 && args[0] === 'list'))
      || (name === 'mark' && args.length > 0))) {
    throw new Error('run navigation or mark changes as a separate command');
  }
  return commands;
}

function isSessionPath(input) {
  return /^[./\\]|^[a-zA-Z]:|[\\/]/.test(input);
}

function findSession(input, repo) {
  const sessionsRoot = join(repo, '.oracle-lab', 'run-play');
  let names = [];
  try {
    names = readdirSync(sessionsRoot, { withFileTypes: true })
      .filter(entry => entry.isDirectory())
      .map(entry => entry.name)
      .filter(name => name.toLowerCase().includes(input.toLowerCase()))
      .sort();
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  const exact = names.filter(name => name.toLowerCase() === input.toLowerCase());
  const matches = exact.length ? exact : names;
  if (matches.length === 1) return join(sessionsRoot, matches[0]);
  if (matches.length > 1) {
    throw new Error(`ambiguous session "${input}": ${matches.join(', ')}; pass an explicit path instead`);
  }
  return null;
}

export function resolveSession(input, repo = repository) {
  if (typeof input !== 'string' || !input.trim()) throw new Error('session must not be empty');
  const normalized = input.replaceAll('\\', '/');
  if (/^(?:[a-zA-Z]:\/|\/)/.test(normalized)) return normalized;
  if (isSessionPath(input)) return join(repo, normalized);
  const found = findSession(input, repo);
  if (found) return found;
  throw new Error(`no session under .oracle-lab/run-play matches "${input}"; pass an explicit path instead`);
}

// New runs may name a directory that does not exist yet. Keep ordinary
// session lookup strict so a misspelled existing-session name never creates a
// different run by accident.
export function resolveNewSession(input, repo = repository) {
  if (typeof input !== 'string' || !input.trim()) throw new Error('session must not be empty');
  if (isSessionPath(input)) return resolveSession(input, repo);
  if (!/^[a-zA-Z0-9][a-zA-Z0-9._-]*$/.test(input)) {
    throw new Error('new session name must use letters, digits, dots, underscores, or hyphens');
  }
  return join(repo, '.oracle-lab', 'run-play', input);
}

export async function readCurrentSession(repo = repository) {
  try {
    return (await readFile(join(repo, '.oracle-lab', 'run-play-records', 'current-session'), 'utf8')).trim() || null;
  } catch (error) {
    if (error.code === 'ENOENT') return null;
    throw error;
  }
}

// Resolve the session before interpreting the remaining arguments as commands.
// Known commands and row numbers always address the default session. Existing
// session names precede bare option names. An explicit session path removes
// either collision: command-named sessions or session-named options.
export function resolveInvocation(args, currentSession, repo = repository) {
  const first = args[0] ?? '';
  if (first && isSessionPath(first)) {
    return { session: resolveSession(first, repo), command: args[1], parameters: args.slice(2) };
  }
  const usesDefault = !first || COMMANDS.has(first) || /^\d+$/.test(first)
    || /^[a-z][a-z-]*:/.test(first) || /#\d+/.test(first);
  const namedSession = usesDefault ? null : findSession(first, repo);
  if (namedSession) {
    return { session: namedSession, command: args[1], parameters: args.slice(2) };
  }
  if (currentSession) {
    return { session: resolveSession(currentSession, repo), command: first || undefined, parameters: args.slice(1) };
  }
  if (!usesDefault) resolveSession(first, repo); // Report an unmatched session name.
  throw new Error('no current session; pass a session name or an explicit path');
}
