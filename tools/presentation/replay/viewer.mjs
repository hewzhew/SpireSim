import { createPresentation, preferredLocale } from '../index.mjs';
import { neowOptionRuns, neowStructuredRuns } from './neow-option.mjs';
import { intentView } from './intent-view.mjs';
import { handTransition, sequenceView } from './sequence.mjs';
import { recordedJourney, replayMap } from './journey.mjs';

async function main() {
  const data = JSON.parse(document.getElementById('data').textContent);
  const story = JSON.parse(document.getElementById('story').textContent);
  const { frames } = data, { scenes } = story;
  const journeys = recordedJourney(story);
  let savedLocale;
  try { savedLocale = localStorage.getItem('spiresim.demo.locale'); } catch {}
  const display = await createPresentation({ locale: savedLocale || preferredLocale(navigator.languages), onMissing: gap => console.warn('Missing display name', gap) });
  const t = (key, params) => display.text('replay:' + key, params);
  const name = (kind, id) => display.entity(kind, id);
  const number = value => display.number(value);
  let selected = Math.max(0, scenes.findIndex(scene => scene.id === location.hash.slice(1)));
  if (!location.hash) {
    const model = scenes.findIndex(scene => scene.kind === 'sequence' && scene.provenance.operator === 'model');
    selected = model >= 0 ? model : Math.min(1, scenes.length - 1);
  }
  let rawTab = 'calls';
  let rawRecordKey;
  let turnRecordOpen = true;
  function locationCursor(index) {
    const raw = new URLSearchParams(location.search).get('step');
    const step = raw && /^\d+$/.test(raw) ? Number(raw) : 0;
    return scenes[index]?.kind === 'sequence' && Number.isSafeInteger(step)
      ? Math.min(step, scenes[index].snapshots.length - 1) : 0;
  }
  let actionCursor = locationCursor(selected);
  let sequenceDetailsOpen = false;
  let historyObserver;
  let refreshHistory = () => {};
  let scrollMapToCurrent = () => {};
  let mapCollapsed = null;
  try {
    const saved = localStorage.getItem('spiresim.demo.mapCollapsed');
    if (saved === 'true' || saved === 'false') mapCollapsed = saved === 'true';
  } catch {}
  let mapSelection = null;
  let mapExpanded = false;
  let mapMode = 'hidden';
  let mapSignature;
  let mapFocusKey;
  let mapPositionFrame;
  let mapAvailable = false;
  const mapMotions = new Set();
  const phaseMotions = new Set();
  const activeMotion = new Set();
  const openPiles = new Set();
  const byId = id => document.getElementById(id);
  const mapPanel = byId('journey-map');
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
  const cardIcon = type => ({ attack: 'swords', skill: 'shield', curse: 'lock', status: 'lock', power: 'bolt' })[type];
  const cardFrame = {
    attack: 'M4 4H96V73L50 96L4 73Z',
    skill: 'M4 4H96V72Q96 96 50 96Q4 96 4 72Z',
    curse: 'M14 4H86L96 16V84L86 96H14L4 84V16Z',
    status: 'M14 4H86L96 16V84L86 96H14L4 84V16Z',
    power: 'M4 35Q4 4 50 4Q96 4 96 35V96H4Z',
  };
  const effectKeys = { damage: 'damage', block: 'block', apply: 'apply', draw: 'draw', unplayable: 'unplayable', ethereal: 'ethereal', double_block: 'doubleBlock', random_damage: 'randomDamage' };
  function gameCard(card, chosen = false) {
    const el = node('article', undefined, 'playing-card ' + card.type + (chosen ? ' chosen' : ''));
    el.dataset.card = card.card_id;
    const top = node('div', undefined, 'card-topline');
    const cost = node('span', card.cost === null ? undefined : card.cost === 'X' ? 'X' : number(card.cost), 'card-cost');
    if (card.cost === null) cost.append(icon('lock'));
    top.append(cost, node('h3', display.cardName(card.card_id, card.upgrades ?? 0), 'card-name'));
    const portrait = node('div', undefined, 'card-portrait');
    const frame = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
    for (const [key, value] of Object.entries({ viewBox: '0 0 100 100', preserveAspectRatio: 'none', 'aria-hidden': 'true', class: 'card-frame' })) frame.setAttribute(key, value);
    const outline = document.createElementNS('http://www.w3.org/2000/svg', 'path'); outline.setAttribute('d', cardFrame[card.type]); frame.append(outline);
    const art = node('div', undefined, 'card-symbol'); art.append(icon(cardIcon(card.type)));
    portrait.append(frame, art, node('div', name('cardType', card.type), 'card-type'));
    const rules = node('div', undefined, 'card-rule');
    for (const line of card.printed_lines?.[display.locale] ?? []) rules.append(node('div', line));
    for (const effect of card.effects ?? []) {
      const params = { ...effect };
      if (effect.power_id) params.power = name('power', effect.power_id);
      rules.append(node('div', display.text('game:effect.' + effectKeys[effect.kind], params)));
    }
    el.append(top, portrait, rules);
    if (chosen) { const mark = node('div', t('reward.chosen'), 'chosen-mark'); mark.prepend(icon('check')); el.append(mark); }
    return el;
  }
  function fighter(label, hp, max, { enemy = false, block, details = [], intent, figureIcon } = {}) {
    const el = node('article', undefined, 'fighter ' + (enemy ? 'fighter-enemy' : 'fighter-player'));
    el.setAttribute('aria-label', label);
    const intentRow = node('div', undefined, 'fighter-intent');
    if (enemy) intentRow.append(renderIntent(intent));
    else intentRow.setAttribute('aria-hidden', 'true');
    const figure = node('div', undefined, 'fighter-figure'); figure.setAttribute('aria-hidden', 'true'); figure.append(icon(figureIcon ?? (enemy ? 'bug' : 'swords')));
    const vitals = node('div', undefined, 'fighter-vitals' + (block > 0 ? ' is-blocked' : ''));
    const health = node('div', undefined, 'fighter-health');
    const fill = node('span', undefined, 'fighter-health-fill'); fill.style.width = (100 * hp / max) + '%';
    health.append(fill, node('span', number(hp) + ' / ' + number(max), 'fighter-health-value'));
    vitals.append(health);
    if (block > 0) {
      const shield = node('span', undefined, 'fighter-block');
      const blockLabel = t('combat.block', { block });
      shield.setAttribute('role', 'img'); shield.setAttribute('aria-label', blockLabel); shield.title = blockLabel;
      const value = node('span', number(block), 'fighter-block-value'); value.setAttribute('aria-hidden', 'true');
      shield.append(icon('shield'), value); vitals.append(shield);
    }
    const statuses = node('div', undefined, 'fighter-statuses'); statuses.append(...details);
    el.append(intentRow, figure, node('div', label, 'fighter-name'), vitals, statuses);
    return el;
  }
  function iconText(iconName, text, className) { const el = node('span', text, className); el.prepend(icon(iconName)); return el; }
  function renderIntent(intent) {
    const view = intentView(intent, display);
    const el = node('span', undefined, 'intent' + (view.attack ? ' intent-attack' : ''));
    el.setAttribute('role', 'img'); el.setAttribute('aria-label', view.label); el.title = view.label;
    const symbol = node('span', undefined, 'intent-symbol');
    view.glyphs.forEach((glyph, index) => {
      const svg = icon(glyph.id); svg.classList.add(index ? 'intent-secondary' : 'intent-main', 'tone-' + glyph.tone); symbol.append(svg);
    });
    const caption = node('span', view.caption, 'intent-caption'); caption.setAttribute('aria-hidden', 'true');
    el.append(symbol, caption); return el;
  }
  function combatBoard(before, heading) {
    const board = node('section', undefined, 'combat-board'); board.setAttribute('aria-label', heading);
    const hud = node('div', undefined, 'combat-hud'); hud.append(resources(before));
    const field = node('div', undefined, 'battlefield');
    const enemies = before.enemies ?? [before.enemy];
    if (enemies.length > 1) field.classList.add('multi-enemy');
    const lineup = node('div', undefined, 'enemy-lineup');
    for (const enemy of enemies) lineup.append(fighter(name('enemy', enemy.enemy_id), enemy.hp, enemy.max_hp, {
      enemy: true, block: enemy.block, intent: enemy.intent, details: enemy.powers.map(power => node('span', t('combat.power', { power: name('power', power.power_id), amount: power.amount }), 'status')),
      figureIcon: enemy.enemy_id === 'Cultist' ? 'flame' : 'bug',
    }));
    field.append(
      fighter(name('character', story.character_id), before.hp, before.max_hp, { block: before.block,
        details: (before.powers ?? []).map(power => node('span', t('combat.power', { power: name('power', power.power_id), amount: power.amount }), 'status')) }),
      lineup,
    );
    const dock = node('div', undefined, 'combat-dock');
    const energy = node('div', undefined, 'energy-reserve'); energy.setAttribute('aria-label', t('combat.energy', { energy: before.energy }));
    energy.append(node('strong', number(before.energy), 'energy-orb'), node('span', t('combat.energyLabel'), 'energy-label'));
    const hand = node('div', undefined, 'hand'); for (const card of before.hand) hand.append(gameCard(card));
    hand.setAttribute('role', 'group'); hand.setAttribute('aria-label', t('combat.hand'));
    dock.append(energy, hand); board.append(hud, field, dock);
    if (before.piles) board.append(pileViews(before.piles));
    return board;
  }
  function renderCombat(scene) {
    const { before, after } = scene;
    const board = combatBoard(before, title(scene));
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
  function cardGroupLabel(card) {
    const label = display.cardName(card.card_id, card.upgrades ?? 0);
    return card.count > 1 ? label + ' ×' + number(card.count) : label;
  }
  function pileViews(piles) {
    const row = node('div', undefined, 'combat-piles');
    for (const zone of ['draw', 'exhaust', 'discard']) {
      const pile = piles[zone], details = node('details', undefined, 'combat-pile'); details.open = openPiles.has(zone);
      details.addEventListener('toggle', () => { if (details.isConnected) { if (details.open) openPiles.add(zone); else openPiles.delete(zone); } });
      const summary = node('summary'); summary.append(icon('cards'), node('span', t('sequence.' + zone)), node('strong', number(pile.count)), icon('chevron-right'));
      details.append(summary);
      if (pile.ordered) details.append(node('div', t('sequence.ordered'), 'pile-order-label'));
      const list = node(pile.ordered ? 'ol' : 'ul', undefined, 'pile-cards' + (pile.ordered ? ' pile-ordered' : ''));
      for (const card of pile.cards) list.append(node('li', cardGroupLabel(card)));
      if (!pile.cards.length) list.append(node('li', t('sequence.empty')));
      details.append(list); row.append(details);
    }
    return row;
  }
  function sequenceActionLabel(action, current) {
    if (action.kind === 'end_turn') return t('sequence.endTurn');
    if (action.kind === 'choose_route_node') return t('sequence.enterRoom');
    if (action.kind === 'choose_event_option') return t('sequence.chooseEvent', { option: action.option_index + 1 });
    if (action.kind === 'claim_reward' && action.reward_id) return t('sequence.claimItem', { item: name(action.reward_kind, action.reward_id) });
    if (action.kind === 'claim_reward') return t('sequence.claimGold', { amount: action.amount });
    if (action.kind === 'open_card_reward') return t('sequence.openCards');
    if (action.kind === 'skip_card_reward') return t('sequence.skipCards');
    if (action.kind === 'singing_bowl') return t('sequence.maxHpReward', { amount: action.max_hp_gain });
    if (action.kind === 'proceed') return t('sequence.returnToMap');
    const card = display.cardName(action.card.card_id, action.card.upgrades);
    if (action.kind === 'take_card') return t('sequence.takeCard', { card });
    return current && action.target_enemy_id ? t('sequence.targetedCard', { card, target: name('enemy', action.target_enemy_id) }) : card;
  }
  function postCombatBoard(state, completed) {
    const board = node('section', undefined, 'combat-board sequence-board');
    const hud = node('div', undefined, 'combat-hud'); hud.append(resources(state));
    if (state.kind === 'map') {
      board.append(hud); return board;
    }
    const result = node('div', undefined, 'battle-outcome');
    result.append(node('h2', t(state.kind === 'card_reward' ? 'sequence.chooseCard' : activeJourney().visit.event_id ? 'scene.initialReward' : 'sequence.victory'), 'outcome-title'));
    if (state.kind === 'card_reward') {
      result.classList.add('card-reward-outcome');
      const cards = node('div', undefined, 'reward-cards');
      for (const card of state.reward_cards) cards.append(gameCard(card));
      result.append(cards);
      const alternatives = node('div', undefined, 'reward-alternatives');
      alternatives.setAttribute('role', 'group'); alternatives.setAttribute('aria-label', t('sequence.otherChoices'));
      for (const option of state.reward_alternatives) {
        const label = option.kind === 'skip_card_reward' ? t('sequence.skip') : t('sequence.maxHpReward', { amount: option.max_hp_gain });
        const entry = node('span', label, 'reward-alternative'); entry.dataset.command = option.command;
        if (option.kind === 'singing_bowl') { entry.prepend(icon('heart')); entry.title = name('relic', 'SingingBowl'); }
        alternatives.append(entry);
      }
      result.append(alternatives);
    } else {
      const panel = node('section', undefined, 'combat-reward-panel');
      if (state.kind === 'reward') panel.append(node('h3', t(activeJourney().visit.event_id ? 'scene.initialReward' : 'scene.rewards')));
      const rewards = node('ul', undefined, 'combat-rewards');
      const rewardRow = (glyph, label, claimed) => {
        const row = node('li', undefined, 'combat-reward' + (claimed ? ' is-claimed' : ''));
        const symbol = node('span', undefined, 'reward-symbol'); symbol.append(icon(glyph));
        row.append(symbol, node('span', label));
        if (claimed) { const mark = icon('check'); mark.classList.add('reward-claimed'); mark.setAttribute('role', 'img'); mark.setAttribute('aria-hidden', 'false'); mark.setAttribute('aria-label', t('sequence.claimed')); row.append(mark); }
        rewards.append(row);
      };
      const claimedGold = completed.filter(action => action.kind === 'claim_reward' && Number.isFinite(action.amount)).reduce((total, action) => total + action.amount, 0);
      if (state.reward_gold || claimedGold) rewardRow('coin', t('sequence.goldReward', { amount: state.reward_gold || claimedGold }), !state.reward_gold);
      if (state.has_card_reward) rewardRow('cards', t('sequence.addCard'), false);
      for (const item of state.reward_items) rewardRow(item.kind === 'potion' ? 'flame' : 'bolt', name(item.kind, item.id), false);
      for (const action of completed.filter(action => action.kind === 'take_card')) rewardRow('cards', t('sequence.takeCard', { card: display.cardName(action.card.card_id, action.card.upgrades) }), true);
      for (const action of completed.filter(action => action.kind === 'skip_card_reward' || action.kind === 'singing_bowl')) rewardRow(action.kind === 'singing_bowl' ? 'heart' : 'chevron-right', sequenceActionLabel(action), true);
      panel.append(rewards); result.append(panel);
    }
    board.append(hud, result); return board;
  }
  function renderSequence(scene, motion, scrollLeft) {
    const view = sequenceView(scene, actionCursor), state = view.state;
    const record = node('section', undefined, 'sequence-detail-panel');
    const header = node('div', undefined, 'sequence-toolbar');
    const controls = node('div', undefined, 'sequence-controls');
    const progress = node('span', number(actionCursor) + ' / ' + number(scene.actions.length), 'sequence-progress');
    progress.id = 'sequence-progress'; progress.tabIndex = -1;
    progress.setAttribute('role', 'status'); progress.setAttribute('aria-atomic', 'true');
    progress.setAttribute('aria-label', t('sequence.progress', { current: actionCursor, total: scene.actions.length }));
    for (const [offset, label, glyph] of [[-1, 'previous', 'chevron-left'], [1, 'next', 'chevron-right']]) {
      const button = node('button', undefined, 'btn sequence-control' + (offset > 0 ? ' btn-primary' : ''));
      button.id = 'sequence-' + label; button.type = 'button';
      button.append(...(offset < 0 ? [icon(glyph), t('sequence.' + label)] : [t('sequence.' + label), icon(glyph)]));
      button.disabled = actionCursor + offset < 0 || actionCursor + offset >= scene.snapshots.length;
      button.addEventListener('click', () => selectAction(actionCursor + offset, '#' + button.id));
      controls.append(button); if (offset < 0) controls.append(progress);
    }
    header.append(controls);
    const history = node('div', undefined, 'sequence-record');
    history.append(node('span', state.kind === 'combat' ? t('scene.turn', { turn: state.turn })
      : state.kind === 'event' ? name('event', state.event_id) : t(state.kind === 'map' ? 'sequence.mapReached' : 'scene.rewards'), 'sequence-turn'));
    const actions = node('div', undefined, 'sequence-history'); actions.setAttribute('role', 'group'); actions.setAttribute('aria-label', t('sequence.history'));
    for (let index = 0; index <= actionCursor; index++) {
      const action = scene.actions[index - 1];
      const button = node('button', index === 0 ? t('sequence.start') : sequenceActionLabel(action, index === actionCursor), 'sequence-step');
      button.type = 'button'; button.dataset.cursor = index;
      if (index > 0) button.prepend(icon(action.kind === 'play_card' ? cardIcon(action.card.type) : action.kind === 'claim_reward' ? 'coin' : 'chevron-right'));
      if (index === actionCursor) button.setAttribute('aria-current', 'step');
      button.addEventListener('click', () => selectAction(index)); actions.append(button);
    }
    const historyWindow = node('div', undefined, 'sequence-history-window');
    const scrollButtons = [-1, 1].map(direction => {
      const button = node('button', undefined, 'history-scroll'); button.type = 'button'; button.disabled = true;
      button.setAttribute('aria-label', t(direction < 0 ? 'sequence.earlierActions' : 'sequence.laterActions'));
      button.append(icon(direction < 0 ? 'chevron-left' : 'chevron-right'));
      button.addEventListener('click', () => {
        actions.scrollLeft = Math.max(0, Math.min(actions.scrollWidth - actions.clientWidth, actions.scrollLeft + direction * Math.max(120, actions.clientWidth * .7)));
        updateHistory();
      });
      return button;
    });
    const updateHistory = () => {
      const overflow = actions.scrollWidth > actions.clientWidth + 1;
      for (const button of scrollButtons) button.classList.toggle('is-needed', overflow);
      const max = Math.max(0, actions.scrollWidth - actions.clientWidth);
      scrollButtons[0].disabled = actions.scrollLeft <= 1;
      scrollButtons[1].disabled = actions.scrollLeft >= max - 1;
    };
    actions.addEventListener('scroll', updateHistory, { passive: true });
    historyWindow.append(scrollButtons[0], actions, scrollButtons[1]);
    history.append(historyWindow); header.append(history);
    if (actionCursor) {
      const details = node('details', undefined, 'sequence-details'); details.open = sequenceDetailsOpen;
      details.addEventListener('toggle', () => { if (details.isConnected) sequenceDetailsOpen = details.open; });
      details.append(node('summary', t('sequence.details')));
      const previous = scene.snapshots[actionCursor - 1], changes = node('div', undefined, 'sequence-changes');
      for (const [field, label] of [['energy', t('combat.energyLabel')], ['block', t('sequence.block')], ['hp', name('character', story.character_id)]]) {
        if (view.deltas[field]) changes.append(node('span', t('sequence.change', { label, before: previous[field], after: state[field] })));
      }
      if (view.deltas.enemy_hp) changes.append(node('span', t('sequence.change', { label: t('combat.enemies'), before: previous.enemies.reduce((sum, enemy) => sum + enemy.hp, 0), after: state.enemies.reduce((sum, enemy) => sum + enemy.hp, 0) })));
      if (view.added.length) changes.append(node('span', t('sequence.addedHand', { cards: view.added.map(cardGroupLabel).join(display.locale === 'zh-CN' ? '、' : ', ') }), 'sequence-new-cards'));
      details.append(changes); record.append(details);
    }
    const visitKey = snapshot => JSON.stringify([snapshot.location.act, snapshot.location.floor, snapshot.location.node]);
    const completedHere = view.completed.filter((_action, index) => visitKey(scene.snapshots[index + 1]) === visitKey(state));
    const board = state.kind === 'combat' ? combatBoard(state, title(scene))
      : state.kind === 'event' ? sequenceEventBoard(state) : postCombatBoard(state, completedHere);
    board.classList.add('sequence-board');
    const content = node('div', undefined, 'sequence-content');
    const phase = node('div', undefined, 'sequence-scene');
    const hud = board.firstElementChild;
    while (hud.nextSibling) phase.append(hud.nextSibling);
    const mapSlot = node('aside', undefined, 'map-stage'); mapSlot.id = 'map-stage'; mapSlot.hidden = true;
    const reopen = node('button', undefined, 'map-reopen'); reopen.id = 'map-reopen'; reopen.type = 'button';
    reopen.setAttribute('aria-controls', 'journey-map'); reopen.setAttribute('aria-expanded', 'false'); reopen.setAttribute('aria-label', t('nav.showMap'));
    reopen.append(icon('map'), node('span', t('nav.map'))); reopen.hidden = true;
    reopen.addEventListener('click', () => {
      mapCollapsed = false; saveMapPreference(); const before = mapBeforeLayout(); updateMapLayout(before);
      mapPanel.querySelector('.map-viewport')?.focus({ preventScroll: true });
    });
    mapSlot.append(reopen);
    content.append(phase, mapSlot); board.append(content, header);
    for (const index of motion?.hand?.added ?? []) board.querySelector('.hand').children[index].classList.add('is-arriving');
    byId('stage').append(board, record);
    const hand = board.querySelector('.hand'); if (hand) hand.scrollLeft = scrollLeft;
    const arriving = board.querySelector('.is-arriving');
    if (arriving) {
      hand.scrollLeft += Math.max(0, arriving.getBoundingClientRect().right - hand.getBoundingClientRect().right + 6);
    }
    const current = actions.querySelector('[aria-current]');
    refreshHistory = () => {
      const viewport = actions.getBoundingClientRect(), item = current.getBoundingClientRect();
      const delta = item.right > viewport.right ? item.right - viewport.right + 4
        : item.left < viewport.left ? item.left - viewport.left - 4 : 0;
      actions.scrollLeft = Math.max(0, Math.min(actions.scrollWidth - actions.clientWidth, actions.scrollLeft + delta));
      updateHistory();
    };
    refreshHistory();
    if (typeof ResizeObserver === 'function') {
      historyObserver = new ResizeObserver(refreshHistory); historyObserver.observe(actions);
    }
    if (motion?.enabled) animateBoundary(board, motion);
  }
  function selectAction(index, focus = `[data-cursor="${index}"]`) {
    if (index < 0 || index >= scenes[selected].snapshots.length) return;
    let motion = null;
    if (index === actionCursor + 1 && scenes[selected].snapshots[actionCursor].kind === 'combat' && scenes[selected].snapshots[index].kind === 'combat') {
      const scene = scenes[selected], hand = byId('stage').querySelector('.hand');
      const action = scene.actions[actionCursor];
      motion = { hand: action.kind === 'end_turn' ? { played: null, retained: [], added: scene.snapshots[index].hand.map((_card, index) => index) }
        : handTransition(scene.snapshots[actionCursor].hand, scene.snapshots[index].hand, action.hand_index),
        enabled: !window.matchMedia('(prefers-reduced-motion: reduce)').matches && typeof hand.animate === 'function' };
      if (motion.enabled) {
        motion.rects = [...hand.children].map(card => card.getBoundingClientRect());
        motion.played = motion.hand?.played != null ? hand.children[motion.hand.played].cloneNode(true) : null;
        motion.healthWidths = [...byId('stage').querySelectorAll('.fighter-health-fill')].map(fill => fill.getBoundingClientRect().width);
      }
    }
    if (index !== actionCursor) { mapExpanded = false; mapSelection = null; }
    actionCursor = index; render(motion); writeLocation();
    const target = focus ? byId('stage').querySelector(focus) : null;
    (target?.disabled ? byId('sequence-progress') : target)?.focus({ preventScroll: true });
  }
  function clearMotion() {
    for (const item of activeMotion) { item.animation.cancel(); item.ghost?.remove(); }
    activeMotion.clear();
  }
  function animateBoundary(board, motion) {
    const play = (element, frames, ghost) => {
      const animation = element.animate(frames, { duration: 200, easing: 'cubic-bezier(.2,.7,.2,1)' });
      const item = { animation, ghost }; activeMotion.add(item);
      const settled = () => { ghost?.remove(); activeMotion.delete(item); };
      animation.finished.then(settled, settled);
    };
    const hand = board.querySelector('.hand');
    if (motion.hand) {
      const { played, retained, added } = motion.hand;
      const rect = motion.rects[played], ghost = motion.played;
      if (rect?.width > 0 && rect.height > 0 && ghost) {
        ghost.classList.remove('is-arriving'); ghost.classList.add('played-card-ghost'); ghost.setAttribute('aria-hidden', 'true');
        Object.assign(ghost.style, { left: rect.left + 'px', top: rect.top + 'px', width: rect.width + 'px', height: rect.height + 'px' });
        document.body.append(ghost);
        play(ghost, [{ opacity: .85, transform: 'translateY(0)' }, { opacity: 0, transform: 'translateY(-16px)' }], ghost);
      }
      for (const { from, to } of retained) {
        const card = hand.children[to], now = card.getBoundingClientRect(), before = motion.rects[from];
        play(card, [{ transform: `translate(${before.left - now.left}px, ${before.top - now.top}px)` }, { transform: 'translate(0, 0)' }]);
      }
      for (const index of added) play(hand.children[index], [{ opacity: .25, transform: 'translateY(6px)' }, { opacity: 1, transform: 'translateY(0)' }]);
    }
    board.querySelectorAll('.fighter-health-fill').forEach((fill, index) => {
      if (motion.healthWidths[index] > 0) play(fill, [{ width: motion.healthWidths[index] + 'px' }, { width: fill.style.width }]);
    });
  }
  function resources(state) {
    const row = node('div', undefined, 'resource-line');
    row.append(iconText('heart', number(state.hp) + ' / ' + number(state.max_hp)), iconText('coin', number(state.gold)), ...(state.relic_ids ?? story.relic_ids).map(id => iconText('flame', name('relic', id))));
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
  function sequenceEventBoard(state) {
    const board = node('section', undefined, 'combat-board sequence-board');
    const hud = node('div', undefined, 'combat-hud'); hud.append(resources(state));
    const layout = node('div', undefined, 'sequence-event'); layout.append(node('h2', name('event', state.event_id), 'outcome-title'));
    const options = node('ul', undefined, 'event-options');
    for (const option of state.event_options) {
      const entry = node('li', undefined, 'event-option');
      const label = node('span', undefined, 'option-label');
      for (const run of neowStructuredRuns(option, display.locale)) label.append(node('span', run.text, 'option-text-' + run.tone));
      entry.append(label); options.append(entry);
    }
    layout.append(options); board.append(hud, layout); return board;
  }
  function title(scene) { return scene.kind === 'sequence' ? t(scene.provenance.operator === 'model' ? scene.provenance.scope === 'continuous' ? 'sequence.modelRunTitle' : 'sequence.modelTitle' : 'sequence.title') : scene.kind === 'event' ? name('event', scene.event_id) : scene.kind === 'combat' ? name('encounter', scene.encounter_id) : t('scene.rewards'); }
  function label(scene) { return scene.kind === 'sequence' && scene.provenance.scope === 'continuous'
    ? t('scene.floor', { floor: scene.snapshots[actionCursor].location.floor }) : scene.kind === 'sequence' ? scene.provenance.preparation === 'developer-natural'
    ? t('sequence.preparedBattle', { encounter: name('encounter', scene.encounter_id) }) : t('sequence.manual')
    : scene.kind === 'event' ? t('scene.start') : scene.kind === 'combat' ? t('scene.turn', { turn: scene.turn }) : t('scene.floor', { floor: scene.floor }); }
  function renderNavigation() {
    const { recording } = activeJourney();
    byId('recording').replaceChildren(...journeys.map(record => {
      const option = node('option', t(record.title_key)); option.value = record.id; return option;
    }));
    byId('recording').value = recording.id;
    byId('timeline').replaceChildren(); byId('timeline').dataset.recording = recording.id;
    let chapter;
    recording.visits.forEach((visit, visitIndex) => {
      if (!chapter || chapter.dataset.chapterAct !== String(visit.act)) {
        chapter = node('section', undefined, 'journey-act'); chapter.dataset.chapterAct = visit.act;
        chapter.append(node('h2', t('scene.act', { act: visit.act }))); byId('timeline').append(chapter);
      }
      const room = node('div', undefined, 'journey-visit');
      const button = node('button', undefined, 'visit-link'); button.type = 'button'; button.dataset.visit = visitIndex;
      const title = visit.event_id ? name('event', visit.event_id) : visit.encounter_id ? name('encounter', visit.encounter_id) : t('nav.room');
      button.append(node('span', visit.floor === 0 ? '—' : number(visit.floor), 'floor-number'), node('span', title));
      const first = visit.phases[0]; button.addEventListener('click', () => select(scenes.findIndex(scene => scene.id === first.scene_id), true, first.cursor));
      const phases = node('div', undefined, 'visit-phases'); phases.dataset.visit = visitIndex;
      visit.phases.forEach((phase, phaseIndex) => {
        const target = scenes.find(scene => scene.id === phase.scene_id);
        const caption = visit.event_id && phase.kind === 'reward' ? t('scene.initialReward') : target.kind === 'combat' ? t('scene.turn', { turn: target.turn }) : t('nav.phase.' + phase.kind);
        const entry = node('button', caption, 'scene-link'); entry.type = 'button'; entry.dataset.scene = phase.scene_id;
        entry.dataset.phase = visitIndex + ':' + phaseIndex;
        entry.addEventListener('click', () => select(scenes.findIndex(scene => scene.id === phase.scene_id), true, phase.cursor)); phases.append(entry);
      });
      room.append(button, phases); chapter.append(room);
    });
  }
  function activeJourney() {
    for (const recording of journeys) for (const [visitIndex, visit] of recording.visits.entries()) {
      const phaseIndex = visit.phases.findIndex(phase => phase.scene_id === scenes[selected].id && actionCursor >= phase.cursor && actionCursor <= phase.end_cursor);
      if (phaseIndex >= 0) return { recording, visit, visitIndex, phaseIndex };
    }
    throw new Error('No recorded visit for the current boundary');
  }
  function updateNavigation() {
    const { recording, visit, visitIndex, phaseIndex } = activeJourney();
    if (byId('timeline').dataset.recording !== recording.id) renderNavigation();
    for (const entry of byId('timeline').querySelectorAll('[data-visit]')) {
      const current = entry.dataset.visit === String(visitIndex);
      if (entry.matches('.visit-phases')) entry.hidden = !current;
      else {
        entry.classList.toggle('is-current', current);
        const floor = recording.visits[Number(entry.dataset.visit)].floor;
        entry.setAttribute('aria-label', (floor === 0 ? t('scene.start') : t('scene.floor', { floor })) + ' ' + entry.lastElementChild.textContent);
      }
    }
    for (const entry of byId('timeline').querySelectorAll('[data-phase]')) {
      if (entry.dataset.phase === visitIndex + ':' + phaseIndex) entry.setAttribute('aria-current', 'step'); else entry.removeAttribute('aria-current');
    }
    for (const entry of document.querySelectorAll('[data-act]')) entry.textContent = t('scene.act', { act: visit.act });
    byId('map-location').textContent = t('scene.floor', { floor: visit.floor });
  }
  function renderMap(scene) {
    const map = scene.kind === 'sequence' ? replayMap(scene, actionCursor) : null;
    mapAvailable = Boolean(map);
    if (!map) {
      mapSelection = null; renderMapSelection();
      byId('map-content').replaceChildren(); mapSignature = undefined; mapFocusKey = undefined;
      scrollMapToCurrent = () => {}; return;
    }
    const focusKey = JSON.stringify([scene.id, scene.snapshots[actionCursor].location.map_id, map.current]);
    const moved = focusKey !== mapFocusKey; mapFocusKey = focusKey;
    const signature = JSON.stringify([display.locale, map]);
    if (signature === mapSignature) { renderMapSelection(); return moved; }
    mapSignature = signature;
    const scrollTop = mapPanel.querySelector('.map-viewport')?.scrollTop ?? 0;
    byId('map-content').replaceChildren();
    const svgNode = (tag, attributes = {}) => {
      const el = document.createElementNS('http://www.w3.org/2000/svg', tag);
      for (const [key, value] of Object.entries(attributes)) el.setAttribute(key, value);
      return el;
    };
    const maxY = Math.max(...map.nodes.map(node => node.y));
    // The boss is a shared destination; center its display without changing its source coordinate.
    const point = node => [node.room === 'boss' ? 246 : 48 + node.x * 66, 56 + (maxY - node.y) * 58];
    const glyphs = { monster: 'swords', elite: 'map-elite', rest: 'flame', shop: 'coin', treasure: 'map-chest', boss: 'map-boss', question_mark: 'intent-unknown' };
    const svg = svgNode('svg', { viewBox: `0 0 492 ${100 + maxY * 58}`, class: 'map-graph', role: 'group', 'aria-label': t('nav.map') });
    const edges = svgNode('g', { class: 'map-edges', 'aria-hidden': 'true' });
    for (const origin of map.nodes) for (const destination of origin.destinations) {
      const next = map.nodes.find(node => node.x === destination[0] && node.y === destination[1]);
      const from = point(origin), to = point(next), travelled = map.route.some(([a, b]) => a === `${origin.x},${origin.y}` && b === String(destination));
      edges.append(svgNode('path', { d: `M${from} L${to}`, class: travelled ? 'is-travelled' : '' }));
    }
    svg.append(edges);
    for (const room of map.nodes) {
      const [x, y] = point(room), label = t('nav.mapNode', { room: name('room', room.room), x: room.x, y: room.y })
        + (room.current ? ' ' + t('nav.currentRoom') : '') + (room.emerald_key ? ' ' + t('nav.emerald') : '');
      const group = svgNode('g', { transform: `translate(${x} ${y})`, class: 'map-node' + (room.current ? ' is-current' : '') + (room.target ? ' is-visited' : '') + (room.emerald_key ? ' has-key' : ''),
        role: room.target ? 'button' : 'img', 'aria-label': label, ...(room.target ? { tabindex: 0, 'aria-pressed': 'false' } : {}) });
      group.dataset.mapNode = `${room.x},${room.y}`;
      const title = svgNode('title'); title.textContent = label; group.append(title, svgNode('circle', { r: room.room === 'boss' ? 23 : 17 }));
      const glyph = icon(glyphs[room.room]); glyph.setAttribute('x', '-11'); glyph.setAttribute('y', '-11'); glyph.setAttribute('width', '22'); glyph.setAttribute('height', '22'); group.append(glyph);
      if (room.target) {
        group.dataset.mapCursor = room.target.cursor;
        group.append(svgNode('circle', { r: room.room === 'boss' ? 28 : 22, class: 'map-selection-ring' }));
        const choose = () => {
          mapSelection = { node: group.dataset.mapNode, target: room.target, room: room.room,
            scene_id: scene.id, map_id: scene.snapshots[actionCursor].location.map_id };
          renderMapSelection();
        };
        group.addEventListener('click', choose);
        group.addEventListener('keydown', event => {
          if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); choose(); byId('map-follow').focus({ preventScroll: true }); }
        });
      }
      if (room.room === 'boss' && map.boss) { const boss = svgNode('text', { x: 0, y: -33, class: 'map-boss-name' }); boss.textContent = name('encounter', map.boss); group.append(boss); }
      svg.append(group);
    }
    const viewport = node('div', undefined, 'map-viewport'); viewport.tabIndex = 0;
    viewport.setAttribute('aria-label', t('nav.map')); viewport.append(svg);
    viewport.addEventListener('click', event => { if (!event.target.closest('.map-node[role="button"]')) dismissMapSelection(); });
    const legend = node('div', undefined, 'map-legend');
    for (const [kind, glyph] of Object.entries(glyphs)) legend.append(iconText(glyph, name('room', kind)));
    byId('map-content').append(viewport, legend);
    renderMapSelection();
    // Before the first room there is no current node. Show the public entrances
    // at the bottom rather than falling back to the boss at scroll position 0.
    const current = map.nodes.find(node => node.current) ?? map.nodes.reduce((first, node) => node.y < first.y ? node : first);
    scrollMapToCurrent = () => {
      const width = svg.getBoundingClientRect().width || Math.min(viewport.clientWidth, 492);
      const top = point(current)[1] * width / 492 - viewport.clientHeight * .72;
      viewport.scrollTop = Math.max(0, Math.min(viewport.scrollHeight - viewport.clientHeight, top));
    };
    viewport.scrollTop = scrollTop;
    return moved;
  }
  function renderMapSelection() {
    const scene = scenes[selected], state = scene.kind === 'sequence' ? scene.snapshots[actionCursor] : null;
    if (mapSelection && (mapSelection.scene_id !== scene.id || mapSelection.map_id !== state?.location.map_id)) mapSelection = null;
    byId('map-selection').hidden = !mapSelection;
    for (const group of mapPanel.querySelectorAll('.map-node[role="button"]')) {
      const chosen = group.dataset.mapNode === mapSelection?.node;
      group.classList.toggle('is-selected', chosen); group.setAttribute('aria-pressed', String(chosen));
    }
    if (mapSelection) {
      const target = scenes.find(scene => scene.id === mapSelection.target.scene_id).snapshots[mapSelection.target.cursor];
      byId('map-selection-label').textContent = t('nav.selectedRoom', { floor: target.location.floor, room: name('room', mapSelection.room) });
    }
  }
  function dismissMapSelection(returnFocus = false) {
    const group = [...mapPanel.querySelectorAll('.map-node')].find(group => group.dataset.mapNode === mapSelection?.node);
    mapSelection = null; renderMapSelection();
    if (returnFocus) group?.focus({ preventScroll: true });
  }
  function saveMapPreference() {
    try { localStorage.setItem('spiresim.demo.mapCollapsed', String(mapCollapsed)); } catch {}
  }
  function desiredMapMode() {
    const scene = scenes[selected], state = scene.kind === 'sequence' ? scene.snapshots[actionCursor] : null;
    if (!state?.location?.map_id) return 'hidden';
    if (state.kind === 'map') return 'primary';
    if (mapExpanded) return 'expanded';
    const collapsed = mapCollapsed ?? !window.matchMedia('(min-width: 1280px)').matches;
    return collapsed ? 'collapsed' : 'side';
  }
  const mainMap = mode => ['primary', 'expanded'].includes(mode);
  const visibleMap = mode => ['primary', 'expanded', 'side'].includes(mode);
  function mapBeforeLayout() {
    const next = desiredMapMode(), visible = visibleMap(mapMode);
    const scene = scenes[selected], state = scene.kind === 'sequence' ? scene.snapshots[actionCursor] : null;
    const phase = byId('stage').querySelector('.sequence-scene');
    const changed = phase && byId('stage').dataset.scene === scene.id
      && (mapMode !== next || byId('stage').dataset.phase !== state?.kind);
    return { mode: mapMode, rect: visible ? mapPanel.getBoundingClientRect() : mapMode === 'collapsed' ? byId('map-reopen')?.getBoundingClientRect() : null,
      scrollTop: mapPanel.querySelector('.map-viewport')?.scrollTop ?? 0,
      focus: mapPanel.contains(document.activeElement) ? document.activeElement : null,
      ghost: visible && !visibleMap(next) ? mapPanel.cloneNode(true) : null,
      phase: changed ? { rect: !phase.hidden ? phase.getBoundingClientRect() : null, ghost: !phase.hidden ? phase.cloneNode(true) : null, scrollTop: phase.scrollTop } : null };
  }
  function positionMap() {
    cancelAnimationFrame(mapPositionFrame);
    mapPositionFrame = requestAnimationFrame(() => { if (visibleMap(mapMode)) scrollMapToCurrent(); });
  }
  function stopLayoutMotion() {
    for (const items of [phaseMotions, mapMotions]) {
      for (const item of items) { item.animation.cancel(); item.ghost?.remove(); }
      items.clear();
    }
  }
  function transitionGhost(source, rect, className) {
    source.removeAttribute('id'); source.querySelectorAll('[id]').forEach(el => el.removeAttribute('id'));
    source.inert = true; source.setAttribute('aria-hidden', 'true'); source.classList.add(className);
    Object.assign(source.style, { left: rect.left + 'px', top: rect.top + 'px', width: rect.width + 'px', height: rect.height + 'px' });
    document.body.append(source); return source;
  }
  function playLayoutMotion(items, element, frames, ghost) {
    const animation = element.animate(frames, { duration: 240, easing: 'cubic-bezier(.2,.7,.2,1)' });
    const item = { animation, ghost }; items.add(item);
    const settled = () => { ghost?.remove(); items.delete(item); };
    animation.finished.then(settled, settled);
  }
  function animateMap(before) {
    if (before.mode === mapMode) return;
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches || typeof mapPanel.animate !== 'function') return;
    let element = mapPanel, ghost;
    let frames = [{ opacity: 0, transform: 'translateX(12px)', transformOrigin: 'top left' }, { opacity: 1, transform: 'none', transformOrigin: 'top left' }];
    if (!visibleMap(mapMode)) {
      if (!before.ghost || !before.rect?.width) return;
      ghost = transitionGhost(before.ghost, before.rect, 'map-transition-ghost'); element = ghost;
      ghost.querySelector('.map-viewport').scrollTop = before.scrollTop;
      const rail = byId('map-reopen')?.getBoundingClientRect();
      frames = [{ opacity: 1, transform: 'none', transformOrigin: 'top left' }, { opacity: 0, transformOrigin: 'top left',
        transform: rail?.width && mapMode === 'collapsed' ? `translate(${rail.left - before.rect.left}px, ${rail.top - before.rect.top}px) scale(${rail.width / before.rect.width}, 1)` : 'translateX(12px)' }];
    } else if (before.rect?.width && before.rect.height) {
      const rect = mapPanel.getBoundingClientRect();
      if (visibleMap(before.mode)) frames[0].opacity = 1;
      if (rect.width && rect.height) frames[0].transform = `translate(${before.rect.left - rect.left}px, ${before.rect.top - rect.top}px) scale(${before.rect.width / rect.width}, ${before.rect.height / rect.height})`;
    }
    playLayoutMotion(mapMotions, element, frames, ghost);
  }
  function animatePhase(before) {
    const phase = byId('stage').querySelector('.sequence-scene');
    if (!phase || window.matchMedia('(prefers-reduced-motion: reduce)').matches || typeof phase.animate !== 'function') return;
    if (before.rect?.width && before.ghost) {
      const ghost = transitionGhost(before.ghost, before.rect, 'phase-transition-ghost');
      ghost.scrollTop = before.scrollTop;
      playLayoutMotion(phaseMotions, ghost, [{ opacity: 1 }, { opacity: 0 }], ghost);
    }
    if (!phase.hidden) playLayoutMotion(phaseMotions, phase, [{ opacity: 0, transform: 'translateX(-12px)' }, { opacity: 1, transform: 'none' }]);
  }
  function updateMapLayout(before = mapBeforeLayout(), moved = false) {
    mapMode = mapAvailable ? desiredMapMode() : 'hidden';
    stopLayoutMotion();
    const primary = mapMode === 'primary', main = mainMap(mapMode), visible = visibleMap(mapMode);
    const host = mapAvailable ? byId('map-stage') : byId('map-home');
    if (mapPanel.parentElement !== host) host.append(mapPanel);
    const content = byId('stage').querySelector('.sequence-content');
    if (content) {
      content.dataset.mapMode = mapMode;
      content.querySelector('.sequence-scene').hidden = main;
      byId('map-stage').hidden = !mapAvailable;
      byId('map-reopen').hidden = mapMode !== 'collapsed';
    }
    byId('replay-layout').dataset.mapMode = mapMode; mapPanel.dataset.mode = mapMode;
    refreshHistory();
    mapPanel.hidden = !visible;
    byId('map-close').hidden = primary;
    const closeLabel = t(mapMode === 'expanded' ? 'nav.returnToPlay' : 'nav.collapse');
    byId('map-close-label').textContent = closeLabel;
    byId('map-close').setAttribute('aria-label', t(mapMode === 'expanded' ? 'nav.returnToPlay' : 'nav.collapseMap')); byId('map-close').title = closeLabel;
    byId('map-close-icon').replaceChildren(icon(mapMode === 'expanded' ? 'chevron-left' : 'chevron-right'));
    byId('map-expand').hidden = main; byId('map-expand').title = t('nav.expandMap');
    if (before.focus) {
      const target = visible && before.focus.isConnected && !before.focus.closest('[hidden]') ? before.focus
        : mapMode === 'collapsed' ? byId('map-reopen') : byId('sequence-progress');
      target?.focus({ preventScroll: true });
    }
    if (visible && (moved || before.mode !== mapMode)) positionMap();
    if (before.phase) animatePhase(before.phase);
    animateMap(before);
  }
  function closeMap() {
    if (mapMode === 'expanded') mapExpanded = false;
    else { mapCollapsed = true; saveMapPreference(); dismissMapSelection(); }
    const before = mapBeforeLayout(); updateMapLayout(before);
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
  function render(motion = null) {
    const scene = scenes[selected]; byId('heading').textContent = title(scene); byId('turn-label').textContent = label(scene);
    const beforeMap = mapBeforeLayout();
    stopLayoutMotion();
    const scrollLeft = byId('stage').dataset.scene === scene.id ? byId('stage').querySelector('.hand')?.scrollLeft ?? 0 : 0;
    clearMotion(); historyObserver?.disconnect(); refreshHistory = () => {}; document.body.classList.toggle('has-sequence', scene.kind === 'sequence');
    updateNavigation();
    // Keep the one map panel alive while replacing a route board around it.
    if (byId('stage').contains(mapPanel)) byId('map-home').append(mapPanel);
    byId('stage').replaceChildren();
    if (scene.kind === 'sequence') renderSequence(scene, motion, scrollLeft); else if (scene.kind === 'combat') renderCombat(scene); else if (scene.kind === 'event') renderEvent(scene); else renderRewards(scene);
    byId('stage').dataset.scene = scene.id;
    byId('stage').dataset.phase = scene.kind === 'sequence' ? scene.snapshots[actionCursor].kind : scene.kind;
    const moved = renderMap(scene); updateMapLayout(beforeMap, moved);
    const manual = scene.kind === 'sequence' && scene.provenance.operator === 'developer';
    byId('raw-record').hidden = manual; byId('record-model').hidden = manual;
    const recordKey = JSON.stringify([scene.id, display.locale]);
    if (!manual && rawRecordKey !== recordKey) { renderRaw(scene); rawRecordKey = recordKey; }
  }
  function writeLocation() {
    const url = new URL(location.href);
    if (actionCursor) url.searchParams.set('step', actionCursor); else url.searchParams.delete('step');
    url.hash = scenes[selected].id;
    history.replaceState(null, '', url);
  }
  function select(index, updateHash = true, cursor = 0) {
    if (index < 0 || index >= scenes.length) return;
    if (selected !== index || actionCursor !== cursor) { mapExpanded = false; mapSelection = null; }
    selected = index; actionCursor = cursor; sequenceDetailsOpen = false; openPiles.clear(); turnRecordOpen = true; byId('raw-record').open = false;
    render();
    if (updateHash) writeLocation();
  }
  byId('brand-mark').append(icon('cards')); byId('run-icon').append(icon('swords')); byId('crumb-arrow').append(icon('chevron-right'));
  byId('map-close-icon').append(icon('chevron-right')); byId('map-selection-close-icon').append(icon('x'));
  byId('map-expand-icon').append(icon('expand'));
  byId('map-close').addEventListener('click', closeMap);
  byId('map-expand').addEventListener('click', () => {
    mapExpanded = true; const before = mapBeforeLayout(); updateMapLayout(before);
    mapPanel.querySelector('.map-viewport')?.focus({ preventScroll: true });
  });
  byId('map-selection-close').addEventListener('click', () => dismissMapSelection(true));
  byId('map-follow').addEventListener('click', () => {
    if (!mapSelection) return;
    const { target } = mapSelection;
    mapSelection = null; select(scenes.findIndex(scene => scene.id === target.scene_id), true, target.cursor);
  });
  byId('recording').addEventListener('change', event => {
    const recording = journeys.find(recording => recording.id === event.currentTarget.value);
    mapExpanded = false; mapSelection = null; select(scenes.findIndex(scene => scene.id === recording.scene_ids[0]));
  });
  window.addEventListener('resize', () => { updateMapLayout(); if (visibleMap(mapMode)) positionMap(); refreshHistory(); });
  window.addEventListener('keydown', event => {
    if (scenes[selected].kind !== 'sequence' || event.defaultPrevented || event.isComposing || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey
        || event.target.closest?.('input, select, textarea, [contenteditable]:not([contenteditable="false"]), [role="slider"], [role="listbox"], [role="combobox"], [role="tablist"]')
        || [...document.querySelectorAll('dialog[open], [role="dialog"][aria-modal="true"], [role="menu"], [role="listbox"]')].some(element => element.getClientRects().length > 0)
        || window.getSelection()?.isCollapsed === false) return;
    if (event.key === 'Escape' && mapSelection) { event.preventDefault(); dismissMapSelection(true); return; }
    if (event.key === 'Escape' && ['side', 'expanded'].includes(mapMode)) { event.preventDefault(); closeMap(); return; }
    const offset = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
    if (!offset) return;
    event.preventDefault();
    const focus = byId('stage').contains(document.activeElement) ? '#sequence-progress' : null;
    selectAction(actionCursor + offset, focus);
  });
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
  window.addEventListener('hashchange', () => { const index = scenes.findIndex(scene => scene.id === location.hash.slice(1)); if (index >= 0) select(index, false, locationCursor(index)); });
  renderChrome(); renderNavigation(); render();
}
main().catch(error => console.error(error));
