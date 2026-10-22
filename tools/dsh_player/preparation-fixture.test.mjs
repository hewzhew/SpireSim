// Test DTOs only. Text is deliberately opaque and independent of headers;
// mechanics in each test's public facts remain the policy input.
export function preparationFixture(observation, { overview = '当前公开局面', selections = '', routes = '',
  candidateViews = {}, cardViews = {}, relicViews = {}, stockViews = {}, cardRuleViews = {}, stockLabels = {}, resources: suppliedResources } = {}) {
  const state = observation.position.observation.observation;
  const cards = (observation.deck_evidence?.cards?.map(row => row.card) ?? state.cards ?? []).map(card => ({
    card: card.card, upgrades: card.upgrades ?? 0, count: card.count ?? 1,
    view: cardViews[card.card] ?? `卡牌 ${card.card} 的完整原生规则。`,
  }));
  const relics = (state.relics ?? []).map(row => ({ relic: row.relic,
    view: relicViews[row.relic] ?? `遗物 ${row.relic} 的完整原生规则。` }));
  const candidates = (observation.position.actions ?? []).filter(row => row.semantic_action).map(row => {
    const action = row.candidate.action;
    const acquired = action.card ? [{ card: action.card, upgrades: action.upgrades ?? 0 }]
      : observation.decision_evidence?.candidates?.find(item => item.semantic_action === row.semantic_action)?.cards?.map(item => item.current) ?? [];
    return { command: row.semantic_action, label: `当前选择 ${row.semantic_action}`, view: candidateViews[row.semantic_action] ?? '完整效果。',
      card_rules: acquired.map(card => ({ card: card.card, upgrades: card.upgrades ?? 0, available: true,
        view: cardRuleViews[card.card] ?? `Card ${card.card}\nPrinted rule: supplied ${card.card} rule.` })) };
  });
  const inventory = state.context?.inventory;
  const stock = ['card', 'relic', 'potion'].flatMap(kind => (inventory?.[`${kind}s`] ?? []).map(item => {
    const action = { kind: `buy_${kind}`, shop_slot: item.slot, [kind]: item[kind], price: item.price,
      ...(kind === 'card' ? { upgrades: item.upgrades ?? 0 } : {}) };
    const command = (observation.position.actions ?? []).find(row => row.candidate.action.kind === action.kind
      && row.candidate.action.shop_slot === item.slot)?.semantic_action ?? null;
    const label = stockLabels[item[kind]] ?? `${item[kind]} ${item.price}g`;
    const rules = kind === 'card' ? [{ card: item.card, upgrades: item.upgrades ?? 0, available: true,
      view: `完整的 ${item.card} 规则。` }] : [];
    return { action, command, label, view: stockViews[item[kind]] ?? `${label}\n完整的商品规则。`, card_rules: rules };
  }));
  const resources = suppliedResources ?? [...relics.map(row => row.view), '当前公开药水及规则。'].join('\n');
  return { schema: 'sts-preparation-reading-v1', public_snapshot_id: observation.position.public_snapshot_id,
    overview, selections, routes, candidates, stock,
    inventory: { cards, relics, resources, view: [...cards.map(row => row.view), resources].join('\n') } };
}
