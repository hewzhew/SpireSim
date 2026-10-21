use crate::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CombatEvent, DamageScalingAxis, InstalledRule, Mechanic,
    PayoffRequirement, TriggeredEffect,
};
use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
use crate::ai::strategy::deck_role_inventory::card_is_stable_strength_source;
use crate::ai::strategy::package_transition::PackageKind;
use crate::ai::strategy::reward_admission::{
    RewardAdmission, RewardAdmissionClass, RewardAdmissionReason,
};
use crate::content::cards::CardId;
use crate::content::monsters::factory::EncounterId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BossScalingEvidence {
    pub label: &'static str,
    pub score_delta: i32,
    pub relevant_to_boss_plan: bool,
}

impl BossScalingEvidence {
    const fn relevant(label: &'static str, score_delta: i32) -> Self {
        Self {
            label,
            score_delta,
            relevant_to_boss_plan: true,
        }
    }

    const fn score_only(label: &'static str, score_delta: i32) -> Self {
        Self {
            label,
            score_delta,
            relevant_to_boss_plan: false,
        }
    }

    const fn none() -> Self {
        Self {
            label: "",
            score_delta: 0,
            relevant_to_boss_plan: false,
        }
    }
}

pub fn assess_boss_scaling_evidence(
    deck: DeckPlanSnapshot,
    card: Option<(CardId, u8)>,
    admission: &RewardAdmission,
) -> BossScalingEvidence {
    let card_semantics = card.map(|(id, upgrades)| card_definition_with_upgrades(id, upgrades));

    if let Some(evidence) = boss_specific_support_evidence(deck, card) {
        return evidence;
    }
    if admission_provides(admission, Mechanic::Strength)
        || card.is_some_and(|(id, upgrades)| {
            card_is_stable_strength_source(id, upgrades, deck.roles.repeatable_self_damage_supply)
        })
    {
        if admission.class == RewardAdmissionClass::OpensUnsupportedPayoff {
            return BossScalingEvidence::score_only("boss-unsupported-scaling-source", -35);
        }

        let existing_sources = deck
            .roles
            .strength_source_units
            .saturating_add(deck.roles.conditional_strength_source_units);
        if existing_sources == 0 {
            return BossScalingEvidence::relevant("boss-scaling-source", 70);
        }
        if deck.repairs_strength_package_reliability(card) {
            return BossScalingEvidence::relevant("boss-scaling-reliability", 70);
        }
        return BossScalingEvidence::score_only("boss-marginal-scaling-source", 0);
    }
    if admission_provides(admission, Mechanic::StrengthMultiplier) {
        return if deck.roles.strength_source_units > 0 {
            BossScalingEvidence::relevant("boss-scaling-multiplier", 45)
        } else {
            BossScalingEvidence::score_only("boss-speculative-multiplier", -35)
        };
    }
    if installs_corruption(admission) && deck.roles.exhaust_payoff_units > 0 {
        return BossScalingEvidence::relevant("boss-exhaust-engine-pair", 75);
    }
    if candidate_exhaust_payoff(card_semantics.as_ref()) {
        if deck.roles.exhaust_payoff_units > 0 {
            return BossScalingEvidence::score_only("boss-duplicate-engine-payoff", -25);
        }
        if deck.roles.broad_exhaust_units > 0
            || deck.roles.corruption_units > 0
            || deck.roles.exhaust_stream_units >= 2
        {
            return BossScalingEvidence::relevant("boss-exhaust-engine-pair", 75);
        }
        if deck.roles.exhaust_stream_units == 1 {
            return BossScalingEvidence::score_only("boss-limited-exhaust-synergy", 25);
        }
    }
    if candidate_exhaust_source(admission) && deck.roles.exhaust_payoff_units > 0 {
        return if card.is_some_and(|(card, upgrades)| {
            crate::ai::card_analysis_v1::card_analysis_profile_v1(card, upgrades)
                .is_block_plan_broad_exhaust_source
        }) {
            BossScalingEvidence::relevant("boss-exhaust-engine-pair", 75)
        } else {
            // A single-card exhaust source can trigger an installed payoff,
            // but it is not enough throughput to repair a boss-scale engine.
            // In particular, unsupported top-deck execution such as Havoc
            // must not become a strategic asset merely because Feel No Pain
            // is already present.
            BossScalingEvidence::score_only("boss-limited-exhaust-source", 25)
        };
    }
    if candidate_block_payoff(admission) {
        return if deck.block_plan_readiness
            >= crate::ai::block_plan_profile_v1::BlockPlanReadinessV1::Supported
        {
            BossScalingEvidence::relevant("boss-block-damage-engine", 55)
        } else {
            BossScalingEvidence::score_only("boss-speculative-payoff", -35)
        };
    }
    if admission_is_strength_payoff(admission) {
        return if deck.roles.strength_source_units > 0 && deck.roles.strength_payoff_units == 0 {
            BossScalingEvidence::relevant("boss-strength-payoff", 40)
        } else {
            BossScalingEvidence::score_only("boss-speculative-payoff", -35)
        };
    }
    if admission_provides(admission, Mechanic::CardDraw)
        || admission_provides(admission, Mechanic::Energy)
    {
        return BossScalingEvidence::score_only("boss-tempo-access", 25);
    }
    if admission_provides(admission, Mechanic::Weak)
        || admission_provides(admission, Mechanic::EnemyStrengthDown)
        || admission_provides(admission, Mechanic::TemporaryEnemyStrengthDown)
        || admission_provides(admission, Mechanic::Vulnerable)
    {
        return BossScalingEvidence::score_only("boss-support-only", 30);
    }
    if admission_provides(admission, Mechanic::Block) {
        return BossScalingEvidence::score_only("boss-premium-survival", 20);
    }
    BossScalingEvidence::none()
}

