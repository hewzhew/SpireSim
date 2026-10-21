import test from 'node:test';
import assert from 'node:assert/strict';
import { createHostLocale } from './client/test-support.mjs';
import { mapNamespace, gameNamespace, mapDictionaries, gameDictionaries, mapTokenTitle, validateHostDictionaries } from './client/locale.mjs';
import { renderMapPicture } from './map-layout.mjs';

test('map dictionaries use the host parameter format and complete bilingual keys', () => {
  validateHostDictionaries(mapDictionaries, 'Map');
  const missing = structuredClone(mapDictionaries); delete missing.zh.currentRoom;
  assert.throws(() => validateHostDictionaries(missing, 'Map'), /keys differ/);
  const wrong = structuredClone(mapDictionaries); wrong.zh.act = '第 {{act}} 幕';
  assert.throws(() => validateHostDictionaries(wrong, 'Map'), /Invalid host translation/);
  wrong.zh.act = '第 {floor} 幕';
  assert.throws(() => validateHostDictionaries(wrong, 'Map'), /parameters differ/);
});

test('published dsh locale switches map names and full tooltip sentences over unchanged geometry', async () => {
  const locale = await createHostLocale('en');
  const disposeMap = locale.register(mapNamespace, mapDictionaries), disposeGame = locale.register(gameNamespace, gameDictionaries);
  const t = locale.bind(mapNamespace), game = locale.bind(gameNamespace);
  const view = renderMapPicture(new Map([
    ['0,0', { type: 'Elite+key', edges: ['1,1'] }], ['1,1', { type: 'Rest', edges: ['0,2'] }],
    ['0,2', { type: 'Boss', edges: [] }],
  ]), '0,0');
  const before = JSON.stringify(view);
  const title = (room, boss = 'Hexaghost') => mapTokenTitle(view.lines.flat().find(token => token.room === room), boss, t, game);
  assert.equal(title('elite'), 'Current location: Elite (0,0), holds the Emerald Key');
  assert.equal(title('rest'), 'Rest (1,1)');
  assert.equal(title('boss'), 'Hexaghost');
  assert.equal(t('act', { act: 2 }), 'Act 2');
  let changes = 0;
  const unsubscribe = locale.subscribe(() => { changes++; });
  locale.setLocale('zh');
  assert.equal(changes, 1);
  assert.equal(title('elite'), '当前位置：精英 (0,0)，携带绿钥匙');
  assert.equal(title('rest'), '休息 (1,1)');
  assert.equal(title('boss'), '六火亡魂');
  assert.equal(title('boss', null), '首领尚未公开');
  assert.equal(title('boss', 'FutureBoss'), 'FutureBoss');
  assert.equal(t('act', { act: 2 }), '第 2 幕');
  assert.equal(JSON.stringify(view), before);
  unsubscribe(); disposeGame(); disposeMap();
  assert.equal(t('title'), 'title');
});
