import original from '../original-card-text.json' with { type: 'json' };

// These recorded offers are fresh, unupgraded cards. Bind their printed
// placeholders to captured definition facts, never to combat-modified values.
const magicField = { ThunderClap: 'vulnerable', Carnage: null, Warcry: 'draw_cards' };
export function rewardCardView(card, definitions) {
  if (!Object.hasOwn(magicField, card.card_id) || card.upgrades !== 0) throw new Error(`Unbound reward card: ${card.card_id}+${card.upgrades}`);
  const facts = definitions.find(fact => fact.kind === 'card' && fact.definition.current.card === card.card_id)?.definition.current;
  if (!facts || !Number.isSafeInteger(facts.cost)) throw new Error(`Missing captured reward definition: ${card.card_id}`);
  const values = { D: facts.damage?.damage_per_hit, B: facts.block, M: facts[magicField[card.card_id]] };
  const printed_lines = {};
  for (const locale of ['en', 'zh-CN']) {
    const template = original.cards[locale][card.card_id].DESCRIPTION;
    const text = template.replace(/!([DBM])!/g, (_token, field) => {
      if (!Number.isSafeInteger(values[field])) throw new Error(`Missing printed reward value: ${card.card_id}:${field}`);
      return String(values[field]);
    });
    if (/![A-Z]!|#[a-z]/.test(text)) throw new Error(`Unbound printed reward markup: ${card.card_id}`);
    // Match presentation/src/card_text.rs: original Chinese keyword spacing
    // serves the game's markup and is removed in a plain text card face.
    printed_lines[locale] = text.split(/\s+NL\s+/).map(line => {
      const plain = line.trim().replace(/\s+/g, ' ');
      return locale === 'en' ? plain : plain.replace(/ ([。，、；：！？）])/g, '$1').replace(/(?<=[\u3400-\u9fff]) (?=[\u3400-\u9fff])/g, '');
    }).filter(Boolean);
  }
  return { card_id: card.card_id, upgrades: card.upgrades, type: facts.card_type.toLowerCase(), cost: facts.cost, printed_lines };
}
