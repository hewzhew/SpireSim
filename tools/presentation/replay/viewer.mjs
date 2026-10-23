import { createPresentation, preferredLocale } from '../index.mjs';
import { neowOptionRuns } from './neow-option.mjs';
import { intentView } from './intent-view.mjs';
import { handTransition, sequenceView } from './sequence.mjs';

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
  if (!location.hash) {
    const model = scenes.findIndex(scene => scene.kind === 'sequence' && scene.provenance.operator === 'model');
    selected = model >= 0 ? model : Math.min(1, scenes.length - 1);
  }
  let rawTab = 'calls';
  let turnRecordOpen = true;
  let actionCursor = 0;
  let sequenceDetailsOpen = false;
  let historyObserver;
  const activeMotion = new Set();
  const openPiles = new Set();
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
  const cardFrame = {
    attack: 'M4 4H96V73L50 96L4 73Z',
    skill: 'M4 4H96V72Q96 96 50 96Q4 96 4 72Z',
    curse: 'M14 4H86L96 16V84L86 96H14L4 84V16Z',
  };
  const effectKeys = { damage: 'damage', block: 'block', apply: 'apply', draw: 'draw', unplayable: 'unplayable', ethereal: 'ethereal', double_block: 'doubleBlock', random_damage: 'randomDamage' };
  function gameCard(card, chosen = false) {
    const el = node('article', undefined, 'playing-card ' + card.type + (chosen ? ' chosen' : ''));
    el.dataset.card = card.card_id;
    const top = node('div', undefined, 'card-topline');
    const cost = node('span', card.cost === null ? undefined : number(card.cost), 'card-cost');
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
  function combatBoard(before, heading, playback) {
    const board = node('section', undefined, 'combat-board'); board.setAttribute('aria-label', heading);
    if (playback) board.classList.add('sequence-board');
    const hud = node('div', undefined, 'combat-hud'); hud.append(resources(before));
    const field = node('div', undefined, 'battlefield');
    const enemyDetails = [];
    for (const power of before.enemy.powers) enemyDetails.push(node('span', t('combat.power', { power: name('power', power.power_id), amount: power.amount }), 'status'));
    field.append(
      fighter(name('character', story.character_id), before.hp, before.max_hp, { block: before.block,
        details: (before.powers ?? []).map(power => node('span', t('combat.power', { power: name('power', power.power_id), amount: power.amount }), 'status')) }),
      fighter(name('enemy', before.enemy.enemy_id), before.enemy.hp, before.enemy.max_hp, { enemy: true, block: before.enemy.block, details: enemyDetails, intent: before.enemy.intent,
        figureIcon: before.enemy.enemy_id === 'Cultist' ? 'flame' : 'bug' }),
    );
    const dock = node('div', undefined, 'combat-dock');
    const energy = node('div', undefined, 'energy-reserve'); energy.setAttribute('aria-label', t('combat.energy', { energy: before.energy }));
    energy.append(node('strong', number(before.energy), 'energy-orb'), node('span', t('combat.energyLabel'), 'energy-label'));
    const hand = node('div', undefined, 'hand'); for (const card of before.hand) hand.append(gameCard(card));
    hand.setAttribute('role', 'group'); hand.setAttribute('aria-label', t('combat.hand'));
    dock.append(energy, hand); board.append(hud, field); if (playback) board.append(playback); board.append(dock);
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
    if (action.kind === 'claim_reward') return t('sequence.claimGold', { amount: action.amount });
    if (action.kind === 'open_card_reward') return t('sequence.openCards');
    if (action.kind === 'skip_card_reward') return t('sequence.skipCards');
    if (action.kind === 'proceed') return t('sequence.returnToMap');
    const card = display.cardName(action.card.card_id, action.card.upgrades);
    if (action.kind === 'take_card') return t('sequence.takeCard', { card });
    return current && action.target_enemy_id ? t('sequence.targetedCard', { card, target: name('enemy', action.target_enemy_id) }) : card;
  }
  function postCombatBoard(state, playback, completed) {
    const board = node('section', undefined, 'combat-board sequence-board');
    const hud = node('div', undefined, 'combat-hud'); hud.append(resources(state));
    const result = node('div', undefined, 'battle-outcome');
    result.append(node('h2', t(state.kind === 'map' ? 'sequence.mapReached' : state.kind === 'card_reward' ? 'sequence.chooseCard' : 'sequence.victory'), 'outcome-title'));
    if (state.kind === 'card_reward') {
      result.classList.add('card-reward-outcome');
      const cards = node('div', undefined, 'reward-cards');
      for (const card of state.reward_cards) cards.append(gameCard(card));
      result.append(cards);
    } else {
      const panel = node('section', undefined, 'combat-reward-panel');
      if (state.kind === 'reward') panel.append(node('h3', t('scene.rewards')));
      const rewards = node('ul', undefined, 'combat-rewards');
      const rewardRow = (glyph, label, claimed) => {
        const row = node('li', undefined, 'combat-reward' + (claimed ? ' is-claimed' : ''));
        const symbol = node('span', undefined, 'reward-symbol'); symbol.append(icon(glyph));
        row.append(symbol, node('span', label));
        if (claimed) { const mark = icon('check'); mark.classList.add('reward-claimed'); mark.setAttribute('role', 'img'); mark.setAttribute('aria-hidden', 'false'); mark.setAttribute('aria-label', t('sequence.claimed')); row.append(mark); }
        rewards.append(row);
      };
      const claimedGold = completed.filter(action => action.kind === 'claim_reward').reduce((total, action) => total + action.amount, 0);
      if (state.reward_gold || claimedGold) rewardRow('coin', t('sequence.goldReward', { amount: state.reward_gold || claimedGold }), !state.reward_gold);
      if (state.has_card_reward) rewardRow('cards', t('sequence.addCard'), false);
      for (const action of completed.filter(action => action.kind === 'take_card')) rewardRow('cards', t('sequence.takeCard', { card: display.cardName(action.card.card_id, action.card.upgrades) }), true);
      panel.append(rewards); result.append(panel);
    }
    board.append(hud, result, playback); return board;
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
      : t(state.kind === 'map' ? 'sequence.mapReached' : 'scene.rewards'), 'sequence-turn'));
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
      const button = node('button', undefined, 'history-scroll'); button.type = 'button'; button.hidden = true;
      button.setAttribute('aria-label', t(direction < 0 ? 'sequence.earlierActions' : 'sequence.laterActions'));
      button.append(icon(direction < 0 ? 'chevron-left' : 'chevron-right'));
      button.addEventListener('click', () => {
        actions.scrollLeft = Math.max(0, Math.min(actions.scrollWidth - actions.clientWidth, actions.scrollLeft + direction * Math.max(120, actions.clientWidth * .7)));
        updateHistory();
      });
      return button;
    });
    const updateHistory = () => {
      const overflow = actions.scrollWidth > historyWindow.clientWidth + 1;
      for (const button of scrollButtons) button.hidden = !overflow;
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
      if (view.deltas.enemy_hp) changes.append(node('span', t('sequence.change', { label: name('enemy', state.enemy.enemy_id), before: previous.enemy.hp, after: state.enemy.hp })));
      if (view.added.length) changes.append(node('span', t('sequence.addedHand', { cards: view.added.map(cardGroupLabel).join(display.locale === 'zh-CN' ? '、' : ', ') }), 'sequence-new-cards'));
      details.append(changes); record.append(details);
    }
    const board = state.kind === 'combat' ? combatBoard(state, title(scene), header) : postCombatBoard(state, header, view.completed);
    for (const index of motion?.hand?.added ?? []) board.querySelector('.hand').children[index].classList.add('is-arriving');
    byId('stage').append(board, record);
    const hand = board.querySelector('.hand'); if (hand) hand.scrollLeft = scrollLeft;
    const arriving = board.querySelector('.is-arriving');
    if (arriving) {
      hand.scrollLeft += Math.max(0, arriving.getBoundingClientRect().right - hand.getBoundingClientRect().right + 6);
    }
    const current = actions.querySelector('[aria-current]');
    updateHistory();
    actions.scrollLeft = Math.min(Math.max(0, actions.scrollWidth - actions.clientWidth),
      actions.scrollLeft + Math.max(0, current.getBoundingClientRect().right - actions.getBoundingClientRect().right + 4));
    updateHistory();
    if (typeof ResizeObserver === 'function') {
      historyObserver = new ResizeObserver(updateHistory); historyObserver.observe(actions);
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
    actionCursor = index; render(motion);
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
  function title(scene) { return scene.kind === 'sequence' ? t(scene.provenance.operator === 'model' ? 'sequence.modelTitle' : 'sequence.title') : scene.kind === 'event' ? name('event', scene.event_id) : scene.kind === 'combat' ? name('encounter', scene.encounter_id) : t('scene.rewards'); }
  function label(scene) { return scene.kind === 'sequence' ? scene.provenance.operator === 'model'
    ? t('sequence.preparedBattle', { encounter: name('encounter', scene.encounter_id) }) : t('sequence.manual')
    : scene.kind === 'event' ? t('scene.start') : scene.kind === 'combat' ? t('scene.turn', { turn: scene.turn }) : t('scene.floor', { floor: scene.floor }); }
  function renderNavigation() {
    byId('timeline').replaceChildren();
    scenes.forEach((scene, index) => {
      const button = node('button', undefined, 'scene-link'); button.type = 'button'; button.dataset.scene = scene.id;
      const text = node('span', undefined, 'scene-text'); text.append(node('strong', scene.kind === 'combat' ? label(scene) : title(scene)), node('small', scene.kind === 'sequence' ? label(scene) : scene.kind === 'event' ? t('scene.initialReward') : scene.kind === 'combat' ? title(scene) : name('card', scene.chosen_card_id)));
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
  function render(motion = null) {
    const scene = scenes[selected]; byId('heading').textContent = title(scene); byId('turn-label').textContent = label(scene);
    const scrollLeft = byId('stage').dataset.scene === scene.id ? byId('stage').querySelector('.hand')?.scrollLeft ?? 0 : 0;
    clearMotion(); historyObserver?.disconnect(); document.body.classList.toggle('has-sequence', scene.kind === 'sequence');
    for (const [index, button] of [...byId('timeline').children].entries()) { if (index === selected) button.setAttribute('aria-current', 'step'); else button.removeAttribute('aria-current'); }
    byId('stage').replaceChildren();
    if (scene.kind === 'sequence') renderSequence(scene, motion, scrollLeft); else if (scene.kind === 'combat') renderCombat(scene); else if (scene.kind === 'event') renderEvent(scene); else renderRewards(scene);
    byId('stage').dataset.scene = scene.id;
    const manual = scene.kind === 'sequence' && scene.provenance.operator === 'developer';
    byId('raw-record').hidden = manual; byId('record-model').hidden = manual;
    if (!manual) renderRaw(scene);
  }
  function select(index, updateHash = true) {
    if (index < 0 || index >= scenes.length) return;
    selected = index; actionCursor = 0; sequenceDetailsOpen = false; openPiles.clear(); turnRecordOpen = true; byId('raw-record').open = false; render();
    if (updateHash) history.replaceState(null, '', '#' + scenes[selected].id);
  }
  byId('brand-mark').append(icon('cards')); byId('run-icon').append(icon('swords')); byId('crumb-arrow').append(icon('chevron-right'));
  window.addEventListener('keydown', event => {
    if (scenes[selected].kind !== 'sequence' || event.defaultPrevented || event.isComposing || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey
        || event.target.closest?.('input, select, textarea, [contenteditable]:not([contenteditable="false"]), [role="slider"], [role="listbox"], [role="combobox"], [role="tablist"]')
        || [...document.querySelectorAll('dialog[open], [role="dialog"][aria-modal="true"], [role="menu"], [role="listbox"]')].some(element => element.getClientRects().length > 0)
        || window.getSelection()?.isCollapsed === false) return;
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
  window.addEventListener('hashchange', () => { const index = scenes.findIndex(scene => scene.id === location.hash.slice(1)); if (index >= 0) select(index, false); });
  renderChrome(); renderNavigation(); render();
}
main().catch(error => console.error(error));
