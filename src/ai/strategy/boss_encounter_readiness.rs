use serde::Serialize;

use crate::ai::boss_mechanics_v1::{boss_mechanic_pressure_profile_v1, BossMechanicRedFlagV1};
use crate::ai::card_semantics_v1::card_owned_base_output_v1;
use crate::ai::noncombat_strategy_v1::{
    build_run_strategy_snapshot_from_run_state_v2, StrategyCapabilityCoverageV1,
    StrategyCapabilityInputKindV1, StrategyCapabilityKindV1, StrategyThreatSourceV1,
};
use crate::content::monsters::factory::EncounterId;
use crate::content::potions::PotionId;
use crate::content::relics::RelicId;
use crate::state::run::RunState;

// This is a conservative static deck-inventory proxy for the public
// three-turn 70-damage Slime Boss question, not a simulated draw-order claim.
const SLIME_BOSS_STATIC_ATTACK_DAMAGE_TARGET_V1: i32 = 70;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BossEncounterPreparationBandV1 {
    #[default]
    Unassessed,
    Exposed,
    PotionBacked,
    Established,
}

impl BossEncounterPreparationBandV1 {
    pub const fn requires_resource_preservation(self) -> bool {
        matches!(self, Self::Exposed | Self::PotionBacked)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BossDamagePotionFactV1 {
    pub slot: usize,
    pub uuid: u32,
    pub potion: PotionId,
    pub opening_burst_support: bool,
    pub post_split_support: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BossEncounterReadinessV1 {
    pub boss: Option<EncounterId>,
    pub preparation: BossEncounterPreparationBandV1,
    pub phase_control: StrategyCapabilityCoverageV1,
    pub multi_target_control: StrategyCapabilityCoverageV1,
    pub timed_damage_race: StrategyCapabilityCoverageV1,
    pub sustained_defense: StrategyCapabilityCoverageV1,
    pub draw_energy_consistency: StrategyCapabilityCoverageV1,
    pub static_single_target_attack_damage: i32,
    pub best_single_hit: i32,
    pub aoe_sources: i32,
    pub strong_aoe_sources: usize,
    pub burst_attack_sources: usize,
    pub boss_threat_gap_count: usize,
    pub missing_transition_burst: bool,
    pub missing_post_split_aoe: bool,
    pub sacred_bark: bool,
    pub damage_potions: Vec<BossDamagePotionFactV1>,
    pub two_stage_potion_coverage: bool,
}

impl Default for BossEncounterReadinessV1 {
    fn default() -> Self {
        Self {
            boss: None,
            preparation: BossEncounterPreparationBandV1::Unassessed,
            phase_control: StrategyCapabilityCoverageV1::Unknown,
            multi_target_control: StrategyCapabilityCoverageV1::Unknown,
            timed_damage_race: StrategyCapabilityCoverageV1::Unknown,
            sustained_defense: StrategyCapabilityCoverageV1::Unknown,
            draw_energy_consistency: StrategyCapabilityCoverageV1::Unknown,
            static_single_target_attack_damage: 0,
            best_single_hit: 0,
            aoe_sources: 0,
            strong_aoe_sources: 0,
            burst_attack_sources: 0,
            boss_threat_gap_count: 0,
            missing_transition_burst: false,
            missing_post_split_aoe: false,
            sacred_bark: false,
            damage_potions: Vec::new(),
            two_stage_potion_coverage: false,
        }
    }
}

pub fn boss_encounter_readiness_v1(run_state: &RunState) -> BossEncounterReadinessV1 {
    let Some(boss) = run_state.boss_key else {
        return BossEncounterReadinessV1::default();
    };
    let strategy = build_run_strategy_snapshot_from_run_state_v2(run_state);
    let capability = |kind| {
        strategy
            .threat_coverage
            .capability(kind)
            .map(|fact| fact.coverage)
            .unwrap_or(StrategyCapabilityCoverageV1::Unknown)
    };
    let input = |kind, input| {
        strategy
            .threat_coverage
            .capability(kind)
            .and_then(|fact| fact.inputs.iter().find(|fact| fact.input == input))
            .map(|fact| fact.value)
            .unwrap_or(0)
    };
    let mechanics = boss_mechanic_pressure_profile_v1(run_state, boss);
    let damage_potions = damage_potion_facts(run_state);
    let two_stage_potion_coverage = has_two_stage_potion_coverage(&damage_potions);
    let strong_aoe_sources = usize::try_from(input(
        StrategyCapabilityKindV1::MultiTargetControl,
        StrategyCapabilityInputKindV1::StrongAoeSources,
    ))
    .unwrap_or(0);
    let burst_attack_sources = burst_attack_source_count(run_state);
    let phase_control = capability(StrategyCapabilityKindV1::PhaseControl);
    let multi_target_control = capability(StrategyCapabilityKindV1::MultiTargetControl);
    let timed_damage_race = capability(StrategyCapabilityKindV1::TimedDamageRace);
    let static_single_target_attack_damage = input(
        StrategyCapabilityKindV1::SingleTargetFrontload,
        StrategyCapabilityInputKindV1::SingleTargetDamage,
    );
    let preparation = match boss {
        EncounterId::SlimeBoss => {
            let opening_plan = timed_damage_race >= StrategyCapabilityCoverageV1::Supported
                && (phase_control >= StrategyCapabilityCoverageV1::Supported
                    || static_single_target_attack_damage
                        >= SLIME_BOSS_STATIC_ATTACK_DAMAGE_TARGET_V1);
            let post_split_plan = multi_target_control >= StrategyCapabilityCoverageV1::Strong
                || strong_aoe_sources > 0
                || burst_attack_sources > 0;
            if opening_plan && post_split_plan {
                BossEncounterPreparationBandV1::Established
            } else if two_stage_potion_coverage {
                BossEncounterPreparationBandV1::PotionBacked
            } else {
                BossEncounterPreparationBandV1::Exposed
            }
        }
        _ => BossEncounterPreparationBandV1::Unassessed,
    };

    BossEncounterReadinessV1 {
        boss: Some(boss),
        preparation,
        phase_control,
        multi_target_control,
        timed_damage_race,
        sustained_defense: capability(StrategyCapabilityKindV1::SustainedDefense),
        draw_energy_consistency: capability(StrategyCapabilityKindV1::DrawEnergyConsistency),
        static_single_target_attack_damage,
        best_single_hit: input(
            StrategyCapabilityKindV1::SingleTargetFrontload,
            StrategyCapabilityInputKindV1::BestSingleHit,
        ),
        aoe_sources: input(
            StrategyCapabilityKindV1::MultiTargetControl,
            StrategyCapabilityInputKindV1::AoeSources,
        ),
        strong_aoe_sources,
        burst_attack_sources,
        boss_threat_gap_count: strategy
            .threat_coverage
            .gaps
            .iter()
            .filter(|gap| gap.source == StrategyThreatSourceV1::ActBoss)
            .count(),
        missing_transition_burst: mechanics.has_red_flag(BossMechanicRedFlagV1::NoHalfHpBurstPlan),
        missing_post_split_aoe: mechanics
            .has_red_flag(BossMechanicRedFlagV1::SplitDamageWithoutAoe),
        sacred_bark: run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::SacredBark),
        damage_potions,
        two_stage_potion_coverage,
    }
}

fn burst_attack_source_count(run_state: &RunState) -> usize {
    run_state
        .master_deck
        .iter()
        .filter(|card| {
            card_owned_base_output_v1(card)
                .damage
                .unconditional_targeted_base_total()
                .is_some_and(|amount| amount >= 20)
        })
        .count()
}

fn damage_potion_facts(run_state: &RunState) -> Vec<BossDamagePotionFactV1> {
    run_state
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            let potion = potion.as_ref()?;
            let (opening_burst_support, post_split_support) = match potion.id {
                PotionId::FearPotion => (true, false),
                PotionId::FirePotion
                | PotionId::ExplosivePotion
                | PotionId::StrengthPotion
                | PotionId::SteroidPotion
                | PotionId::AttackPotion => (true, true),
                _ => return None,
            };
            Some(BossDamagePotionFactV1 {
                slot,
                uuid: potion.uuid,
                potion: potion.id,
                opening_burst_support,
                post_split_support,
            })
        })
        .collect()
}

