use crate::ai::card_analysis_v1::card_analysis_profile_v1;
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::deck_role_inventory::card_is_strong_aoe;
use crate::ai::strategy::deck_strategic_deficit::StrategicDeficitLevel;
use crate::ai::strategy::reward_admission::{RewardAdmission, RewardAdmissionReason};
use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BossSurvivalRepairKind {
    PlanRepair,
    TimedBridge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BossSurvivalEvidence {
    pub label: &'static str,
    pub score_delta: i32,
    pub repair_kind: Option<BossSurvivalRepairKind>,
}

impl BossSurvivalEvidence {
    const fn plan_repair(label: &'static str, score_delta: i32) -> Self {
        Self {
            label,
            score_delta,
            repair_kind: Some(BossSurvivalRepairKind::PlanRepair),
        }
    }

    const fn timed_bridge(label: &'static str, score_delta: i32) -> Self {
        Self {
            label,
            score_delta,
            repair_kind: Some(BossSurvivalRepairKind::TimedBridge),
        }
    }

    const fn score_only(label: &'static str, score_delta: i32) -> Self {
        Self {
            label,
            score_delta,
            repair_kind: None,
        }
    }

    const fn none() -> Self {
        Self {
            label: "",
            score_delta: 0,
            repair_kind: None,
        }
    }

    pub const fn repairs_plan(self) -> bool {
        self.repair_kind.is_some()
    }
}

pub fn assess_boss_survival_evidence(
    deck: DeckPlanSnapshot,
    card: Option<(CardId, u8)>,
    admission: &RewardAdmission,
) -> BossSurvivalEvidence {
    let Some((card, upgrades)) = card else {
        return BossSurvivalEvidence::none();
    };
    match deck.boss_key {
        Some(EncounterId::TheGuardian) => guardian_survival_evidence(deck, card),
        Some(EncounterId::Hexaghost) => hexaghost_survival_evidence(deck, card, upgrades),
        Some(EncounterId::TheChamp) => champ_survival_evidence(deck, card),
        Some(EncounterId::DonuAndDeca) => {
            donu_deca_survival_evidence(deck, card, upgrades, admission)
        }
        Some(EncounterId::AwakenedOne) => awakened_one_survival_evidence(deck, card, upgrades),
        Some(EncounterId::Collector) => collector_minion_control_evidence(deck, admission),
        Some(EncounterId::Automaton) => automaton_survival_evidence(deck, card, upgrades),
        _ => BossSurvivalEvidence::none(),
    }
}

fn automaton_survival_evidence(
    deck: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
) -> BossSurvivalEvidence {
    let candidate = card_analysis_profile_v1(card, upgrades);
    if deck.roles.high_quality_block_units == 0 && candidate.is_block_plan_high_quality_chunk {
        BossSurvivalEvidence::plan_repair("automaton-first-hyperbeam-block-plan", 90)
    } else {
        BossSurvivalEvidence::none()
    }
}

fn hexaghost_survival_evidence(
    deck: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
) -> BossSurvivalEvidence {
    let candidate = card_analysis_profile_v1(card, upgrades);
    if deck.roles.high_quality_block_units == 0 && candidate.is_block_plan_high_quality_chunk {
        BossSurvivalEvidence::plan_repair("hexaghost-first-burst-block-plan", 85)
    } else {
        BossSurvivalEvidence::none()
    }
}

fn champ_survival_evidence(deck: DeckPlanSnapshot, card: CardId) -> BossSurvivalEvidence {
    match card {
        CardId::Disarm if deck.roles.persistent_enemy_strength_down_units == 0 => {
            BossSurvivalEvidence::plan_repair("champ-execute-strength-down", 100)
        }
        CardId::Shockwave if deck.roles.weak_units <= 1 => {
            BossSurvivalEvidence::plan_repair("champ-execute-debuff-window", 85)
        }
        _ => BossSurvivalEvidence::none(),
    }
}

fn donu_deca_survival_evidence(
    deck: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
    admission: &RewardAdmission,
) -> BossSurvivalEvidence {
    if deck.roles.strong_aoe_units == 0
        && admission
            .reasons
            .contains(&RewardAdmissionReason::AreaDamage)
        && card_is_strong_aoe(card, upgrades)
    {
        BossSurvivalEvidence::plan_repair("donu-deca-first-strong-aoe", 100)
    } else {
        BossSurvivalEvidence::none()
    }
}

fn guardian_survival_evidence(deck: DeckPlanSnapshot, card: CardId) -> BossSurvivalEvidence {
    match card {
        CardId::Clothesline if deck.roles.mitigation_units == 0 => {
            BossSurvivalEvidence::plan_repair("guardian-first-weak-answer", 70)
        }
        CardId::FlameBarrier if deck.roles.block_units <= 5 => {
            BossSurvivalEvidence::plan_repair("guardian-first-substantial-block", 70)
        }
        _ => BossSurvivalEvidence::none(),
    }
}

fn collector_minion_control_evidence(
    deck: DeckPlanSnapshot,
    admission: &RewardAdmission,
) -> BossSurvivalEvidence {
    let control_gap_open = matches!(
        deck.strategic_deficit.aoe_or_minion_control,
        StrategicDeficitLevel::Missing | StrategicDeficitLevel::Thin
    );
    if control_gap_open
        && admission
            .reasons
            .contains(&RewardAdmissionReason::AreaDamage)
    {
        // Keep the known-boss authority explicit here. Generic shallow-AoE
        // credit no longer rewards duplicate weak attacks, so Collector's
        // concrete two-minion requirement owns the full comparison weight.
        BossSurvivalEvidence::plan_repair("collector-minion-control", 150)
    } else {
        BossSurvivalEvidence::none()
    }
}

fn awakened_one_survival_evidence(
    deck: DeckPlanSnapshot,
    card: CardId,
    upgrades: u8,
) -> BossSurvivalEvidence {
    if !awakened_one_survival_pressure_open(deck) {
        return BossSurvivalEvidence::none();
    }
    match card {
        CardId::Disarm if deck.roles.persistent_enemy_strength_down_units == 0 => {
            BossSurvivalEvidence::plan_repair("awakened-one-strength-down-survival", 100)
        }
        CardId::Disarm => {
            BossSurvivalEvidence::score_only("awakened-one-duplicate-strength-down", 20)
        }
        CardId::Shockwave if deck.roles.weak_units == 0 => {
            BossSurvivalEvidence::plan_repair("awakened-one-weak-strength-down-survival", 95)
        }
        CardId::Shockwave => BossSurvivalEvidence::score_only("awakened-one-duplicate-weak", 15),
        CardId::DarkShackles if deck.roles.temporary_enemy_strength_down_units == 0 => {
            BossSurvivalEvidence::timed_bridge(
                "awakened-one-temporary-strength-timed-bridge",
                dark_shackles_bridge_score(deck, upgrades),
            )
        }
        CardId::DarkShackles => {
            BossSurvivalEvidence::score_only("awakened-one-duplicate-timed-bridge", 15)
        }
        CardId::Impervious | CardId::PowerThrough => {
            BossSurvivalEvidence::plan_repair("awakened-one-dark-echo-block-plan", 85)
        }
        CardId::FlameBarrier => {
            BossSurvivalEvidence::plan_repair("awakened-one-repeatable-block-plan", 70)
        }
        CardId::SecondWind
            if deck.roles.exhaust_stream_units > 0 || deck.roles.corruption_units > 0 =>
        {
            BossSurvivalEvidence::plan_repair("awakened-one-exhaust-block-plan", 65)
        }
        CardId::FeelNoPain
            if deck.roles.exhaust_stream_units > 0 || deck.roles.corruption_units > 0 =>
        {
            BossSurvivalEvidence::plan_repair("awakened-one-exhaust-block-engine", 60)
        }
        CardId::ShrugItOff if upgrades > 0 => {
            BossSurvivalEvidence::score_only("awakened-one-generic-block-access", 20)
        }
        _ => BossSurvivalEvidence::none(),
    }
}

fn awakened_one_survival_pressure_open(deck: DeckPlanSnapshot) -> bool {
    deck.context.act >= 3 && (deck.roles.strength_source_units > 0 || deck.roles.aoe_units > 0)
}

fn dark_shackles_bridge_score(deck: DeckPlanSnapshot, upgrades: u8) -> i32 {
    80 + if upgrades > 0 { 20 } else { 0 }
        + if deck.run_facts.has_runic_pyramid {
            15
        } else {
            0
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::analysis::card_semantics::Mechanic;
    use crate::ai::strategy::deck_admission::DeckAdmissionContext;
    use crate::ai::strategy::reward_admission::{RewardAdmissionClass, RewardAdmissionReason};
    use crate::ai::strategy::run_strategic_facts::RunStrategicFacts;
    use crate::runtime::combat::CombatCard;

    fn deck_plan(cards: &[CardId], boss: Option<EncounterId>) -> DeckPlanSnapshot {
        let deck = cards
            .iter()
            .enumerate()
            .map(|(index, card)| CombatCard::for_scenario(*card, index as u32 + 1))
            .collect::<Vec<_>>();
        DeckPlanSnapshot::from_deck(
            &deck,
            DeckAdmissionContext {
                act: 3,
                current_hp: 70,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: 3,
                starter_basic_count: 2,
                curse_count: 0,
                has_energy_relic: true,
                has_runic_pyramid: false,
            },
        )
        .with_boss_key(boss)
    }

    #[test]
    fn dark_shackles_is_timed_bridge_alongside_existing_weak_and_disarm() {
        let plan = deck_plan(
            &[
                CardId::DemonForm,
                CardId::Whirlwind,
                CardId::Clothesline,
                CardId::Disarm,
            ],
            Some(EncounterId::AwakenedOne),
        );
        let admission = RewardAdmission {
            card: Some(CardId::DarkShackles),
            class: RewardAdmissionClass::ImmediateWork,
            reasons: vec![
                RewardAdmissionReason::Provides(Mechanic::TemporaryEnemyStrengthDown),
                RewardAdmissionReason::ExhaustsSelf,
            ],
        };

        let evidence =
            assess_boss_survival_evidence(plan, Some((CardId::DarkShackles, 0)), &admission);

        assert_eq!(
            evidence.repair_kind,
            Some(BossSurvivalRepairKind::TimedBridge)
        );
        assert_eq!(
            evidence.label,
            "awakened-one-temporary-strength-timed-bridge"
        );
    }

    #[test]
    fn duplicate_dark_shackles_is_score_only_not_second_timed_bridge() {
        let plan = deck_plan(
            &[CardId::DemonForm, CardId::Whirlwind, CardId::DarkShackles],
            Some(EncounterId::AwakenedOne),
        );
        let admission = RewardAdmission {
            card: Some(CardId::DarkShackles),
            class: RewardAdmissionClass::ImmediateWork,
            reasons: vec![RewardAdmissionReason::Provides(
                Mechanic::TemporaryEnemyStrengthDown,
            )],
        };

        let evidence =
            assess_boss_survival_evidence(plan, Some((CardId::DarkShackles, 1)), &admission);

        assert_eq!(evidence.repair_kind, None);
        assert!(evidence.score_delta > 0);
    }

    #[test]
    fn upgrade_and_pyramid_raise_timed_bridge_score_without_changing_kind() {
        let base = deck_plan(
            &[CardId::DemonForm, CardId::Whirlwind, CardId::Clothesline],
            Some(EncounterId::AwakenedOne),
        );
        let mut retained = base;
        retained.run_facts.has_runic_pyramid = true;
        let admission = RewardAdmission {
            card: Some(CardId::DarkShackles),
            class: RewardAdmissionClass::ImmediateWork,
            reasons: vec![RewardAdmissionReason::Provides(
                Mechanic::TemporaryEnemyStrengthDown,
            )],
        };

        let plain =
            assess_boss_survival_evidence(base, Some((CardId::DarkShackles, 0)), &admission);
        let upgraded_retained =
            assess_boss_survival_evidence(retained, Some((CardId::DarkShackles, 1)), &admission);

        assert_eq!(plain.repair_kind, upgraded_retained.repair_kind);
        assert!(upgraded_retained.score_delta > plain.score_delta);
    }

    #[test]
    fn awakened_one_mitigation_card_repairs_open_survival_plan() {
        let plan = deck_plan(
            &[
                CardId::DemonForm,
                CardId::Rupture,
                CardId::Whirlwind,
                CardId::Cleave,
                CardId::ShrugItOff,
            ],
            Some(EncounterId::AwakenedOne),
        );
        let admission = RewardAdmission {
            card: Some(CardId::Disarm),
            class: crate::ai::strategy::reward_admission::RewardAdmissionClass::ImmediateWork,
            reasons: vec![RewardAdmissionReason::Provides(Mechanic::EnemyStrengthDown)],
        };

        let evidence = assess_boss_survival_evidence(plan, Some((CardId::Disarm, 0)), &admission);

        assert_eq!(evidence.label, "awakened-one-strength-down-survival");
        assert!(evidence.repairs_plan());
    }

    #[test]
    fn awakened_one_generic_block_draw_is_not_a_survival_plan_repair() {
        let plan = deck_plan(
            &[
                CardId::DemonForm,
                CardId::Rupture,
                CardId::Whirlwind,
                CardId::Cleave,
                CardId::ShrugItOff,
            ],
            Some(EncounterId::AwakenedOne),
        );
        let admission = RewardAdmission {
            card: Some(CardId::ShrugItOff),
            class: crate::ai::strategy::reward_admission::RewardAdmissionClass::ImmediateWork,
            reasons: vec![
                RewardAdmissionReason::Provides(Mechanic::Block),
                RewardAdmissionReason::Provides(Mechanic::CardDraw),
            ],
        };

        let evidence =
            assess_boss_survival_evidence(plan, Some((CardId::ShrugItOff, 1)), &admission);

        assert_eq!(evidence.label, "awakened-one-generic-block-access");
        assert!(!evidence.repairs_plan());
    }

    #[test]
    fn hexaghost_first_high_quality_block_chunk_repairs_the_survival_plan() {
        let plan = deck_plan(
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
                CardId::Clothesline,
                CardId::BurningPact,
            ],
            Some(EncounterId::Hexaghost),
        );
        let admission = RewardAdmission {
            card: Some(CardId::PowerThrough),
            class: RewardAdmissionClass::BurdenedImmediateWork,
            reasons: vec![RewardAdmissionReason::Provides(Mechanic::Block)],
        };

        let evidence =
            assess_boss_survival_evidence(plan, Some((CardId::PowerThrough, 0)), &admission);

        assert_eq!(evidence.label, "hexaghost-first-burst-block-plan");
        assert!(evidence.repairs_plan());
    }

    #[test]
    fn hexaghost_does_not_promote_a_duplicate_high_quality_block_chunk() {
        let plan = deck_plan(
            &[
                CardId::Strike,
                CardId::Defend,
                CardId::Bash,
                CardId::Impervious,
            ],
            Some(EncounterId::Hexaghost),
        );
        let admission = RewardAdmission {
            card: Some(CardId::PowerThrough),
            class: RewardAdmissionClass::BurdenedImmediateWork,
            reasons: vec![RewardAdmissionReason::Provides(Mechanic::Block)],
        };

        let evidence =
            assess_boss_survival_evidence(plan, Some((CardId::PowerThrough, 0)), &admission);

        assert!(!evidence.repairs_plan());
    }

    #[test]
    fn automaton_first_high_quality_block_chunk_repairs_hyperbeam_plan() {
        let plan = deck_plan(
            &[
                CardId::Strike,
                CardId::Defend,
                CardId::Bash,
                CardId::DarkEmbrace,
                CardId::SecondWind,
            ],
            Some(EncounterId::Automaton),
        );
        let admission = RewardAdmission {
            card: Some(CardId::Impervious),
            class: RewardAdmissionClass::ImmediateWork,
            reasons: vec![
                RewardAdmissionReason::Provides(Mechanic::Block),
                RewardAdmissionReason::ExhaustsSelf,
            ],
        };

        let evidence =
            assess_boss_survival_evidence(plan, Some((CardId::Impervious, 0)), &admission);

        assert_eq!(evidence.label, "automaton-first-hyperbeam-block-plan");
        assert!(evidence.repairs_plan());
    }

    #[test]
    fn automaton_does_not_promote_a_duplicate_high_quality_block_chunk() {
        let plan = deck_plan(
            &[
                CardId::Strike,
                CardId::Defend,
                CardId::Bash,
                CardId::Impervious,
            ],
            Some(EncounterId::Automaton),
        );
        let admission = RewardAdmission {
            card: Some(CardId::PowerThrough),
            class: RewardAdmissionClass::BurdenedImmediateWork,
            reasons: vec![RewardAdmissionReason::Provides(Mechanic::Block)],
        };

        let evidence =
            assess_boss_survival_evidence(plan, Some((CardId::PowerThrough, 0)), &admission);

        assert!(!evidence.repairs_plan());
    }

    #[test]
    fn collector_area_damage_repairs_thin_minion_control() {
        let plan = deck_plan(
            &[
                CardId::Strike,
                CardId::Strike,
                CardId::Defend,
                CardId::Defend,
                CardId::Bash,
                CardId::PommelStrike,
                CardId::ShrugItOff,
                CardId::Cleave,
            ],
            Some(EncounterId::Collector),
        );
        let admission = RewardAdmission {
            card: Some(CardId::Cleave),
            class: crate::ai::strategy::reward_admission::RewardAdmissionClass::ImmediateWork,
            reasons: vec![RewardAdmissionReason::AreaDamage],
        };

        let evidence = assess_boss_survival_evidence(plan, Some((CardId::Cleave, 0)), &admission);

        assert_eq!(evidence.label, "collector-minion-control");
        assert!(evidence.repairs_plan());
        assert!(evidence.score_delta > 0);
    }

    #[test]
    fn champ_first_persistent_strength_down_repairs_execute_survival() {
        let plan = deck_plan(
            &[CardId::Clothesline, CardId::ShrugItOff],
            Some(EncounterId::TheChamp),
        );
        let admission = RewardAdmission {
            card: Some(CardId::Disarm),
            class: RewardAdmissionClass::ImmediateWork,
            reasons: vec![RewardAdmissionReason::Provides(Mechanic::EnemyStrengthDown)],
        };

        let evidence = assess_boss_survival_evidence(plan, Some((CardId::Disarm, 0)), &admission);

        assert_eq!(evidence.label, "champ-execute-strength-down");
        assert!(evidence.repairs_plan());
    }

    #[test]
    fn donu_deca_first_strong_aoe_repairs_multi_target_survival() {
        let plan = deck_plan(
            &[CardId::Inflame, CardId::HeavyBlade, CardId::SwordBoomerang],
            Some(EncounterId::DonuAndDeca),
        );
        let admission = RewardAdmission {
            card: Some(CardId::Whirlwind),
            class: RewardAdmissionClass::BuildsSupportedPackage,
            reasons: vec![RewardAdmissionReason::AreaDamage],
        };

        let evidence =
            assess_boss_survival_evidence(plan, Some((CardId::Whirlwind, 1)), &admission);

        assert_eq!(evidence.label, "donu-deca-first-strong-aoe");
        assert!(evidence.repairs_plan());
    }
}
