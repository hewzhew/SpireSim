#!/usr/bin/env node
// Node workbench over client.mjs and the run_play executable: one short
// command per decision during manual or search-assisted play.
//
//   node scripts/p.mjs                                 reading (current session)
//   node scripts/p.mjs Disarm                          choose an option by its name
//   node scripts/p.mjs take Disarm                     same choice with the verb spelled
//   node scripts/p.mjs 0                              one current choice by row number
//   node scripts/p.mjs claim:BagOfPreparation#0        named reward (rows can move)
//   node scripts/p.mjs go [x,y]                       route by coordinate, or sole map exit
//   node scripts/p.mjs act <action...>                 combat line: play:X potion:X end
//   node scripts/p.mjs preview <action...>             same line, private engine branch
//   node scripts/p.mjs board                           combat table (incoming vs block)
//   node scripts/p.mjs map [full|flip|set flip|normal] current-map routes; full expands grid
//   node scripts/p.mjs loot / info / cancel            claim / mechanics / back into room
//   node scripts/p.mjs use <session>                   switch the default session
//   node scripts/p.mjs undo                            roll back the last public action
//   node scripts/p.mjs back [steps]                    restore a retained earlier decision
//   node scripts/p.mjs jump [nNODE|source:hN|target:hN] show a return point, or jump to one
//   node scripts/p.mjs refresh <fresh-session>         verify old state, create new root, select it
//   node scripts/p.mjs research|plans|review|commit    search, then inspect/adopt saved results
//   node scripts/p.mjs exe <args...>                   executable passthrough
//   node scripts/p.mjs new <session> <seed> [--ascension N] [--goal g]
//   node scripts/p.mjs <cmd> , <cmd> ...               several commands, one connection
//
// The mental model is three verbs: choose what you see (a bare name, an
// optional verb prefix, or the row number — the display brackets the
// engine's own verb so what-you-see is what-you-type), fight in lines
// (act; preview tries a line on an isolated engine branch),
// look at the world (reading, board, map, info). Option rows carry
// `[verb]` annotations; combat hands show adjusted numbers in the
// English abbreviations (2c 33d 5b), with `*` marking cards that just arrived.
// end! is accepted as end; intentional unspent energy needs no override.
// Layout: this file owns CLI startup and session selection; workbench.mjs
// owns shared routing, the command context and bound resident players.
// commands/combat.mjs (act/preview/board), commands/travel.mjs
// (go/map/loot/info/cancel/use), commands/choose.mjs (the default
// gesture), commands/search.mjs (research/plans/review/commit), and the
// render/contract/tokens modules carry their own behavior. Errors
// print as one friendly block (P_DEBUG=1 keeps the stack).
import { openRunPlay } from './client.mjs';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { printReading } from './render.mjs';
import { commandGroups, readCurrentSession, resolveInvocation, resolveNewSession, resolveSession, repository, freshRecordDirectory } from './session.mjs';
export { resolveSession, freshRecordDirectory };
import { info } from './commands/travel.mjs';
import { createWorkbenchContext, executeWorkbenchCommands, navigationArguments } from './workbench.mjs';
import { engineOutput, jumpTo } from './commands/navigation.mjs';

export * from './render.mjs';
export * from './contract.mjs';
export * from './tokens.mjs';

const binary = join(repository, 'target', 'dev-opt', 'run_play.exe');

