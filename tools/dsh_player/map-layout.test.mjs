import test from 'node:test';
import assert from 'node:assert/strict';
import { renderMapPicture } from './map-layout.mjs';

const graph = () => new Map([
  ['0,0', { type: 'Monster', edges: ['1,1'] }],
  ['2,0', { type: 'Monster', edges: ['1,1', '2,1'] }],
  ['1,1', { type: 'Rest', edges: ['0,2', '2,2'] }],
  ['2,1', { type: 'Shop', edges: ['2,2'] }],
  ['0,2', { type: 'Elite+key', edges: ['0,3'] }],
  ['2,2', { type: 'Rest', edges: ['0,3'] }],
  ['0,3', { type: 'Boss', edges: [] }],
]);

test('compact diagram preserves every ordinary edge and Boss approach without raster staircases', () => {
  const nodes = graph(), before = JSON.stringify([...nodes]);
  const view = renderMapPicture(nodes, '0,0');
  assert.equal(view.picture, view.lines.map(row => row.map(token => token.text).join('')).join('\n'));
  assert.equal(view.lines.length, 8);
  assert.deepEqual(view.extraConnections, []);
  const shown = new Set(view.lines.flat().filter(token => token.kind === 'edge').map(token => token.title));
  const expected = new Set([...nodes].flatMap(([from, node]) => node.edges.map(to => `(${from}) -> (${to})`)));
  assert.deepEqual(shown, expected);
  assert.doesNotMatch(view.lines.slice(2).flat().map(token => token.text).join(''), /[+-]/);
  assert.match(view.picture, /\$/);
  assert.equal(JSON.stringify([...nodes]), before);
});

test('state shading follows actual reachability and does not invent the path through passed floors', () => {
  const view = renderMapPicture(graph(), '1,1');
  const rooms = view.lines.flat().filter(token => token.kind === 'room');
  const state = key => rooms.find(token => key === '0,3' ? token.text === 'B' : token.title.includes(`(${key})`)).state;
  assert.equal(state('1,1'), 'current');
  assert.equal(state('0,2'), 'next'); assert.equal(state('2,2'), 'next');
  assert.equal(state('0,3'), 'reachable');
  assert.equal(state('2,1'), 'unreachable');
  assert.equal(state('0,0'), 'past'); assert.equal(state('2,0'), 'past');
  assert.ok(rooms.every(token => token.text.length === 1));
  assert.equal(rooms.find(token => token.emerald).text, 'E');
  assert.match(rooms.find(token => token.emerald).title, /携带绿钥匙/);
  assert.doesNotMatch(view.picture, /[@K]/);
  assert.equal(renderMapPicture(graph(), '-1,-1').lines.flat().filter(token => token.state === 'current').length, 0);
});

test('non-adjacent public connections are explicit instead of becoming false nearby edges', () => {
  const nodes = graph(); nodes.get('0,0').edges = ['2,2'];
  const view = renderMapPicture(nodes, '0,0');
  assert.deepEqual(view.extraConnections, ['M(0,0) -> R(2,2)']);
  assert.equal(view.lines.flat().find(token => token.title === '休息 (2,2)').state, 'next');
  nodes.get('0,0').edges = ['5,5'];
  assert.throws(() => renderMapPicture(nodes, '0,0'), /no published destination/);
});

test('a partial public map does not add blank unpublished floors', () => {
  const view = renderMapPicture(new Map([
    ['1,5', { type: 'Rest', edges: ['1,6'] }], ['1,6', { type: 'Boss', edges: [] }],
  ]), '1,5');
  assert.deepEqual(view.lines.map(row => row[0].text.trim()).filter(Boolean), ['5', 'x']);
});
