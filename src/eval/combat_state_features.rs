//! Deterministic semantic features for offline combat-state ranking.
//!
//! Unlike the legacy guide-component vector, this representation retains
//! concrete card-zone contents and order. It deliberately excludes card
//! lineages and raw RNG words: those are exact replay identities, not reusable
//! semantics.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::content::monsters::EnemyId;
use crate::runtime::combat::{CardPileView, CombatCard, PowerPayload};
use crate::sim::combat::CombatPosition;

pub const COMBAT_STATE_FEATURE_SCHEMA_V1: &str = "semantic-combat-state/v3";

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct CombatStateFeatureV1 {
    pub name: String,
    pub value: i32,
}

/// Produces sparse, deterministic facts suitable for an offline state or
/// frontier-node ranker. A missing numeric fact is equivalent to zero.
pub fn semantic_combat_state_features_v1(position: &CombatPosition) -> Vec<CombatStateFeatureV1> {
    let mut features = BTreeMap::<String, i32>::new();
    let combat = &position.combat;
    let player = &combat.entities.player;

    add(&mut features, "turn/count", combat.turn.turn_count as i32);
    add(&mut features, "turn/energy", i32::from(combat.turn.energy));
    add(
        &mut features,
        "turn/draw_modifier",
        combat.turn.turn_start_draw_modifier,
    );
    categorical(
        &mut features,
        format!(
            "turn/phase/{}",
            combat.turn.current_phase.vocabulary_token_v1()
        ),
    );
    add(
        &mut features,
        "turn/cards_played",
        i32::from(combat.turn.counters.cards_played_this_turn),
    );
    add(
        &mut features,
        "turn/attacks_played",
        i32::from(combat.turn.counters.attacks_played_this_turn),
    );
    add(
        &mut features,
        "turn/cards_discarded",
        i32::from(combat.turn.counters.cards_discarded_this_turn),
    );

    add(&mut features, "player/current_hp", player.current_hp);
    add(&mut features, "player/max_hp", player.max_hp);
    add(&mut features, "player/block", player.block);
    add(
        &mut features,
        "player/energy_master",
        i32::from(player.energy_master),
    );
    add(&mut features, "player/max_orbs", i32::from(player.max_orbs));
    categorical(
        &mut features,
        format!("player/stance/{}", player.stance.stable_id()),
    );

    add_card_zone(
        &mut features,
        "draw",
        CardPileView::Contiguous(combat.zones.draw_pile.as_ref()),
    );
    add_card_zone(
        &mut features,
        "hand",
        CardPileView::Contiguous(&combat.zones.hand),
    );
    add_card_zone(
        &mut features,
        "discard",
        CardPileView::Discard(&combat.zones.discard_pile),
    );
    add_card_zone(
        &mut features,
        "exhaust",
        CardPileView::Contiguous(combat.zones.exhaust_pile.as_slice()),
    );
    add_card_zone(
        &mut features,
        "limbo",
        CardPileView::Contiguous(&combat.zones.limbo),
    );

    for (index, relic) in player.relics.iter().enumerate() {
        let prefix = format!("relic/{index}/{}", relic.id.stable_id());
        categorical(&mut features, prefix.clone());
        add(&mut features, format!("{prefix}/counter"), relic.counter);
        add(&mut features, format!("{prefix}/amount"), relic.amount);
        if relic.used_up {
            categorical(&mut features, format!("{prefix}/used_up"));
        }
    }
    for (index, potion) in combat.entities.potions.iter().enumerate() {
        let Some(potion) = potion else {
            categorical(&mut features, format!("potion/{index}/empty"));
            continue;
        };
        let prefix = format!("potion/{index}/{}", potion.id.stable_id());
        categorical(&mut features, prefix.clone());
        if potion.can_use {
            categorical(&mut features, format!("{prefix}/can_use"));
        }
        if potion.can_discard {
            categorical(&mut features, format!("{prefix}/can_discard"));
        }
        if potion.requires_target {
            categorical(&mut features, format!("{prefix}/requires_target"));
        }
    }

    for monster in &combat.entities.monsters {
        let enemy = monster.enemy_id.stable_id();
        let prefix = format!("monster/{}/{enemy}", monster.slot);
        categorical(&mut features, prefix.clone());
        add(
            &mut features,
            format!("{prefix}/current_hp"),
            monster.current_hp,
        );
        add(&mut features, format!("{prefix}/max_hp"), monster.max_hp);
        add(&mut features, format!("{prefix}/block"), monster.block);
        add(
            &mut features,
            format!("{prefix}/planned_move"),
            i32::from(monster.planned_move_id()),
        );
        if monster.is_dying {
            categorical(&mut features, format!("{prefix}/dying"));
        }
        if monster.is_escaped {
            categorical(&mut features, format!("{prefix}/escaped"));
        }
        if monster.half_dead {
            categorical(&mut features, format!("{prefix}/half_dead"));
        }
        for (history_index, move_id) in monster.move_history().iter().enumerate() {
            add(
                &mut features,
                format!("{prefix}/move_history/{history_index}"),
                i32::from(*move_id),
            );
        }
        let protocol = combat.monster_protocol(monster.id);
        if let Some(protocol) = protocol {
            categorical(
                &mut features,
                format!(
                    "{prefix}/visible_intent/{}",
                    protocol.observation.visible_intent.kind_v1()
                ),
            );
            add(
                &mut features,
                format!("{prefix}/preview_damage_per_hit"),
                protocol.observation.preview_damage_per_hit,
            );
        }
        if matches!(monster.enemy_id, EnemyId::AwakenedOne) {
            if monster.awakened_one.form1 {
                categorical(&mut features, format!("{prefix}/phase/form1"));
            } else {
                categorical(&mut features, format!("{prefix}/phase/form2"));
            }
            if monster.awakened_one.first_turn {
                categorical(&mut features, format!("{prefix}/first_turn"));
            }
        }
    }

    let mut entity_labels = BTreeMap::new();
    entity_labels.insert(player.id, "player".to_string());
    for monster in &combat.entities.monsters {
        entity_labels.insert(monster.id, format!("monster/{}", monster.slot));
    }
    for (entity, powers) in &combat.entities.power_db {
        let entity = entity_labels
            .get(entity)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        for power in powers {
            let prefix = format!("entity/{entity}/power/{}", power.power_type.stable_id());
            add(&mut features, format!("{prefix}/amount"), power.amount);
            add(
                &mut features,
                format!("{prefix}/extra_data"),
                power.extra_data,
            );
            if power.just_applied {
                categorical(&mut features, format!("{prefix}/just_applied"));
            }
            if let PowerPayload::Card(card) = &power.payload {
                categorical(
                    &mut features,
                    format!("{prefix}/payload/{}", semantic_card_id(card)),
                );
            }
        }
    }

    features
        .into_iter()
        .filter(|(_, value)| *value != 0)
        .map(|(name, value)| CombatStateFeatureV1 { name, value })
        .collect()
}

