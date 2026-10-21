// Rendering the run-play readings and the map grid.
// Pure string shaping: no protocol or side effects.

// ---------------------------------------------------------------------------
// Map graph helpers use the typed public observation, which is available
// while choosing at a campfire or shop as well as on the map screen.
const MAP_TYPE_LETTER = new Map([
  ['Monster', 'M'], ['Elite', 'E'], ['Elite+key', 'K'], ['Rest', 'R'],
  ['Shop', 'S'], ['Treasure', 'T'], ['QuestionMark', '?'], ['Boss', 'B'],
]);
export const MAP_LETTER_ORDER = ['M', 'E', 'K', 'R', 'S', 'T', '?', 'B'];
export const MAP_LETTER_EN = new Map([
  ['M', 'fight'], ['E', 'elite'], ['K', 'key-elite'], ['R', 'rest'],
  ['S', 'shop'], ['T', 'chest'], ['?', 'unknown'], ['B', 'boss'],
]);

export function mapGraphFromObservation(observation) {
  const position = observation?.position;
  const visible = position?.kind === 'decision' ? position.observation : null;
  const map = visible?.domain === 'strategic'
    ? visible.observation?.public_map
    : visible?.domain === 'combat' && visible.run_context?.status === 'available'
      ? visible.run_context.public_map : null;
  if (!map || !Array.isArray(map.current) || !Array.isArray(map.nodes)) {
    throw new Error('current observation has no public map');
  }
  const roomTypes = new Map([
    ['question_mark', 'QuestionMark'], ['monster', 'Monster'],
    ['elite', 'Elite'], ['boss', 'Boss'], ['rest', 'Rest'],
    ['shop', 'Shop'], ['treasure', 'Treasure'], ['victory', 'Boss'],
  ]);
  const nodes = new Map();
  for (const [x, y, room, emeraldKey, destinations] of map.nodes) {
    const type = roomTypes.get(room);
    if (!type || !Array.isArray(destinations)) throw new Error(`unrecognized public map room: ${room}`);
    nodes.set(`${x},${y}`, {
      type: type === 'Elite' && emeraldKey ? 'Elite+key' : type,
      edges: destinations.map(([toX, toY]) => `${toX},${toY}`),
    });
  }
  return { nodes, current: map.current.join(','), boss: map.boss };
}

// Each cell carries its own edges so connections stay visible without a
// separate verbose listing: R(1,6)->(1,7) means that rest leads to (1,7).
//
// Direction: the game's own view puts large y on top (you walk upward);
// flip puts small y on top so forward travel reads top-to-bottom — the
// friendlier orientation for planning forward, and for the backward-from-
// boss habit strong players use. Rows that are structurally fixed by the
// map rules (all-rest boss approach, mid-act chest band) get a tag so the
// planner does not have to remember them.
export function mapRowTag(nodes, y) {
  const cells = [...nodes.entries()].filter(([key]) => key.split(',')[1] === String(y));
  if (cells.length === 0) return '';
  const types = new Set(cells.map(([, node]) => node.type));
  const bossRow = [...nodes.values()].some(node => node.type === 'Boss');
  const bossY = [...nodes.entries()].find(([, node]) => node.type === 'Boss')?.[0].split(',')[1];
  if (types.size === 1 && types.has('Rest') && bossRow && String(y + 1) === bossY) return ' pre-boss rest row';
  if (types.size === 1 && types.has('Treasure') && bossRow) return ' treasure row';
  return '';
}

export function renderMapGrid(nodes, current, { flipped = false } = {}) {
  const rows = [...new Set([...nodes.keys()].map(k => Number.parseInt(k.split(',')[1], 10)))]
    .sort((a, b) => (flipped ? a - b : b - a));
  const lines = [];
  for (const y of rows) {
    const cells = [...nodes.entries()]
      .filter(([key]) => key.split(',')[1] === String(y))
      .sort((a, b) => Number.parseInt(a[0].split(',')[0], 10) - Number.parseInt(b[0].split(',')[0], 10))
      .map(([key, node]) => {
        const letter = MAP_TYPE_LETTER.get(node.type) ?? node.type[0];
        const edges = node.edges.map(e => `(${e})`).join('/');
        const cell = `${letter}(${key})->${edges || '[end]'}`;
        return key === current ? `*${cell}` : cell;
      });
    lines.push(`y${String(y).padStart(2, '0')}${mapRowTag(nodes, y)} ${cells.join(' ')}`);
  }
  return lines.join('\n');
}

// Option rows carry the engine's own verb first (take Disarm, smith
// Bash); bracketing it keeps what-you-see matched to what-you-type.
const OPTION_VERBS = new Set(['take', 'buy', 'skip', 'claim', 'proceed', 'smith', 'leave', 'open', 'cancel', 'select', 'route', 'remove', 'use']);
export function annotateOptionVerbs(view) {
  return view.split('\n').map(line => {
    const match = /^(\d+: )([a-z]+) (.+)$/.exec(line);
    if (match && OPTION_VERBS.has(match[2])) return `${match[1]}[${match[2]}] ${match[3]}`;
    return line;
  }).join('\n');
}

