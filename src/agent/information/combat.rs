//! Public combat information visible to a deployed agent.

use serde::{Deserialize, Serialize};

use crate::content::relics::RelicId;
use crate::runtime::combat::{CombatCard, CombatState, Intent};

pub const OBSERVATION_BOUNDARY_SCHEMA_NAME: &str = "CombatPublicObservationV2";
pub const OBSERVATION_BOUNDARY_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InformationAccessV1 {
    Public,
    PrivilegedSimulator,
    DebugRaw,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationEvidenceKindV1 {
    VisibleExact,
    PublicOrderedCollection,
    PublicUnorderedCollection,
    Hidden,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HiddenInformationReasonV1 {
    RunicDome,
    IntentNotVisible,
    DrawPileOrderHidden,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicObservationV2 {
    pub schema_name: String,
    pub schema_version: u32,
    pub information_access: InformationAccessV1,
    pub player: CombatPublicPlayerV1,
    pub hand: Vec<CombatPublicCardV2>,
    pub piles: CombatPublicPilesV2,
    pub potions: Vec<Option<CombatPublicPotionV1>>,
    pub monsters: Vec<CombatPublicMonsterV1>,
    pub hidden_reasons: Vec<HiddenInformationReasonV1>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicPlayerV1 {
    pub player_class: String,
    pub ascension_level: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub energy: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicCardV2 {
    pub card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicPilesV2 {
    pub draw: CombatPublicCardPileV2,
    pub discard_count: usize,
    pub exhaust_count: usize,
    pub limbo_count: usize,
    pub queued_cards_count: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicCardPileV2 {
    pub count: usize,
    pub evidence: ObservationEvidenceKindV1,
    pub hidden_reason: Option<HiddenInformationReasonV1>,
    pub cards: Vec<CombatPublicCardV2>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicPotionV1 {
    pub potion_id: String,
    pub can_use: bool,
    pub can_discard: bool,
    pub requires_target: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicMonsterV1 {
    pub slot: u8,
    pub enemy_id: String,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub escaped: bool,
    pub dying: bool,
    pub half_dead: bool,
    pub intent: CombatPublicIntentV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicIntentV1 {
    pub evidence: ObservationEvidenceKindV1,
    pub intent: Option<String>,
    pub preview_damage_per_hit: Option<i32>,
    pub hidden_reason: Option<HiddenInformationReasonV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatPublicIntentFactsV1 {
    pub evidence: ObservationEvidenceKindV1,
    pub intent: Option<Intent>,
    pub preview_damage_per_hit: Option<i32>,
    pub hidden_reason: Option<HiddenInformationReasonV1>,
}

pub fn combat_public_observation_v2(combat: &CombatState) -> CombatPublicObservationV2 {
    let draw_evidence = combat_public_draw_evidence_v1(combat);

    CombatPublicObservationV2 {
        schema_name: OBSERVATION_BOUNDARY_SCHEMA_NAME.to_string(),
        schema_version: OBSERVATION_BOUNDARY_SCHEMA_VERSION,
        information_access: InformationAccessV1::Public,
        player: combat_public_player_v1(combat),
        hand: combat.zones.hand.iter().map(public_card).collect(),
        piles: CombatPublicPilesV2 {
            draw: public_draw_pile(combat, draw_evidence),
            discard_count: combat.zones.discard_pile.len(),
            exhaust_count: combat.zones.exhaust_pile.len(),
            limbo_count: combat.zones.limbo.len(),
            queued_cards_count: combat.zones.queued_cards.len(),
        },
        potions: combat_public_potions_v1(combat),
        monsters: combat_public_monsters_v1(combat),
        hidden_reasons: combat_public_hidden_reasons_v1(combat),
    }
}

pub(crate) fn combat_public_player_v1(combat: &CombatState) -> CombatPublicPlayerV1 {
    CombatPublicPlayerV1 {
        player_class: combat.meta.player_class.clone(),
        ascension_level: combat.meta.ascension_level,
        hp: combat.entities.player.current_hp,
        max_hp: combat.entities.player.max_hp,
        block: combat.entities.player.block,
        energy: combat.turn.energy,
    }
}

pub(crate) fn combat_public_potions_v1(combat: &CombatState) -> Vec<Option<CombatPublicPotionV1>> {
    combat
        .entities
        .potions
        .iter()
        .map(|slot| {
            slot.as_ref().map(|potion| CombatPublicPotionV1 {
                potion_id: potion.id.stable_id().to_string(),
                can_use: potion.can_use,
                can_discard: potion.can_discard,
                requires_target: potion.requires_target,
            })
        })
        .collect()
}

pub(crate) fn combat_public_monsters_v1(combat: &CombatState) -> Vec<CombatPublicMonsterV1> {
    combat
        .entities
        .monsters
        .iter()
        .map(|monster| CombatPublicMonsterV1 {
            slot: monster.slot,
            enemy_id: monster.enemy_id.stable_id().to_owned(),
            hp: monster.current_hp,
            max_hp: monster.max_hp,
            block: monster.block,
            alive: monster.is_alive_for_action(),
            escaped: monster.is_escaped,
            dying: monster.is_dying,
            half_dead: monster.half_dead,
            intent: public_intent(combat, monster.id),
        })
        .collect()
}

pub(crate) fn combat_public_hidden_reasons_v1(
    combat: &CombatState,
) -> Vec<HiddenInformationReasonV1> {
    let mut hidden_reasons = Vec::new();
    if combat.entities.player.has_relic(RelicId::RunicDome) {
        hidden_reasons.push(HiddenInformationReasonV1::RunicDome);
    }
    if combat_public_draw_evidence_v1(combat)
        == ObservationEvidenceKindV1::PublicUnorderedCollection
        && !combat.zones.draw_pile.is_empty()
    {
        hidden_reasons.push(HiddenInformationReasonV1::DrawPileOrderHidden);
    }
    hidden_reasons
}

pub(crate) fn combat_public_draw_evidence_v1(combat: &CombatState) -> ObservationEvidenceKindV1 {
    if combat.entities.player.has_relic(RelicId::FrozenEye) {
        ObservationEvidenceKindV1::PublicOrderedCollection
    } else {
        ObservationEvidenceKindV1::PublicUnorderedCollection
    }
}

fn public_draw_pile(
    combat: &CombatState,
    evidence: ObservationEvidenceKindV1,
) -> CombatPublicCardPileV2 {
    let mut cards = combat
        .zones
        .draw_pile
        .iter()
        .map(public_card)
        .collect::<Vec<_>>();
    if evidence != ObservationEvidenceKindV1::PublicOrderedCollection {
        cards.sort_by(|a, b| {
            a.card_id
                .cmp(&b.card_id)
                .then(a.upgrades.cmp(&b.upgrades))
                .then(a.cost_for_turn.cmp(&b.cost_for_turn))
        });
    }
    CombatPublicCardPileV2 {
        count: combat.zones.draw_pile.len(),
        evidence,
        hidden_reason: (evidence == ObservationEvidenceKindV1::PublicUnorderedCollection
            && !combat.zones.draw_pile.is_empty())
        .then_some(HiddenInformationReasonV1::DrawPileOrderHidden),
        cards,
    }
}

fn public_intent(combat: &CombatState, monster_id: usize) -> CombatPublicIntentV1 {
    let facts = combat_public_intent_facts_v1(combat, monster_id);
    CombatPublicIntentV1 {
        evidence: facts.evidence,
        intent: facts.intent.map(|intent| intent.protocol_token_v1()),
        preview_damage_per_hit: facts.preview_damage_per_hit,
        hidden_reason: facts.hidden_reason,
    }
}

pub fn combat_public_intent_facts_v1(
    combat: &CombatState,
    monster_id: usize,
) -> CombatPublicIntentFactsV1 {
    if combat.entities.player.has_relic(RelicId::RunicDome) {
        return hidden_intent_facts(HiddenInformationReasonV1::RunicDome);
    }
    let observation = combat
        .runtime
        .monster_protocol
        .get(&monster_id)
        .map(|protocol| &protocol.observation);
    if let Some(observation) = observation {
        if observation.visible_intent != Intent::Unknown {
            return CombatPublicIntentFactsV1 {
                evidence: ObservationEvidenceKindV1::VisibleExact,
                intent: Some(observation.visible_intent.clone()),
                preview_damage_per_hit: (observation.preview_damage_per_hit > 0)
                    .then_some(observation.preview_damage_per_hit),
                hidden_reason: None,
            };
        }
    }

    let Some(monster) = combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == monster_id)
    else {
        return hidden_intent_facts(HiddenInformationReasonV1::IntentNotVisible);
    };
    let Some(visible_spec) = monster.move_state.planned_visible_spec.as_ref() else {
        return hidden_intent_facts(HiddenInformationReasonV1::IntentNotVisible);
    };
    let intent = Intent::from_visible_move_spec(visible_spec);
    let preview =
        crate::sim::combat_projection::project_monster_move_preview_in_combat(combat, monster);
    CombatPublicIntentFactsV1 {
        evidence: ObservationEvidenceKindV1::VisibleExact,
        intent: Some(intent),
        preview_damage_per_hit: preview.damage_per_hit,
        hidden_reason: None,
    }
}

fn hidden_intent_facts(reason: HiddenInformationReasonV1) -> CombatPublicIntentFactsV1 {
    CombatPublicIntentFactsV1 {
        evidence: ObservationEvidenceKindV1::Hidden,
        intent: None,
        preview_damage_per_hit: None,
        hidden_reason: Some(reason),
    }
}

fn public_card(card: &CombatCard) -> CombatPublicCardV2 {
    CombatPublicCardV2 {
        card_id: card.id.stable_id().to_string(),
        upgrades: card.upgrades,
        cost_for_turn: card.cost_for_turn_java(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::relics::RelicState;
    use crate::runtime::combat::CombatCard;
    use crate::runtime::monster_move::{AttackSpec, DamageKind, MonsterMoveSpec};

    #[test]
    fn planned_visible_spec_is_public_without_a_protocol_mirror() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.slot = 0;
        let spec = MonsterMoveSpec::Attack(AttackSpec {
            base_damage: 11,
            hits: 1,
            damage_kind: DamageKind::Normal,
        });
        monster.set_planned_move_id(1);
        monster.set_planned_steps(spec.to_steps());
        monster.set_planned_visible_spec(Some(spec));
        combat.entities.monsters.push(monster);

        let facts = combat_public_intent_facts_v1(&combat, 7);

        assert_eq!(facts.evidence, ObservationEvidenceKindV1::VisibleExact);
        assert_eq!(
            facts.intent,
            Some(Intent::Attack {
                damage: 11,
                hits: 1,
            })
        );
        assert_eq!(facts.preview_damage_per_hit, Some(11));
        assert_eq!(facts.hidden_reason, None);
    }

    #[test]
    fn planned_life_suck_uses_the_original_attack_buff_intent() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::ShelledParasite);
        monster.id = 7;
        monster.slot = 0;
        let spec = MonsterMoveSpec::AttackBuffIntent(AttackSpec {
            base_damage: 10,
            hits: 1,
            damage_kind: DamageKind::Normal,
        });
        monster.set_planned_move_id(3);
        monster.set_planned_steps(spec.to_steps());
        monster.set_planned_visible_spec(Some(spec));
        combat.entities.monsters.push(monster);

        let facts = combat_public_intent_facts_v1(&combat, 7);

        assert_eq!(
            facts.intent,
            Some(Intent::AttackBuff {
                damage: 10,
                hits: 1,
            })
        );
        assert_eq!(facts.preview_damage_per_hit, Some(10));
    }

    #[test]
    fn planned_heal_is_not_collapsed_into_a_generic_buff() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::Healer);
        monster.id = 7;
        monster.slot = 0;
        let spec = MonsterMoveSpec::Heal(crate::runtime::monster_move::HealSpec {
            target: crate::runtime::monster_move::MoveTarget::AllMonsters,
            amount: 20,
        });
        monster.set_planned_move_id(2);
        monster.set_planned_steps(spec.to_steps());
        monster.set_planned_visible_spec(Some(spec));
        combat.entities.monsters.push(monster);

        let facts = combat_public_intent_facts_v1(&combat, 7);

        assert_eq!(facts.intent, Some(Intent::Heal { amount: Some(20) }));
        assert_eq!(facts.preview_damage_per_hit, None);
    }

    #[test]
    fn visible_unknown_intent_is_not_collapsed_into_hidden_information() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::BronzeAutomaton);
        monster.id = 7;
        monster.slot = 0;
        monster.set_planned_move_id(4);
        monster.set_planned_steps(MonsterMoveSpec::Unknown.to_steps());
        monster.set_planned_visible_spec(Some(MonsterMoveSpec::Unknown));
        combat.entities.monsters.push(monster);

        let facts = combat_public_intent_facts_v1(&combat, 7);

        assert_eq!(facts.evidence, ObservationEvidenceKindV1::VisibleExact);
        assert_eq!(facts.intent, Some(Intent::Unknown));
        assert_eq!(facts.preview_damage_per_hit, None);
        assert_eq!(facts.hidden_reason, None);
    }

    #[test]
    fn runic_dome_public_observation_hides_intent_and_damage_preview() {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.slot = 0;
        combat.entities.monsters.push(monster);
        combat.set_monster_protocol_visible_intent(
            7,
            Intent::Attack {
                damage: 11,
                hits: 1,
            },
        );
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicDome));

        let observation = combat_public_observation_v2(&combat);

        assert!(observation
            .hidden_reasons
            .contains(&HiddenInformationReasonV1::RunicDome));
        assert_eq!(
            observation.monsters[0].intent.hidden_reason,
            Some(HiddenInformationReasonV1::RunicDome)
        );
        assert_eq!(observation.monsters[0].intent.intent, None);
        assert_eq!(observation.monsters[0].intent.preview_damage_per_hit, None);
    }

    #[test]
    fn draw_pile_order_is_hidden_without_frozen_eye_but_card_set_is_public() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Bash, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Defend, 3),
        ])
        .into();

        let observation = combat_public_observation_v2(&combat);

        assert_eq!(
            observation.piles.draw.evidence,
            ObservationEvidenceKindV1::PublicUnorderedCollection
        );
        assert_eq!(
            observation.piles.draw.hidden_reason,
            Some(HiddenInformationReasonV1::DrawPileOrderHidden)
        );
        assert_eq!(
            observation
                .piles
                .draw
                .cards
                .iter()
                .map(|card| card.card_id.as_str())
                .collect::<Vec<_>>(),
            vec!["Bash", "Defend", "Strike"]
        );
    }

    #[test]
    fn frozen_eye_public_observation_keeps_draw_pile_order() {
        let mut combat = crate::test_support::blank_test_combat();
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::FrozenEye));
        combat.zones.draw_pile = (vec![
            CombatCard::for_scenario(CardId::Bash, 1),
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Defend, 3),
        ])
        .into();

        let observation = combat_public_observation_v2(&combat);

        assert_eq!(
            observation.piles.draw.evidence,
            ObservationEvidenceKindV1::PublicOrderedCollection
        );
        assert_eq!(observation.piles.draw.hidden_reason, None);
        assert_eq!(
            observation
                .piles
                .draw
                .cards
                .iter()
                .map(|card| card.card_id.as_str())
                .collect::<Vec<_>>(),
            vec!["Bash", "Strike", "Defend"]
        );
    }

    #[test]
    fn natural_combat_start_exposes_the_rolled_monster_intent() {
        let mut run = crate::state::run::RunState::new(1, 20, false, "Ironclad");
        let (_engine, combat) = crate::sim::combat_start::build_natural_combat_start(
            &mut run,
            crate::content::monsters::factory::EncounterId::JawWorm,
            crate::state::map::node::RoomType::MonsterRoom,
        )
        .expect("combat should initialize");
        let monster = combat.entities.monsters.first().expect("jaw worm");

        let intent = combat_public_intent_facts_v1(&combat, monster.id);

        assert_eq!(intent.evidence, ObservationEvidenceKindV1::VisibleExact);
        assert!(intent.intent.is_some());
        assert_eq!(intent.hidden_reason, None);
    }
}
