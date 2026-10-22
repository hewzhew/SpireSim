// Rendering the run-play readings and the map grid.
// Pure string shaping: no protocol or side effects.

// ---------------------------------------------------------------------------
// Map graph helpers use the typed public observation, which is available
// while choosing at a campfire or shop as well as on the map screen.

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

export function printReading(reading, {
  expanded = false, combatCommands = [], write = writeText } = {}) {
  // The source renderer owns choice commands and their presentation. Leave
  // localized labels and hypothetical preview rows exactly as received.
  write(`${reading.view.trimEnd()}\n`);
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