fn boss_specific_support_evidence(
    deck: DeckPlanSnapshot,
    card: Option<(CardId, u8)>,
) -> Option<BossScalingEvidence> {
    let (card, upgrades) = card?;
    match deck.boss_key {
        Some(EncounterId::Automaton) => automaton_support_evidence(card, upgrades),
        _ => None,
    }
}

fn automaton_support_evidence(card: CardId, upgrades: u8) -> Option<BossScalingEvidence> {
    match card {
        CardId::Shockwave => Some(BossScalingEvidence::relevant(
            "automaton-artifact-debuff-window",
            70,
        )),
        CardId::Uppercut => Some(BossScalingEvidence::relevant(
            "automaton-artifact-debuff-bridge",
            55,
        )),
        CardId::ShrugItOff if upgrades > 0 => Some(BossScalingEvidence::relevant(
            "automaton-hyperbeam-survival",
            45,
        )),
        CardId::SecondWind => Some(BossScalingEvidence::relevant(
            "automaton-pyramid-hand-management",
            45,
        )),
        _ => None,
    }
}

fn admission_provides(admission: &RewardAdmission, mechanic: Mechanic) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::Provides(mechanic))
}

fn installs_corruption(admission: &RewardAdmission) -> bool {
    admission.reasons.contains(&RewardAdmissionReason::Installs(
        InstalledRule::SkillCardsCostZeroAndExhaust,
    ))
}

fn candidate_exhaust_source(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::Emits(CombatEvent::CardExhausted))
        || admission
            .reasons
            .contains(&RewardAdmissionReason::PlaysTopCardAndExhaust)
}

fn candidate_block_payoff(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::DamageUses(Mechanic::Block))
        || admission
            .reasons
            .contains(&RewardAdmissionReason::Supports(PackageKind::Block))
        || admission.reasons.contains(&RewardAdmissionReason::Closes(
            PayoffRequirement::WantsMechanic(Mechanic::Block),
        ))
}

pub fn admission_is_strength_payoff(admission: &RewardAdmission) -> bool {
    admission
        .reasons
        .contains(&RewardAdmissionReason::DamageUses(Mechanic::Strength))
        || admission
            .reasons
            .contains(&RewardAdmissionReason::DamageScalesWith(
                DamageScalingAxis::PerHitStrength,
            ))
}