fn add_card_zone(features: &mut BTreeMap<String, i32>, zone: &str, cards: CardPileView<'_>) {
    add(
        features,
        format!("zone/{zone}/count"),
        i32::try_from(cards.len()).unwrap_or(i32::MAX),
    );
    for (index, card) in cards.iter().enumerate() {
        let card_id = semantic_card_id(card);
        categorical(
            features,
            format!("zone/{zone}/position/{index}/card/{card_id}"),
        );
        add(features, format!("zone/{zone}/card/{card_id}/count"), 1);
        let prefix = format!("zone/{zone}/position/{index}");
        add(
            features,
            format!("{prefix}/cost_modifier"),
            i32::from(card.cost_modifier),
        );
        if let Some(cost) = card.cost_for_turn {
            add(features, format!("{prefix}/cost_for_turn"), i32::from(cost));
        }
        add(features, format!("{prefix}/misc"), card.misc_value);
        add(
            features,
            format!("{prefix}/base_damage_mut"),
            card.base_damage_mut,
        );
        add(
            features,
            format!("{prefix}/base_block_mut"),
            card.base_block_mut,
        );
        add(
            features,
            format!("{prefix}/base_magic_mut"),
            card.base_magic_num_mut,
        );
        add(
            features,
            format!("{prefix}/energy_on_use"),
            card.energy_on_use,
        );
        if card.free_to_play_once {
            categorical(features, format!("{prefix}/free_to_play_once"));
        }
        if let Some(exhaust) = card.exhaust_override {
            categorical(features, format!("{prefix}/exhaust_override/{exhaust}"));
        }
        if let Some(retain) = card.retain_override {
            categorical(features, format!("{prefix}/retain_override/{retain}"));
        }
    }
}

fn semantic_card_id(card: &CombatCard) -> String {
    format!("{}+{}", card.id.stable_id(), card.upgrades)
}

fn add(features: &mut BTreeMap<String, i32>, name: impl Into<String>, value: i32) {
    if value != 0 {
        *features.entry(name.into()).or_default() += value;
    }
}

fn categorical(features: &mut BTreeMap<String, i32>, name: impl Into<String>) {
    add(features, name, 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::runtime::combat::CombatCard;
    use crate::state::core::EngineState;
    use crate::testing::support::blank_test_combat;

    #[test]
    fn card_lineage_does_not_change_semantic_features() {
        let mut left = blank_test_combat();
        left.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::WildStrike, 1),
            CombatCard::for_scenario(CardId::Havoc, 2),
        ])
        .into();
        let mut right = left.clone();
        let mut right_draw = std::mem::take(&mut right.zones.draw_pile).into_vec();
        right_draw[0].lineage = crate::test_support::scenario_lineage(91);
        right_draw[1].lineage = crate::test_support::scenario_lineage(92);
        right.zones.draw_pile = right_draw.into();
        assert_eq!(
            semantic_combat_state_features_v1(&CombatPosition::new(
                EngineState::CombatPlayerTurn,
                left
            )),
            semantic_combat_state_features_v1(&CombatPosition::new(
                EngineState::CombatPlayerTurn,
                right
            ))
        );
    }

    #[test]
    fn draw_pile_order_changes_semantic_features() {
        let mut left = blank_test_combat();
        left.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::WildStrike, 1),
            CombatCard::for_scenario(CardId::Havoc, 2),
        ])
        .into();
        let mut right = left.clone();
        right.zones.draw_pile.swap(0, 1);
        assert_ne!(
            semantic_combat_state_features_v1(&CombatPosition::new(
                EngineState::CombatPlayerTurn,
                left
            )),
            semantic_combat_state_features_v1(&CombatPosition::new(
                EngineState::CombatPlayerTurn,
                right
            ))
        );
    }
}