function usage(code = 1) {
  const output = code === 0 ? process.stdout : process.stderr;
  output.write(
    'usage: node scripts/p.mjs                          reading (current session)\n' +
    '       node scripts/p.mjs <session> [command...]    exact name, unique fragment, or path\n' +
    '       node scripts/p.mjs <name> | <verb> <name> | <index>   choose what you see\n' +
    '       node scripts/p.mjs claim:BagOfPreparation#0  reward by name; reward rows reject numbers\n' +
    '       node scripts/p.mjs go [x,y]                  sole map exit, or coordinate\n' +
    '       node scripts/p.mjs act <actions...>         combat line (play:X potion:X end)\n' +
    '       node scripts/p.mjs preview <actions...>     try a line without moving the current position\n' +
    '       node scripts/p.mjs piles                    public unordered draw, discard and exhaust piles\n' +
    '       node scripts/p.mjs piles sl                 current exact combat piles (SL internal information)\n' +
    '       node scripts/p.mjs board | map [full|flip|set flip|normal|elites|route X Y] | loot | options | cancel\n' +
    '       node scripts/p.mjs read [brief|full] | back [steps | floor N | mark NAME | list]\n' +
    '       node scripts/p.mjs mark [NAME | drop NAME]   save/list/release exact return points\n' +
    '       node scripts/p.mjs info [power/enemy/card name | card <name> | M0 | player | powers | transition <HP> | help]\n' +
    '       node scripts/p.mjs undo | exe <args...> | use <session>\n' +
    '       node scripts/p.mjs player [status|next|accept [--continue]|replace [--continue] <command>|reject <guidance>|coach <guidance>]\n' +
    '       node scripts/p.mjs player inspect [STEP[.ATTEMPT] [input]] [--run RUN_DIRECTORY_NAME]\n' +
    '       node scripts/p.mjs observe [STEP [input] | live | list] [--home DSH_HOME] [--chat ID_FRAGMENT] [--turn N]\n' +
    '       node scripts/p.mjs jump [nNODE | source:hN | target:hN]   show a return point, or jump to one\n' +
    '       node scripts/p.mjs refresh <fresh-session>   verify old exact state into a new session and select it\n' +
    '       node scripts/p.mjs research [ms] [work transitions] | research <effort> [ms] | plans [full] | review <plan ID> [turn] | commit <plan ID> [expected effort]\n' +
    '       node scripts/p.mjs new <session> <seed> [--ascension N] [--goal heart-victory|act-three-victory]\n' +
    '       act/preview actions: spaces or comma-space; an explicit command starts a new group\n' +
    '       other command groups: comma-space, or \';\' to always start a new group\n');
  process.exit(code);
}

