//! Held-potion facts, not scores or hypothetical generated cards.
use super::{EntityPosition, WIDTH};
use sts_core::content::potions::{
    get_potion_definition, potion_can_use_in_combat_like_java, potion_effects::card_discovery_spec,
    potion_mechanic_roles, potion_potency_in_combat,
};
use sts_core::runtime::combat::CombatState;

pub(super) fn encode(p: &mut EntityPosition, combat: &CombatState) -> Vec<i64> {
    combat
        .entities
        .potions
        .iter()
        .enumerate()
        .map(|(slot, potion)| {
            let mut v = [0.; WIDTH];
            v[0] = slot as f32;
            v[1] = f32::from(potion.is_some());
            if let Some(potion) = potion {
                let definition = get_potion_definition(potion.id);
                let potency = potion_potency_in_combat(potion.id, combat);
                v[2] = potency as f32 / 20.;
                v[3] = definition.base_potency as f32 / 20.;
                v[4] = f32::from(potion_can_use_in_combat_like_java(potion, combat));
                v[5] = f32::from(potion.can_discard);
                v[6] = f32::from(potion.requires_target);
                v[7] = f32::from(definition.is_thrown);
                if let Some(discovery) = card_discovery_spec(potion.id) {
                    v[8] = 1.;
                    v[9] = f32::from(discovery.colorless);
                    super::optional(&mut v, 10, discovery.card_type.map(|t| t as i32), 1.);
                    v[12] = potency.max(0) as f32; // selected copies, not offered options
                    super::optional(&mut v, 13, discovery.cost_for_turn.map(i32::from), 1.);
                    v[15] = f32::from(discovery.can_skip);
                    v[16] = sts_core::runtime::action::DISCOVERY_CARD_CHOICE_COUNT as f32;
                }
            }
            let row = p.push(
                6,
                potion
                    .as_ref()
                    .map_or(0, |q| i64::from(q.id.stable_ordinal()) + 1),
                v,
                [0, -1, -1],
            );
            if let Some(potion) = potion {
                for role in potion_mechanic_roles(potion.id) {
                    // The same role category connects different potion identities.
                    p.push(22, *role as i64, [0.; WIDTH], [row, 0, -1]);
                }
            }
            row
        })
        .collect()
}
