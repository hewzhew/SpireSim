use crate::content::cards;
use crate::runtime::combat::CombatState;
use crate::state::core::ClientInput;

use super::types::CombatSearchV2ActionCardFacts;

pub(super) fn card_facts(
    combat: &CombatState,
    input: &ClientInput,
) -> Option<CombatSearchV2ActionCardFacts> {
    let ClientInput::PlayCard { card_index, target } = *input else {
        return None;
    };
    let card = combat.zones.hand.get(card_index)?;
    let def = cards::get_card_definition(card.id);
    let evaluated = cards::evaluate_card_for_play(card, combat, target);

    Some(CombatSearchV2ActionCardFacts {
        hand_index: card_index,
        lineage: card.lineage,
        card_id: card.id.stable_id().to_string(),
        name: def.name,
        upgraded: card.upgrades > 0,
        card_type: def.card_type,
        definition_target: def.target,
        effective_target: cards::effective_target(card),
        cost_for_turn: card.cost_for_turn_java(),
        base_cost: def.cost,
        evaluated_damage: evaluated.base_damage_mut.max(0),
        evaluated_block: evaluated.base_block_mut.max(0),
        evaluated_magic: evaluated.base_magic_num_mut.max(0),
        exhaust: card
            .exhaust_override
            .unwrap_or_else(|| cards::exhausts_when_played(card)),
        ethereal: cards::is_ethereal(card),
        innate: def.innate,
    })
}