export async function main() {
  function runExecutable(cliArgs) {
    const result = spawnSync(binary, cliArgs, { cwd: repository, stdio: 'inherit', windowsHide: true });
    if (result.error) throw result.error;
    process.exit(result.status ?? 1);
  }

  const args = process.argv.slice(2);
  if (args.length === 1 && ['help', '--help', '-h'].includes(args[0])) usage(0);
  if (args[0] === 'observe') {
    const { observeNative } = await import('../dsh_player/native-inspection.mjs');
    const result = await observeNative({ repository, parameters: args.slice(1) });
    process.stdout.write(result.text);
    return;
  }
  const stateFile = join(repository, '.oracle-lab', 'run-play-records', 'current-session');

  if (args[0] === 'new') {
    const [session, seed, ...rest] = args.slice(1);
    if (!session || !/^\d+$/.test(seed ?? '')) usage();
    const known = ['--ascension', '--goal'];
    const resolvedSession = resolveNewSession(session);
    const cliArgs = ['new', '--session', resolvedSession, '--seed', seed];
    for (let i = 0; i < rest.length; i += 2) {
      if (!known.includes(rest[i]) || i + 1 >= rest.length) usage();
      cliArgs.push(rest[i], rest[i + 1]);
    }
    // Create quietly, then print the first decision in the SAME clean
    // reading format every other command uses. The raw executable listing
    // (key=value candidate dumps) is a machine surface; showing it here
    // invited output filtering that once cut option [0] off entirely.
    const created = spawnSync(binary, cliArgs, { cwd: repository, encoding: 'utf8', windowsHide: true });
    if (created.status !== 0) {
      process.stderr.write(created.stdout + created.stderr);
      process.exit(created.status ?? 1);
    }
    const sessionLabel = resolvedSession.split(/[\\/]/).pop() ?? 'session';
    const recordDirectory = freshRecordDirectory(repository, sessionLabel, 'new');
    const newGame = await openRunPlay({ repository, session: resolvedSession, recordDirectory });
    try {
      await printReading(await newGame.decision({ details: ['brief'] }));
    } finally {
      await newGame.close();
    }
    // A fresh run becomes the current one: the next bare node scripts/p.mjs or index
    // choice must land on it, not on whatever ran before.
    const { writeFile: writeState } = await import('node:fs/promises');
    await writeState(stateFile, resolvedSession).catch(() => {});
    process.exit(0);
  }

  const currentSession = await readCurrentSession();
  const { session, command, parameters } = resolveInvocation(args, currentSession);
  if (command === 'player') {
    if (currentSession !== session && parameters[0] !== 'inspect') {
      const { writeFile } = await import('node:fs/promises');
      await writeFile(stateFile, session);
    }
    const { controlPlayer } = await import('../dsh_player/launch.mjs');
    await controlPlayer({ repository, session, binary, parameters }, text => process.stdout.write(text + '\n'));
    return;
  }
  const commands = commandGroups(command, parameters);
  const { writeFile: writeFileAsync } = await import('node:fs/promises');
  if (command === 'refresh') {
    if (commands.length !== 1 || parameters.length !== 1) usage();
    const target = resolveNewSession(parameters[0]);
    const result = spawnSync(binary, ['reroot-current', '--session', session, '--target', target],
      { cwd: repository, encoding: 'utf8', windowsHide: true });
    if (result.error) throw result.error;
    if (result.status !== 0) {
      process.stderr.write(result.stdout + result.stderr);
      process.exit(result.status ?? 1);
    }
    const label = target.split(/[\\/]/).pop() ?? 'session';
    const recordDirectory = freshRecordDirectory(repository, label, 'refresh');
    const refreshed = await openRunPlay({ repository, session: target, recordDirectory });
    let reading;
    try {
      reading = await refreshed.decision({ details: ['brief'] });
    } finally {
      await refreshed.close();
    }
    await writeFileAsync(stateFile, target);
    process.stdout.write(`Verified current state in new session ${label}; source unchanged.\n`);
    await printReading(reading);
    return;
  }
  if (currentSession !== session) {
    await writeFileAsync(stateFile, session).catch(() => {});
  }

  // Listings and mark edits need no game connection. Restores return the same brief
  // reading as other commands instead of the executable's diagnostic dump.
  if ((command === 'back' && parameters[0] === 'list') || command === 'mark') {
    process.stdout.write(await engineOutput({ binary, session, repository },
      [command, ...navigationArguments(command, parameters)]));
    return;
  }
  if (command === 'scout') {
    // Retired at the owner's request: peeking into rooms before committing
    // changed the decisions themselves (spoilers), even though the SL
    // information level would allow it. The fork/probe implementation is
    // preserved in git history (dc33f509a^) for when a verdict-first mode
    // is wanted again.
    process.stderr.write('scout is banned: room recon counts as spoilers for the choosing mind; use map/info on public state instead\n');
    process.exit(1);
  }
  if (command === 'exe') {
    if (parameters.length < 1 || parameters.includes('--session')) usage();
    runExecutable([parameters[0], '--session', session, ...parameters.slice(1)]);
  }
  if (command === 'pick') {
    throw new Error('pick was retired — what you see is what you type: node scripts/p.mjs <name> (e.g. node scripts/p.mjs Disarm), node scripts/p.mjs <verb> <name>, or the row number');
  }

  // Info already runs the read-only executable directly. Opening an unused
  // agent-shell here adds a second process that may still be initializing
  // when EOF shutdown begins, turning a successful query into a timeout.
  if (commands.every(([name]) => name === 'info')) {
    for (const [, query] of commands) {
      await info({ binary, session, repository }, query);
    }
    return;
  }

  // Navigation can recover even from a terminal. Let Rust restore the retained
  // decision before opening an agent connection, which must see the new head.
  let appliedNavigation;
  if (commands[0]?.[0] === 'jump' && commands[0][1].length) {
    await jumpTo({ binary, session, repository }, commands[0][1][0]);
    appliedNavigation = `jump ${commands[0][1][0]}`;
  } else if (command === 'undo' || command === 'back') {
    await engineOutput({ binary, session, repository },
      [command, ...navigationArguments(command, parameters)]);
    appliedNavigation = [command, ...parameters].join(' ');
    commands[0] = ['read', []];
  }

  // A fresh record directory per connection keeps transcripts unmixed;
  // client.mjs creates it and refuses any path that already exists.
  const sessionLabel = session.split(/[\\/]/).pop() ?? 'session';
  const recordDirectory = freshRecordDirectory(repository, sessionLabel);

  let game;
  try {
    game = await openRunPlay({ repository, session, recordDirectory });
  } catch (error) {
    if (appliedNavigation) error.message = `Applied before error:\n  ${appliedNavigation}\nError: ${error.message}`;
    throw error;
  }
  const ctx = createWorkbenchContext({
    game, binary, session, repository, sessionLabel, stateFile, recordDirectory,
    join, openRunPlay, usage,
  });
  try {
    ctx.write(await executeWorkbenchCommands(ctx, commands));
  } catch (error) {
    if (appliedNavigation) error.message = `Applied before error:\n  ${appliedNavigation}\nError: ${error.message}`;
    throw error;
  } finally {
    await game.close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  await main();
}
