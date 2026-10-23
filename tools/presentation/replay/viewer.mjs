import { createPresentation, preferredLocale } from '../index.mjs';
import { neowOptionRuns } from './neow-option.mjs';

async function main() {
  const data = JSON.parse(document.getElementById('data').textContent);
  const story = JSON.parse(document.getElementById('story').textContent);
  const { frames } = data, { scenes } = story;
  let savedLocale;
  try { savedLocale = localStorage.getItem('spiresim.demo.locale'); } catch {}
  const display = await createPresentation({ locale: savedLocale || preferredLocale(navigator.languages), onMissing: gap => console.warn('Missing display name', gap) });
  const t = (key, params) => display.text('replay:' + key, params);
  const name = (kind, id) => display.entity(kind, id);
  const number = value => display.number(value);
  let selected = Math.max(0, scenes.findIndex(scene => scene.id === location.hash.slice(1)));
  if (!location.hash) selected = 1;
  let rawTab = 'calls';
  let turnRecordOpen = false;
  const byId = id => document.getElementById(id);
  const node = (tag, text, className) => {
    const el = document.createElement(tag);
    if (text !== undefined) el.textContent = text;
    if (className) el.className = className;
    return el;
  };
  function icon(name) {
    const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    for (const [key, value] of Object.entries({ viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': '1.6', 'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true', class: 'icon' })) svg.setAttribute(key, value);
    const use = document.createElementNS('http://www.w3.org/2000/svg', 'use'); use.setAttribute('href', '#i-' + name); svg.append(use);
    return svg;
  }
  const cardIcon = type => ({ attack: 'swords', skill: 'shield', curse: 'lock' })[type];
  const effectKeys = { damage: 'damage', block: 'block', apply: 'apply', draw: 'draw', unplayable: 'unplayable', ethereal: 'ethereal', double_block: 'doubleBlock', random_damage: 'randomDamage' };
  function gameCard(card, chosen = false) {
    const el = node('article', undefined, 'playing-card ' + card.type + (chosen ? ' chosen' : ''));
    el.dataset.card = card.card_id;
    const top = node('div', undefined, 'card-topline');
    const cost = node('span', card.cost === null ? undefined : number(card.cost), 'card-cost');
    if (card.cost === null) cost.append(icon('lock'));
    top.append(cost, node('h3', name('card', card.card_id), 'card-name'));
    const art = node('div', undefined, 'card-symbol'); art.append(icon(cardIcon(card.type)));
    const rules = node('div', undefined, 'card-rule');
    for (const effect of card.effects) {
      const params = { ...effect };
      if (effect.power_id) params.power = name('power', effect.power_id);
      rules.append(node('div', display.text('game:effect.' + effectKeys[effect.kind], params)));
    }
    el.append(top, art, node('div', name('cardType', card.type), 'card-type'), rules);
    if (chosen) { const mark = node('div', t('reward.chosen'), 'chosen-mark'); mark.prepend(icon('check')); el.append(mark); }
    return el;
  }
  function fighter(label, hp, max, enemy, details, intent) {
    const el = node('article', undefined, 'fighter ' + (enemy ? 'fighter-enemy' : 'fighter-player'));
    el.setAttribute('aria-label', label);
    const intentRow = node('div', undefined, 'fighter-intent');
    if (intent) intentRow.append(iconText(intent.kind === 'defend_buff' ? 'shield' : 'swords', intentLabel(intent), 'intent'));
    else intentRow.setAttribute('aria-hidden', 'true');
    const figure = node('div', undefined, 'fighter-figure'); figure.setAttribute('aria-hidden', 'true'); figure.append(icon(enemy ? 'bug' : 'swords'));
    const health = node('div', undefined, 'fighter-health');
    const fill = node('span', undefined, 'fighter-health-fill'); fill.style.width = (100 * hp / max) + '%';
    health.append(fill, node('span', number(hp) + ' / ' + number(max), 'fighter-health-value'));
    const statuses = node('div', undefined, 'fighter-statuses'); statuses.append(...details);
    el.append(intentRow, figure, node('div', label, 'fighter-name'), health, statuses);
    return el;
  }
  function iconText(iconName, text, className) { const el = node('span', text, className); el.prepend(icon(iconName)); return el; }
  function intentLabel(intent) {
    const key = { attack: 'attack', attack_defend: 'attackDefend', defend_buff: 'defendBuff' }[intent.kind];
    return t('combat.' + key, intent);
  }
  function renderCombat(scene) {
    const { before, after } = scene;
    const board = node('section', undefined, 'combat-board'); board.setAttribute('aria-label', title(scene));
    const hud = node('div', undefined, 'combat-hud'); hud.append(resources(before));
    const field = node('div', undefined, 'battlefield');
    const enemyDetails = [];
    if (before.enemy.block) enemyDetails.push(iconText('shield', t('combat.block', { block: before.enemy.block })));
    for (const power of before.enemy.powers) enemyDetails.push(node('span', t('combat.power', { power: name('power', power.power_id), amount: power.amount }), 'status'));
    field.append(fighter(name('character', story.character_id), before.hp, before.max_hp, false, []), fighter(name('enemy', before.enemy.enemy_id), before.enemy.hp, before.enemy.max_hp, true, enemyDetails, before.enemy.intent));
    const dock = node('div', undefined, 'combat-dock');
    const energy = node('div', undefined, 'energy-reserve'); energy.setAttribute('aria-label', t('combat.energy', { energy: before.energy }));
    energy.append(node('strong', number(before.energy), 'energy-orb'), node('span', t('combat.energyLabel'), 'energy-label'));
    const hand = node('div', undefined, 'hand'); for (const card of before.hand) hand.append(gameCard(card));
    hand.setAttribute('role', 'group'); hand.setAttribute('aria-label', t('combat.hand'));
    dock.append(energy, hand); board.append(hud, field, dock);
    const record = node('details', undefined, 'turn-record'); record.open = turnRecordOpen;
    record.addEventListener('toggle', () => { turnRecordOpen = record.open; });
    const recordTitle = node('summary', t('combat.plays')); recordTitle.prepend(icon('chevron-right')); record.append(recordTitle);
    const resolution = node('div', undefined, 'resolution');
    const actions = node('div', undefined, 'action-sequence');
    scene.plays.forEach((id, index) => {
      if (index) actions.append(icon('chevron-right'));
      const card = before.hand.find(card => card.card_id === id);
      const chip = node('span', name('card', id), 'action-chip'); chip.dataset.card = id; chip.prepend(icon(cardIcon(card.type))); actions.append(chip);
    });
    const result = node('div', undefined, 'turn-result');
    if (after.enemy) {
      const player = node('span', name('character', story.character_id)); player.append(node('strong', number(before.hp) + ' → ' + number(after.hp), after.hp < before.hp ? 'loss' : ''));
      const enemy = node('span', name('enemy', before.enemy.enemy_id)); enemy.append(node('strong', number(before.enemy.hp) + ' → ' + number(after.enemy.hp))); result.append(player, enemy);
    } else {
      result.append(node('span', t('combat.defeated', { enemy: name('enemy', before.enemy.enemy_id) })), iconText('flame', t('combat.heal', { amount: after.hp - before.hp })));
    }
    resolution.append(actions, result); record.append(resolution); byId('stage').append(board, record);
  }
  function resources(state) {
    const row = node('div', undefined, 'resource-line');
    row.append(iconText('heart', number(state.hp) + ' / ' + number(state.max_hp)), iconText('coin', number(state.gold)), ...story.relic_ids.map(id => iconText('flame', name('relic', id))));
    return row;
  }
  function renderEvent(scene) {
    const { before, after } = scene;
    const layout = node('div', undefined, 'event-layout'); layout.append(resources(before));
    const options = node('ol', undefined, 'event-options');
    for (const entry of scene.options) {
      const option = node('li', undefined, 'event-option' + (scene.selected_option === entry.index ? ' selected' : ''));
      const label = node('span', undefined, 'option-label');
      for (const run of neowOptionRuns(entry.effects, display.locale)) {
        label.append(node('span', run.text, 'option-text-' + run.tone));
      }
      option.append(label);
      if (scene.selected_option === entry.index) {
        const mark = node('span', undefined, 'option-check'); mark.setAttribute('aria-label', t('event.selected')); mark.append(icon('check')); option.append(mark);
      }
      options.append(option);
    }
    const result = node('div', undefined, 'resource-line'); result.append(iconText('heart', number(before.hp) + ' / ' + number(before.max_hp) + ' → ' + number(after.hp) + ' / ' + number(after.max_hp)));
    layout.append(options, result); byId('stage').append(layout);
  }
  function renderRewards(scene) {
    const layout = node('div', undefined, 'event-layout'); layout.append(resources(scene.after));
    const cards = node('div', undefined, 'reward-cards'); for (const card of scene.cards) cards.append(gameCard(card, card.card_id === scene.chosen_card_id));
    layout.append(cards); byId('stage').append(layout);
  }
  const messageText = message => (message.content ?? []).map(block => block.text ?? JSON.stringify(block)).join('\n');
  const appendPre = (parent, text) => parent.append(node('pre', text));
  function renderRaw(scene) {
    const open = new Set([...byId('raw-record').querySelectorAll('details[open][data-record-key]')].map(el => el.dataset.recordKey));
    function detail(key, title) { const el = node('details'); el.dataset.recordKey = key; el.open = open.has(key); el.append(node('summary', title)); return el; }
    for (const id of ['calls', 'input', 'output']) { byId('raw-' + id).replaceChildren(); byId('raw-' + id).hidden = id !== rawTab; }
    for (const button of document.querySelectorAll('[data-tab]')) { button.setAttribute('aria-selected', String(button.dataset.tab === rawTab)); button.tabIndex = button.dataset.tab === rawTab ? 0 : -1; }
    for (const index of scene.indices) {
      const frame = frames[index];
      for (const call of frame.calls) {
        const section = node('section', undefined, call.outcome === 'error' ? 'raw-error' : ''); section.append(node('h3', call.command)); appendPre(section, call.text); byId('raw-calls').append(section);
      }
      for (const captured of frame.inputs ?? []) {
        const key = 'request-' + frame.step + '-' + captured.attempt;
        const request = detail(key, t('raw.request', { step: frame.step }));
        for (const message of captured.request.messages) { request.append(node('h3', t('raw.' + message.role))); appendPre(request, messageText(message)); }
        const original = detail(key + '-json', t('raw.json')); appendPre(original, JSON.stringify(captured, null, 2)); request.append(original); byId('raw-input').append(request);
      }
      const output = frame.messages.flatMap(message => message.blocks).filter(block => block.text?.trim());
      if (output.length) { byId('raw-output').append(node('h3', t('raw.request', { step: frame.step }))); for (const block of output) appendPre(byId('raw-output'), block.text); }
    }
    if (!byId('raw-output').children.length) byId('raw-output').append(node('p', t('raw.noText'), 'text-secondary small'));
  }
  function title(scene) { return scene.kind === 'event' ? name('event', scene.event_id) : scene.kind === 'combat' ? name('encounter', scene.encounter_id) : t('scene.rewards'); }
  function label(scene) { return scene.kind === 'event' ? t('scene.start') : scene.kind === 'combat' ? t('scene.turn', { turn: scene.turn }) : t('scene.floor', { floor: scene.floor }); }
  function renderNavigation() {
    byId('timeline').replaceChildren();
    scenes.forEach((scene, index) => {
      const button = node('button', undefined, 'scene-link'); button.type = 'button'; button.dataset.scene = scene.id;
      const text = node('span', undefined, 'scene-text'); text.append(node('strong', scene.kind === 'combat' ? label(scene) : title(scene)), node('small', scene.kind === 'event' ? t('scene.initialReward') : scene.kind === 'combat' ? title(scene) : name('card', scene.chosen_card_id)));
      button.append(node('span', String(index + 1).padStart(2, '0'), 'scene-index'), text); button.addEventListener('click', () => select(index)); byId('timeline').append(button);
    });
  }
  function renderChrome() {
    document.documentElement.lang = display.locale;
    byId('language').value = display.locale;
    for (const el of document.querySelectorAll('[data-message]')) el.textContent = t(el.dataset.message);
    for (const el of document.querySelectorAll('[data-aria]')) el.setAttribute('aria-label', t(el.dataset.aria));
    for (const el of document.querySelectorAll('[data-character]')) el.textContent = name('character', story.character_id);
    for (const el of document.querySelectorAll('[data-act]')) el.textContent = t('scene.act', { act: story.act });
    byId('raw-summary').replaceChildren(icon('chevron-right'), t('raw.summary'));
  }
  function render() {
    const scene = scenes[selected]; byId('heading').textContent = title(scene); byId('turn-label').textContent = label(scene);
    byId('previous').disabled = selected === 0; byId('next').disabled = selected === scenes.length - 1;
    for (const [index, button] of [...byId('timeline').children].entries()) { if (index === selected) button.setAttribute('aria-current', 'step'); else button.removeAttribute('aria-current'); }
    byId('stage').replaceChildren();
    if (scene.kind === 'combat') renderCombat(scene); else if (scene.kind === 'event') renderEvent(scene); else renderRewards(scene);
    renderRaw(scene);
  }
  function select(index, updateHash = true) {
    if (index < 0 || index >= scenes.length) return;
    selected = index; turnRecordOpen = false; byId('raw-record').open = false; render();
    if (updateHash) history.replaceState(null, '', '#' + scenes[selected].id);
  }
  byId('brand-mark').append(icon('cards')); byId('run-icon').append(icon('swords')); byId('crumb-arrow').append(icon('chevron-right'));
  byId('previous').append(icon('chevron-left')); byId('next').append(icon('chevron-right'));
  byId('previous').addEventListener('click', () => select(selected - 1)); byId('next').addEventListener('click', () => select(selected + 1));
  byId('language').addEventListener('change', async event => {
    const selector = event.currentTarget; selector.disabled = true;
    try {
      await display.setLocale(selector.value); renderChrome(); renderNavigation(); render();
      try { localStorage.setItem('spiresim.demo.locale', display.locale); } catch {}
    } finally { selector.disabled = false; }
  });
  for (const button of document.querySelectorAll('[data-tab]')) {
    button.addEventListener('click', () => { rawTab = button.dataset.tab; renderRaw(scenes[selected]); });
    button.addEventListener('keydown', event => {
      const names = ['calls', 'input', 'output'], offset = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
      if (!offset) return; event.preventDefault(); rawTab = names[(names.indexOf(rawTab) + offset + names.length) % names.length]; renderRaw(scenes[selected]); byId('tab-' + rawTab).focus();
    });
  }
  window.addEventListener('hashchange', () => { const index = scenes.findIndex(scene => scene.id === location.hash.slice(1)); if (index >= 0) select(index, false); });
  renderChrome(); renderNavigation(); render();
}
main().catch(error => console.error(error));