export function namedOptionRows(view, commands) {
  return view.split('\n').map(line => {
    const match = /^(\d+): (.+)$/.exec(line);
    if (!match) return line;
    const command = commands[Number(match[1])];
    if (command === null) return line;
    if (!command) throw new Error(`no semantic command for option row ${match[1]}`);
    let label = match[2].replace(/^claim reward\[\d+\] /, '')
      .replace(/^open card reward #\d+$/, 'card reward');
    // The semantic command already binds the exact copy. Keep the native
    // deck-position selector out of its explanatory label.
    if (command.startsWith('smith:')) label = label.replace(/^smith (.+?)@deck\[\d+\]/, 'Upgrade one $1');
    return `./p ${command}  |  Effect: ${label}`;
  }).join('\n');
}

export function writeText(text) { return process.stdout.write(text); }

// Compare permanent inventory from the typed real root and private successor.
// The comparison itself reads no RNG and publishes no game action.
export function previewInventoryChanges(previous, endpoint) {
  const next = endpoint?.kind === 'decision' ? endpoint.observation : null;
  if (previous?.domain !== 'strategic' || next?.domain !== 'strategic') return '';
  const changes = [];
  for (const [label, field, name, count] of [
    ['Deck', 'cards', card => `${card.card}${card.upgrades ? `+${card.upgrades}` : ''}`, card => card.count],
    ['Relics', 'relics', relic => relic.relic, () => 1],
  ]) {
    if (!Array.isArray(previous.observation[field]) || !Array.isArray(next.observation[field])) continue;
    const totals = items => {
      const values = new Map();
      for (const item of items) values.set(name(item), (values.get(name(item)) ?? 0) + count(item));
      return values;
    };
    const old = totals(previous.observation[field]), current = totals(next.observation[field]);
    const differences = [...new Set([...old.keys(), ...current.keys()])].flatMap(item => {
      const delta = (current.get(item) ?? 0) - (old.get(item) ?? 0);
      return delta ? [`${delta > 0 ? '+' : ''}${delta} ${item}`] : [];
    });
    if (differences.length) changes.push(`${label}: ${differences.join(', ')}`);
  }
  return changes.length ? `Preview inventory changes:\n${changes.map(line => `  ${line}`).join('\n')}\n` : '';
}

export function printReading(reading, { optionCommands = null,
  expanded = false, combatCommands = [], selectionCommands = false, commandHeading = false, write = writeText } = {}) {
  // Rust groups identical copies; displayed indices still bind the full list.
  let rawView = reading.view;
  if (optionCommands !== null) rawView = namedOptionRows(rawView, optionCommands);
  if (commandHeading && optionCommands !== null) {
    rawView = rawView.replace(/^\.\/p /m, 'Commands (copy text before |):\n./p ');
  }
  const nextRooms = reading.raw?.reading?.options?.filter(option => option.candidate?.action?.kind === 'choose_route_node').length;
  if (nextRooms && optionCommands !== null) {
    rawView = rawView.replace(/^\.\/p go /m, `Next rooms: ${nextRooms}\n./p go `);
  }
  if (selectionCommands && reading.selections?.length === 1 && reading.selections[0].max === 1) {
    rawView = rawView.replace(/^  (\d+): (.+)$/gm, '  ./p $1  |  $2');
    if (reading.control === 'combat') {
      // Rust prints semantic combat selection actions. They use the act
      // executor, not the ordinary option-name resolver.
      rawView = rawView.replace(/^  (select:[^\n]+?)  \|  (.+)$/gm, '  ./p act $1  |  $2');
    }
  }
  if (selectionCommands && reading.selections?.length) {
    // Both combat and master-deck multi-selections use the semantic act
    // executor. Rust indents Input under the selection heading.
    rawView = rawView.replace(/^([ \t]*)Input: select:(.+)$/gm, '$1Input: ./p act select:$2');
    if (reading.selections.some(selection => selection.max > 1)) {
      rawView = rawView.replace(/^([ \t]*Input: \.\/p act select:.+)$/gm,
        '$1\n  Select distinct listed card tokens in this one selection; #0 and #1 identify different copies, not the same #0 twice.');
    }
    rawView = rawView.replace(/^([ \t]*)select:none  \|  (.+)$/gm, '$1./p act select:none  |  $2');
  }
  const view = reading.control === 'combat' ? rawView : annotateOptionVerbs(rawView);
  write(`${view.trimEnd()}\n`);
  if (combatCommands.length > 0) {
    write(expanded ? 'Legal commands (current position):\n'
      : 'Playable now (equivalent copies grouped; options lists all):\n');
    for (const command of combatCommands) write(`  ./p act ${command}\n`);
  }
}

// Skin-layer spelling for potions: `potion:Essence` resolves against the
// decision JSON contract — stable ids in the observation's potion slots
// and the ready-made use-potion semantic strings. The name ladder matches
// pick's: exact, then de-spaced contains, then a unique substring; real
// ambiguity is refused with the candidate list. Identical potions in
// several slots are not ambiguous (same effect). A trailing `!`