fn has_two_stage_potion_coverage(potions: &[BossDamagePotionFactV1]) -> bool {
    potions.iter().enumerate().any(|(opening_index, opening)| {
        opening.opening_burst_support
            && potions
                .iter()
                .enumerate()
                .any(|(post_index, post)| opening_index != post_index && post.post_split_support)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::potions::Potion;
    use crate::runtime::combat::CombatCard;
    use crate::test_support::synthetic_deck_test_run;

    fn card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    fn f12_like_slime_boss_run() -> RunState {
        let ids = [
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Strike,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Defend,
            CardId::Bash,
            CardId::PowerThrough,
            CardId::HeavyBlade,
            CardId::SecondWind,
            CardId::DarkEmbrace,
            CardId::ThunderClap,
            CardId::ShrugItOff,
        ];
        let deck = ids
            .into_iter()
            .enumerate()
            .map(|(index, id)| card(id, index as u32 + 1))
            .collect();
        let mut run = synthetic_deck_test_run(1, 0, false, "Ironclad", deck);
        run.boss_key = Some(EncounterId::SlimeBoss);
        run.potions = vec![
            Some(Potion::new(PotionId::FearPotion, 101)),
            Some(Potion::new(PotionId::FearPotion, 102)),
            Some(Potion::new(PotionId::FirePotion, 103)),
        ];
        run
    }

    #[test]
    fn slime_boss_profile_distinguishes_potion_backup_from_an_established_deck_plan() {
        let facts = boss_encounter_readiness_v1(&f12_like_slime_boss_run());

        assert_eq!(facts.boss, Some(EncounterId::SlimeBoss));
        assert_eq!(
            facts.preparation,
            BossEncounterPreparationBandV1::PotionBacked
        );
        assert_eq!(facts.static_single_target_attack_damage, 56);
        assert_eq!(
            facts.multi_target_control,
            StrategyCapabilityCoverageV1::Supported
        );
        assert_eq!(facts.strong_aoe_sources, 0);
        assert_eq!(facts.burst_attack_sources, 0);
        assert_eq!(facts.timed_damage_race, StrategyCapabilityCoverageV1::Thin);
        assert_eq!(facts.phase_control, StrategyCapabilityCoverageV1::Missing);
        assert_eq!(facts.damage_potions.len(), 3);
        assert!(facts.two_stage_potion_coverage);
        assert!(facts.missing_transition_burst);
        assert!(!facts.missing_post_split_aoe);
    }

    #[test]
    fn one_flexible_damage_potion_cannot_cover_both_slime_boss_stages() {
        let mut run = f12_like_slime_boss_run();
        run.potions = vec![Some(Potion::new(PotionId::FirePotion, 103)), None, None];

        let facts = boss_encounter_readiness_v1(&run);

        assert!(!facts.two_stage_potion_coverage);
        assert_eq!(facts.preparation, BossEncounterPreparationBandV1::Exposed);
    }

    #[test]
    fn sufficient_static_damage_and_a_burst_finisher_establish_a_slime_boss_plan() {
        let mut run = f12_like_slime_boss_run();
        let next_uuid = run.master_deck.len() as u32 + 1;
        run.master_deck.push(card(CardId::Carnage, next_uuid));
        run.potions = vec![None, None, None];

        let facts = boss_encounter_readiness_v1(&run);

        assert!(facts.static_single_target_attack_damage >= 70);
        assert_eq!(
            facts.preparation,
            BossEncounterPreparationBandV1::Established
        );
        assert!(!facts.two_stage_potion_coverage);
        assert_eq!(facts.burst_attack_sources, 1);
    }

    #[test]
    fn static_damage_plus_one_light_aoe_does_not_establish_a_post_split_plan() {
        let mut run = f12_like_slime_boss_run();
        let next_uuid = run.master_deck.len() as u32 + 1;
        run.master_deck.extend([
            card(CardId::Strike, next_uuid),
            card(CardId::Strike, next_uuid + 1),
            card(CardId::Strike, next_uuid + 2),
        ]);
        run.potions = vec![None, None, None];

        let facts = boss_encounter_readiness_v1(&run);

        assert!(facts.static_single_target_attack_damage >= 70);
        assert_eq!(
            facts.multi_target_control,
            StrategyCapabilityCoverageV1::Supported
        );
        assert_eq!(facts.strong_aoe_sources, 0);
        assert_eq!(facts.burst_attack_sources, 0);
        assert_eq!(facts.preparation, BossEncounterPreparationBandV1::Exposed);
    }

    #[test]
    fn conditional_aoe_alone_cannot_establish_the_post_split_plan() {
        let mut run = f12_like_slime_boss_run();
        let next_uuid = run.master_deck.len() as u32 + 1;
        run.master_deck.extend([
            card(CardId::Strike, next_uuid),
            card(CardId::Strike, next_uuid + 1),
            card(CardId::Strike, next_uuid + 2),
            card(CardId::Whirlwind, next_uuid + 3),
        ]);
        run.potions = vec![None, None, None];
        let facts = boss_encounter_readiness_v1(&run);
        assert!(facts.static_single_target_attack_damage >= 70);
        assert_eq!(facts.strong_aoe_sources, 0);
        assert_eq!(facts.preparation, BossEncounterPreparationBandV1::Exposed);
    }

    #[test]
    fn post_split_burst_retains_instance_values_and_delivery_constraints() {
        for (id, override_damage, expected) in [
            (CardId::GrandFinale, 50, 0),
            (CardId::SwordBoomerang, 20, 0),
            (CardId::Immolate, 1, 0),
            (CardId::TwinStrike, 10, 1),
        ] {
            let mut run = f12_like_slime_boss_run();
            let mut owned = card(id, 1);
            owned.base_damage_override = Some(override_damage);
            run.master_deck = vec![owned];
            assert_eq!(burst_attack_source_count(&run), expected, "{id:?}");
        }
        let mut run = f12_like_slime_boss_run();
        let mut owned = card(CardId::RitualDagger, 1);
        owned.misc_value = 31;
        run.master_deck = vec![owned];
        assert_eq!(burst_attack_source_count(&run), 1);
    }

    #[test]
    fn one_strong_aoe_can_establish_the_post_split_plan() {
        let mut run = f12_like_slime_boss_run();
        let next_uuid = run.master_deck.len() as u32 + 1;
        run.master_deck.push(card(CardId::Immolate, next_uuid));
        run.potions = vec![None, None, None];

        let facts = boss_encounter_readiness_v1(&run);

        assert!(facts.static_single_target_attack_damage >= 70);
        assert_eq!(facts.strong_aoe_sources, 1);
        assert_eq!(
            facts.preparation,
            BossEncounterPreparationBandV1::Established
        );
    }
}
