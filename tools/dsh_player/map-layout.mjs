// A human map, using the three-column rooms and one connector row seen in
// sts_map_oracle's format_map. Draw the supplied public graph, never a seed.
const letters = { Monster: 'M', Elite: 'E', 'Elite+key': 'E', Rest: 'R', Shop: '$', Treasure: 'T', QuestionMark: '?', Boss: 'B' };
const names = { Monster: '敌人', Elite: '精英', 'Elite+key': '精英', Rest: '休息', Shop: '商人', Treasure: '财宝', QuestionMark: '不明', Boss: '首领' };
const point = key => key.split(',').map(Number);

export function renderMapPicture(nodes, current, { bossTitle } = {}) {
  if (!nodes.size) return { picture: '', lines: [], extraConnections: [] };
  const currentY = nodes.has(current) ? point(current)[1] : -1;
  const reachable = new Set(), pending = nodes.has(current) ? [current] : [];
  while (pending.length) {
    const key = pending.pop();
    if (reachable.has(key)) continue;
    if (!nodes.has(key)) throw new Error(`map edge has no published destination: ${key}`);
    reachable.add(key); pending.push(...nodes.get(key).edges);
  }
  const next = new Set(nodes.get(current)?.edges ?? []);
  const stateOf = key => key === current ? 'current' : point(key)[1] < currentY ? 'past'
    : !nodes.has(current) ? 'normal' : next.has(key) ? 'next' : reachable.has(key) ? 'reachable' : 'unreachable';
  const topY = Math.max(...[...nodes.keys()].map(key => point(key)[1]));
  const top = [...nodes].filter(([key]) => point(key)[1] === topY);
  const boss = top.length === 1 && top[0][1].type === 'Boss' ? top[0][0] : null;
  const rooms = [...nodes].filter(([key]) => key !== boss);
  const maxX = Math.max(0, ...rooms.map(([key]) => point(key)[0]));
  const maxY = Math.max(-1, ...rooms.map(([key]) => point(key)[1]));
  const minY = rooms.length ? Math.min(...rooms.map(([key]) => point(key)[1])) : 0;
  const width = (maxX + 1) * 3, rows = [], extraConnections = [];
  const empty = () => Array.from({ length: width }, () => ({ text: ' ', kind: 'space' }));
  const roomAt = (row, x, key) => {
    const node = nodes.get(key), state = stateOf(key);
    const emerald = node.type === 'Elite+key';
    const title = node.type === 'Boss' ? bossTitle ?? names.Boss : `${names[node.type] ?? node.type} (${key})`;
    row[x] = {
      text: letters[node.type] ?? '?', kind: 'room', state, emerald,
      title: `${state === 'current' ? '当前位置：' : ''}${title}${emerald ? '，携带绿钥匙' : ''}`,
    };
  };
  const edgeAt = (row, x, glyph, from, to) => {
    const state = stateOf(from);
    const old = row[x];
    const dim = value => ['past', 'unreachable'].includes(value);
    const text = old.kind === 'edge' && (old.text === '+' || glyph === '+') ? '+' : glyph;
    if (old.kind === 'edge' && !dim(old.state) && dim(state)) { row[x] = { ...old, text }; return; }
    row[x] = { text, kind: 'edge', state, title: `(${from}) -> (${to})` };
  };
  // Boss coordinates in the engine are a sentinel. A shared rail shows exactly
  // which last-row rooms lead to it, with the Boss centered over the map.
  if (boss) {
    const rail = empty(), stems = empty(), bossX = Math.round(maxX / 2) * 3 + 1;
    for (const [key, node] of rooms) if (point(key)[1] === maxY && node.edges.includes(boss)) {
      const x = point(key)[0] * 3 + 1;
      for (let column = Math.min(x, bossX); column <= Math.max(x, bossX); column++) edgeAt(rail, column, '-', key, boss);
      edgeAt(rail, x, '+', key, boss); edgeAt(stems, x, '|', key, boss);
    }
    roomAt(rail, bossX, boss);
    rows.push({ label: '', cells: rail });
    if (maxY >= 0) rows.push({ label: '', cells: stems });
  }
  for (let y = maxY; y >= minY; y--) {
    const row = empty();
    for (const [key] of rooms) if (point(key)[1] === y) roomAt(row, point(key)[0] * 3 + 1, key);
    rows.push({ label: String(y), cells: row, current: y === currentY });
    if (y === minY) continue;
    const connectors = empty();
    for (const [key, node] of rooms) if (point(key)[1] === y - 1) for (const target of node.edges) {
      if (!nodes.has(target)) throw new Error(`map edge has no published destination: ${key} -> ${target}`);
      if (target === boss) continue;
      const dx = point(target)[0] - point(key)[0];
      if (point(target)[1] !== y || Math.abs(dx) > 1) continue;
      edgeAt(connectors, point(key)[0] * 3 + 1 + dx, dx === 0 ? '|' : dx > 0 ? '/' : '\\', key, target);
    }
    rows.push({ label: '', cells: connectors });
  }
  for (const [from, node] of nodes) for (const to of node.edges) {
    if (!nodes.has(to)) throw new Error(`map edge has no published destination: ${from} -> ${to}`);
    if (to === boss && point(from)[1] === maxY) continue;
    if (point(to)[1] !== point(from)[1] + 1 || Math.abs(point(to)[0] - point(from)[0]) > 1) {
      extraConnections.push(`${letters[node.type]}(${from}) -> ${letters[nodes.get(to).type]}(${to})`);
    }
  }
  rows.push({ label: 'x', cells: Array.from({ length: width }, (_, x) => ({
    text: x % 3 === 1 ? String((x - 1) / 3) : ' ', kind: 'axis',
  })) });
  const lines = rows.map(row => {
    const cells = [...row.cells];
    while (cells.at(-1)?.text === ' ') cells.pop();
    const tokens = [{ text: row.label.padStart(2, ' ') + '   ', kind: 'axis', state: row.current ? 'current' : 'normal' }];
    for (const cell of cells) {
      const last = tokens.at(-1);
      if (last.kind === cell.kind && last.state === cell.state && last.title === cell.title && last.emerald === cell.emerald) last.text += cell.text;
      else tokens.push({ ...cell });
    }
    return tokens;
  });
  return { picture: lines.map(tokens => tokens.map(token => token.text).join('')).join('\n'), lines, extraConnections };
}