fn candidate_exhaust_payoff(
    semantics: Option<&crate::ai::analysis::card_semantics::CardDefinition>,
) -> bool {
    semantics.is_some_and(|definition| {
        definition.event_handlers.iter().any(|handler| {
            handler.on == CombatEvent::CardExhausted
                && matches!(
                    handler.effect,
                    TriggeredEffect::Provide(Mechanic::Block | Mechanic::CardDraw)
                )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::strategy::deck_admission::DeckAdmissionContext;
    use crate::ai::strategy::deck_plan::DeckPlanSnapshot;
    use crate::ai::strategy::reward_admission::assess_reward_admission_from_master_deck;
    use crate::ai::strategy::run_strategic_facts::RunStrategicFacts;
    use crate::runtime::combat::CombatCard;

    fn card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    fn deck_plan(cards: &[CardId]) -> (Vec<CombatCard>, DeckPlanSnapshot) {
        let deck: Vec<_> = cards
            .iter()
            .enumerate()
            .map(|(index, id)| card(*id, index as u32 + 1))
            .collect();
        let plan = DeckPlanSnapshot::from_deck(
            &deck,
            DeckAdmissionContext {
                act: 2,
                current_hp: 70,
                max_hp: 80,
            },
            RunStrategicFacts {
                entering_act: 2,
                starter_basic_count: 0,
                curse_count: 0,
                has_energy_relic: false,
                has_runic_pyramid: false,
            },
        );
        (deck, plan)
    }

    #[test]
    fn conditional_strength_does_not_make_strength_payoff_boss_relevant() {
        let (deck, plan) = deck_plan(&[CardId::SpotWeakness, CardId::SpotWeakness]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::HeavyBlade, 0);
        let evidence =
            assess_boss_scaling_evidence(plan, Some((CardId::HeavyBlade, 0)), &admission);

        assert_eq!(evidence.label, "boss-speculative-payoff");
        assert!(!evidence.relevant_to_boss_plan);
    }

    #[test]
    fn stable_strength_makes_strength_payoff_boss_relevant() {
        let (deck, plan) = deck_plan(&[CardId::Inflame]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::HeavyBlade, 0);
        let evidence =
            assess_boss_scaling_evidence(plan, Some((CardId::HeavyBlade, 0)), &admission);

        assert_eq!(evidence.label, "boss-strength-payoff");
        assert!(evidence.relevant_to_boss_plan);
    }

    #[test]
    fn one_controlled_exhaust_card_is_limited_synergy_not_a_complete_boss_engine() {
        let (deck, plan) = deck_plan(&[CardId::TrueGrit]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::DarkEmbrace, 1);
        let evidence =
            assess_boss_scaling_evidence(plan, Some((CardId::DarkEmbrace, 1)), &admission);

        assert_eq!(evidence.label, "boss-limited-exhaust-synergy");
        assert!(!evidence.relevant_to_boss_plan);
    }

    #[test]
    fn one_random_topdeck_exhaust_is_not_a_complete_boss_engine_source() {
        let (deck, plan) = deck_plan(&[CardId::FeelNoPain]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Havoc, 0);
        let evidence = assess_boss_scaling_evidence(plan, Some((CardId::Havoc, 0)), &admission);

        assert_eq!(evidence.label, "boss-limited-exhaust-source");
        assert!(!evidence.relevant_to_boss_plan);
    }

    #[test]
    fn mass_exhaust_throughput_makes_first_payoff_boss_relevant() {
        let (deck, plan) = deck_plan(&[CardId::SecondWind, CardId::Defend, CardId::Defend]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::FeelNoPain, 1);
        let evidence =
            assess_boss_scaling_evidence(plan, Some((CardId::FeelNoPain, 1)), &admission);

        assert_eq!(evidence.label, "boss-exhaust-engine-pair");
        assert!(evidence.relevant_to_boss_plan);
    }

    #[test]
    fn unsupported_rupture_is_not_a_usable_boss_scaling_source() {
        let (deck, plan) = deck_plan(&[CardId::Strike, CardId::Defend, CardId::Bash]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Rupture, 0);
        let evidence = assess_boss_scaling_evidence(plan, Some((CardId::Rupture, 0)), &admission);

        assert!(!evidence.relevant_to_boss_plan);
    }

    #[test]
    fn offering_backed_rupture_is_not_relevant_boss_scaling() {
        let (deck, plan) = deck_plan(&[CardId::Offering]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Rupture, 0);
        let evidence = assess_boss_scaling_evidence(plan, Some((CardId::Rupture, 0)), &admission);

        assert!(!evidence.relevant_to_boss_plan);
        assert_ne!(evidence.label, "boss-scaling-source");
    }

    #[test]
    fn repeated_rupture_does_not_repeat_full_boss_scaling_credit() {
        let (deck, plan) = deck_plan(&[CardId::Rupture, CardId::Hemokinesis]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Rupture, 1);
        let evidence = assess_boss_scaling_evidence(plan, Some((CardId::Rupture, 1)), &admission);

        assert!(!evidence.relevant_to_boss_plan);
        assert_eq!(evidence.score_delta, 0);
    }

    #[test]
    fn multiplier_with_one_stable_source_keeps_reliability_repair() {
        let (deck, plan) = deck_plan(&[CardId::Inflame, CardId::LimitBreak]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::DemonForm, 0);
        let evidence = assess_boss_scaling_evidence(plan, Some((CardId::DemonForm, 0)), &admission);

        assert!(evidence.relevant_to_boss_plan);
    }

    #[test]
    fn conditional_source_does_not_masquerade_as_reliability_repair() {
        let (deck, plan) = deck_plan(&[CardId::Inflame, CardId::LimitBreak]);
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::SpotWeakness, 1);
        let evidence =
            assess_boss_scaling_evidence(plan, Some((CardId::SpotWeakness, 1)), &admission);

        assert!(!evidence.relevant_to_boss_plan);
    }

    #[test]
    fn automaton_known_boss_makes_shockwave_boss_relevant() {
        let (deck, plan) = deck_plan(&[
            CardId::Strike,
            CardId::Defend,
            CardId::Immolate,
            CardId::FiendFire,
        ]);
        let plan = plan.with_boss_key(Some(EncounterId::Automaton));
        let admission = assess_reward_admission_from_master_deck(&deck, CardId::Shockwave, 0);
        let evidence = assess_boss_scaling_evidence(plan, Some((CardId::Shockwave, 0)), &admission);

        assert_eq!(evidence.label, "automaton-artifact-debuff-window");
        assert!(evidence.relevant_to_boss_plan);
    }
}
