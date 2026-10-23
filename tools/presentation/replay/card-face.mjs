import original from '../original-card-text.json' with { type: 'json' };
import { catalogs } from '../catalogs.mjs';

// Template selection and definition values are captured from the native owner.
// Combat values are already public evaluated fields, not recomputed rules.
export function cardFaceView(card, definitions, combat = false) {
  const captured = definitions.find(fact => fact.kind === 'card' && fact.definition.current.card === card.card_id);
  const facts = card.upgrades === 0 ? captured?.definition.current : card.upgrades === 1 ? captured?.definition.upgraded : null;
  const spec = captured?.face_templates?.find(face => face.upgrades === card.upgrades);
  if (!facts || !spec) throw new Error(`Missing captured card face: ${card.card_id}+${card.upgrades}`);
  const values = combat ? { D: card.damage_by_monster_order?.length === 1 ? card.damage_by_monster_order[0] : card.current_damage,
    B: card.current_block, M: card.current_magic_number } : { D: spec.damage, B: spec.block, M: spec.magic };
  const printed_lines = {};
  for (const locale of ['en', 'zh-CN']) {
    let text = original.cards[locale][card.card_id]?.[spec.field];
    if (typeof text !== 'string' || !text.trim() || text.toLowerCase() === 'deprecated') throw new Error(`Missing original card text: ${card.card_id}:${spec.field}`);
    text = text.replace(/!([A-Za-z0-9]+)!/g, (_token, field) => {
      if (!Number.isSafeInteger(values[field])) throw new Error(`Missing captured card value: ${card.card_id}:${field}`);
      return String(values[field]);
    }).replace(/#[brgyp]/g, '').replace(/\*(?=\p{L})/gu, '').replace(/\[[RGBWE]\]/g, catalogs[locale].mechanics.card.energy);
    const lines = [''];
    for (const token of text.trim().split(/\s+/)) {
      if (token === 'NL') lines.push(''); else lines[lines.length - 1] += (lines.at(-1) ? ' ' : '') + token;
    }
    printed_lines[locale] = lines.map(line => locale === 'en' ? line : line.replace(/ ([。，、；：！？）])/g, '$1')
      .replace(/(?<=[\u3400-\u9fff]) (?=[\u3400-\u9fff])/g, '')).filter(Boolean);
  }
  const cost = combat ? card.effective_cost : facts.cost;
  return { card_id: card.card_id, upgrades: card.upgrades, type: facts.card_type.toLowerCase(),
    cost: cost === -1 ? 'X' : cost < 0 ? null : cost, printed_lines };
}
