use crate::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CombatEvent, Mechanic, TriggeredEffect,
};
use crate::ai::card_analysis_v1::card_analysis_profile_v1;
use crate::ai::card_semantics_v1::{
    card_mechanics_profile_v1, card_owned_base_output_v1, card_reward_facts_v1,
    CardRewardPickDependencyV1, CardRewardStatusPersistenceV1,
};
use crate::ai::card_tactical_delivery_v1::{
    base_block_is_dense_v1, base_damage_is_dense_v1, card_tactical_delivery_profile_v1,
};
use crate::ai::deck_startup_profile_v1::{status_payoff_access_v1, StatusPayoffAccessV1};
use crate::content::cards::mechanics::{
    CardAttackHitsV1, CardAttackTargetsV1, CardBaseDamageDeliveryV1,
};
use crate::content::cards::{CardId, CardType};
use crate::content::relics::RelicId;
use crate::runtime::combat::CombatCard;
use crate::state::rewards::RewardCard;
use crate::state::run::RunState;

use super::types::{
    StrategyCapabilityCoverageV1 as Coverage, StrategyCapabilityEvidenceV1 as Capability,
    StrategyCapabilityInputKindV1 as InputKind, StrategyCapabilityInputV1 as Input,
    StrategyCapabilityKindV1 as Kind, StrategyDeckFactsV1, StrategyThreatCoverageGapV1,
    StrategyThreatCoverageLedgerV1, StrategyThreatProfileV1, StrategyThreatTagV1 as Tag,
};

#[derive(Default)]
struct CapabilityFacts {
    single_target_damage: i32,
    best_single_hit: i32,
    tactically_dense_single_target_damage: i32,
    best_tactically_dense_single_hit: i32,
    aoe_sources: usize,
    strong_aoe_sources: usize,
    context_dependent_aoe_sources: usize,
    block_total: i32,
    tactically_dense_block_total: i32,
    weak_sources: usize,
    reliable_weak_sources: usize,
    strength_down_sources: usize,
    persistent_strength_down_sources: usize,
    temporary_strength_down_sources: usize,
    context_dependent_block_sources: usize,
    scaling_sources: usize,
    scaling_payoffs: usize,
    draw_sources: usize,
    energy_sources: usize,
    exhaust_generators: usize,
    repeatable_exhaust_converters: usize,
    one_shot_exhaust_generators: usize,
    exhaust_fuel_cards: usize,
    targeted_exhaust_converters: usize,
    non_attack_exhaust_converters: usize,
    corruption_exhaust_converters: usize,
    any_card_exhaust_fuel: usize,
    non_attack_exhaust_fuel: usize,
    skill_exhaust_fuel: usize,
    generated_exhaust_fuel_per_cycle: usize,
    broad_exhaust_generators: usize,
    exhaust_payoffs: usize,
    exhaust_block_payoffs: usize,
    debuff_sources: usize,
    dense_card_sources: usize,
    retaliation_safe_sources: usize,
    status_draw_payoffs: usize,
    status_damage_payoffs: usize,
    broad_status_exhaust_converters: usize,
    conditional_status_exhaust_converters: usize,
    medical_kit_access: usize,
}

pub(super) const FRONTLOAD_SUPPORTED_TOTAL_DAMAGE_V1: i32 = 50;
const FRONTLOAD_STRONG_TOTAL_DAMAGE_V1: i32 = 80;
const STRONG_AOE_BASE_DAMAGE_PER_ENEMY_V1: i32 = 18;

pub fn threat_coverage_from_run_state_v1(
    run_state: &RunState,
    threats: &StrategyThreatProfileV1,
) -> StrategyThreatCoverageLedgerV1 {
    ledger_from_capability_facts(capability_facts_from_run_state(run_state), threats)
}

fn capability_facts_from_run_state(run_state: &RunState) -> CapabilityFacts {
    let mut facts = CapabilityFacts::default();
    let status_payoff_access = status_payoff_access_v1(run_state);
    for card in &run_state.master_deck {
        let output = card_owned_base_output_v1(card);
        let card = RewardCard::new(card.id, card.upgrades);
        let observed = card_reward_facts_v1(&card);
        let analysis = card_analysis_profile_v1(observed.card, card.upgrades);
        let tactical = card_tactical_delivery_profile_v1(observed.card, card.upgrades);
        let targeted_base_damage = output
            .damage
            .unconditional_targeted_base_total()
            .unwrap_or(0);
        if targeted_base_damage > 0 {
            facts.single_target_damage = facts
                .single_target_damage
                .saturating_add(targeted_base_damage);
            facts.best_single_hit = facts.best_single_hit.max(output.damage.damage_per_hit);
            if base_damage_is_dense_v1(
                output.energy_cost,
                output.damage.unconditional_base_total().unwrap_or(0),
            ) {
                facts.tactically_dense_single_target_damage = facts
                    .tactically_dense_single_target_damage
                    .saturating_add(targeted_base_damage);
                facts.best_tactically_dense_single_hit = facts
                    .best_tactically_dense_single_hit
                    .max(output.damage.damage_per_hit);
            }
        }
        if output.damage.delivery.and_then(|rule| rule.targets())
            == Some(CardAttackTargetsV1::EachEnemy)
        {
            // Opportunity labels (e.g. Whirlwind's potential with enough X)
            // cannot establish static coverage. Count reviewed owned output
            // per enemy; retain the unresolved mechanism separately.
            let per_enemy = output.damage.unconditional_base_total();
            facts.aoe_sources += usize::from(per_enemy.is_some_and(|amount| amount > 0));
            facts.strong_aoe_sources += usize::from(
                per_enemy.is_some_and(|amount| amount >= STRONG_AOE_BASE_DAMAGE_PER_ENEMY_V1),
            );
            facts.context_dependent_aoe_sources += usize::from(
                output
                    .damage
                    .delivery
                    .is_some_and(|rule| rule.has_context_dependent_attack()),
            );
        }
        let unconditional_block = output.unconditional_block.unwrap_or(0);
        facts.block_total = facts.block_total.saturating_add(unconditional_block);
        if base_block_is_dense_v1(output.energy_cost, unconditional_block) {
            facts.tactically_dense_block_total = facts
                .tactically_dense_block_total
                .saturating_add(unconditional_block);
        }
        facts.context_dependent_block_sources +=
            usize::from(observed.context_dependent_block_delivery_v1().is_some());
        facts.weak_sources += usize::from(observed.weak > 0);
        facts.reliable_weak_sources += usize::from(tactical.reliable_weak);
        facts.strength_down_sources += usize::from(observed.enemy_strength_down > 0);
        facts.persistent_strength_down_sources +=
            usize::from(observed.has_persistent_enemy_strength_reduction_v1());
        facts.temporary_strength_down_sources += usize::from(observed.enemy_strength_down > 0
            && observed.enemy_strength_restoration == Some(crate::content::cards::mechanics::EnemyStrengthRestorationV1::ShackledAtOwnerTurnEnd));
        facts.scaling_sources += usize::from(
            observed.strength_gain > 0
                && !card_mechanics_profile_v1(observed.card).temporary_strength_burst,
        );
        facts.scaling_payoffs += usize::from(
            observed
                .pick_dependencies
                .contains(&CardRewardPickDependencyV1::StrengthScaling),
        );
        let mechanics = card_mechanics_profile_v1(observed.card);
        facts.draw_sources +=
            usize::from(observed.draw_cards > i32::from(mechanics.hand_topdeck_selection));
        facts.energy_sources += usize::from(observed.energy_gain > 0);
        let is_exhaust_converter =
            observed.exhausts_other_cards || observed.card == CardId::Corruption;
        let is_targeted_exhaust_converter = matches!(
            observed.card,
            CardId::BurningPact | CardId::TrueGrit | CardId::Recycle
        );
        let is_non_attack_exhaust_converter =
            matches!(observed.card, CardId::SecondWind | CardId::SeverSoul);
        let is_corruption_exhaust_converter = observed.card == CardId::Corruption;
        let is_repeatable_exhaust_converter = is_targeted_exhaust_converter
            || is_non_attack_exhaust_converter
            || is_corruption_exhaust_converter;
        facts.exhaust_generators += usize::from(is_exhaust_converter);
        facts.repeatable_exhaust_converters += usize::from(is_repeatable_exhaust_converter);
        facts.one_shot_exhaust_generators +=
            usize::from(is_exhaust_converter && !is_repeatable_exhaust_converter);
        // Exhaust is not automatically status control. Corruption only affects
        // Skills, while Sever Soul pays two energy and also destroys every
        // non-Attack resource in hand. Second Wind is the one broad converter
        // here whose exhaust effect itself pays the player back with Block.
        facts.broad_status_exhaust_converters += usize::from(observed.card == CardId::SecondWind);
        facts.conditional_status_exhaust_converters += usize::from(
            is_targeted_exhaust_converter
                || observed.card == CardId::SeverSoul
                || (is_exhaust_converter && !is_repeatable_exhaust_converter),
        );
        facts.targeted_exhaust_converters += usize::from(is_targeted_exhaust_converter);
        facts.non_attack_exhaust_converters += usize::from(is_non_attack_exhaust_converter);
        facts.corruption_exhaust_converters += usize::from(is_corruption_exhaust_converter);
        let is_candidate_fuel =
            observed.card_type != CardType::Power && !is_exhaust_converter && !observed.exhausts;
        facts.any_card_exhaust_fuel += usize::from(is_candidate_fuel);
        facts.non_attack_exhaust_fuel +=
            usize::from(is_candidate_fuel && observed.card_type != CardType::Attack);
        facts.skill_exhaust_fuel +=
            usize::from(is_candidate_fuel && observed.card_type == CardType::Skill);
        facts.generated_exhaust_fuel_per_cycle =
            facts.generated_exhaust_fuel_per_cycle.saturating_add(
                observed
                    .status_injections
                    .iter()
                    .filter(|injection| {
                        injection.persistence == CardRewardStatusPersistenceV1::Persistent
                    })
                    .map(|injection| usize::try_from(injection.count.max(0)).unwrap_or(usize::MAX))
                    .fold(0usize, usize::saturating_add),
            );
        facts.broad_exhaust_generators += usize::from(analysis.is_block_plan_broad_exhaust_source);
        facts.exhaust_payoffs += usize::from(
            observed
                .pick_dependencies
                .contains(&CardRewardPickDependencyV1::ExhaustPackage),
        );
        let strategic_definition = card_definition_with_upgrades(observed.card, card.upgrades);
        facts.exhaust_block_payoffs +=
            usize::from(strategic_definition.event_handlers.iter().any(|handler| {
                handler.on == CombatEvent::CardExhausted
                    && handler.effect == TriggeredEffect::Provide(Mechanic::Block)
            }));
        facts.status_damage_payoffs +=
            usize::from(strategic_definition.event_handlers.iter().any(|handler| {
                handler.on == CombatEvent::StatusDrawn
                    && handler.effect == TriggeredEffect::DealAllDamage
            }));
        facts.status_draw_payoffs +=
            usize::from(strategic_definition.event_handlers.iter().any(|handler| {
                handler.on == CombatEvent::StatusDrawn
                    && handler.effect == TriggeredEffect::Provide(Mechanic::CardDraw)
            }));
        facts.debuff_sources += usize::from(observed.weak > 0)
            + usize::from(observed.vulnerable > 0)
            + usize::from(observed.enemy_strength_down > 0);
        facts.dense_card_sources += usize::from(
            output.energy_cost >= 2
                && (output
                    .damage
                    .unconditional_base_total()
                    .is_some_and(|amount| amount >= 15)
                    || unconditional_block >= 12
                    || observed.enemy_strength_down > 0),
        );
        facts.retaliation_safe_sources += usize::from(
            (matches!(
                output.damage.delivery,
                Some(CardBaseDamageDeliveryV1::Attack {
                    hits: CardAttackHitsV1::Fixed { count: 1 },
                    targets: CardAttackTargetsV1::SelectedEnemy,
                    play_condition: None,
                    ..
                })
            ) && targeted_base_damage >= 12)
                || matches!(
                    observed.card,
                    crate::content::cards::CardId::FlameBarrier
                        | crate::content::cards::CardId::Combust
                        | crate::content::cards::CardId::Juggernaut
                ),
        );
    }
    // A conditional status-draw payoff is a realized capability only when the
    // current deck owns a repeatable way to create its trigger.  Future enemy
    // status generation is encounter-local possibility, not durable deck
    // coverage.
    if status_payoff_access == StatusPayoffAccessV1::Connected {
        facts.retaliation_safe_sources = facts
            .retaliation_safe_sources
            .saturating_add(facts.status_damage_payoffs);
    }
    facts.exhaust_fuel_cards = [
        (facts.targeted_exhaust_converters > 0).then_some(facts.any_card_exhaust_fuel),
        (facts.non_attack_exhaust_converters > 0).then_some(facts.non_attack_exhaust_fuel),
        (facts.corruption_exhaust_converters > 0).then_some(facts.skill_exhaust_fuel),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0);
    facts.medical_kit_access = usize::from(
        run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::MedicalKit),
    );
    facts
}

#[cfg(test)]
mod damage_delivery_tests {
    use super::*;

    #[test]
    fn focused_damage_and_retaliation_facts_require_the_right_hit_scope() {
        let mut run = RunState::new(123, 0, false, "Ironclad");
        for (id, upgrades, total, retaliation) in [
            (CardId::Strike, 0, 6, 0),
            (CardId::WildStrike, 0, 12, 1),
            (CardId::GlassKnife, 1, 24, 0),
            (CardId::Ragnarok, 1, 0, 0),
            (CardId::Whirlwind, 1, 0, 0),
            (CardId::FiendFire, 1, 0, 0),
            (CardId::SignatureMove, 1, 0, 0),
            (CardId::Wish, 1, 0, 0),
        ] {
            let mut card = CombatCard::for_scenario(id, 1);
            card.upgrades = upgrades;
            run.master_deck = vec![card];
            let facts = capability_facts_from_run_state(&run);
            assert_eq!(facts.single_target_damage, total, "{id:?}");
            assert_eq!(facts.retaliation_safe_sources, retaliation, "{id:?}");
        }
    }
}

pub fn threat_coverage_from_deck_facts_v1(
    deck: &StrategyDeckFactsV1,
    threats: &StrategyThreatProfileV1,
) -> StrategyThreatCoverageLedgerV1 {
    let facts = CapabilityFacts {
        single_target_damage: deck.total_attack_damage,
        tactically_dense_single_target_damage: deck.total_attack_damage,
        block_total: deck.total_block,
        tactically_dense_block_total: deck.total_block,
        weak_sources: deck.weak_sources as usize,
        reliable_weak_sources: deck.weak_sources as usize,
        scaling_sources: deck.strength_sources as usize,
        scaling_payoffs: deck.strength_payoffs as usize,
        draw_sources: deck.draw_sources as usize,
        energy_sources: deck.energy_sources as usize,
        exhaust_generators: deck.exhaust_generators as usize,
        exhaust_payoffs: deck.exhaust_payoffs as usize,
        status_damage_payoffs: deck.status_payoffs as usize,
        debuff_sources: (deck.weak_sources + deck.vulnerable_sources) as usize,
        ..CapabilityFacts::default()
    };
    ledger_from_capability_facts(facts, threats)
}

pub fn threat_coverage_after_card_v1(
    run_state: &RunState,
    threats: &StrategyThreatProfileV1,
    card: CardId,
    upgrades: u8,
) -> StrategyThreatCoverageLedgerV1 {
    let mut trial = run_state.clone();
    let lineage = trial.allocate_master_deck_lineage();
    let mut candidate = CombatCard::with_lineage(card, lineage);
    candidate.upgrades = upgrades;
    trial.master_deck.push(candidate);
    threat_coverage_from_run_state_v1(&trial, threats)
}

pub fn threat_relevant_capability_improvements_v1(
    threats: &StrategyThreatProfileV1,
    before: &StrategyThreatCoverageLedgerV1,
    after: &StrategyThreatCoverageLedgerV1,
) -> Vec<Kind> {
    let mut improvements = Vec::new();
    for source in &threats.sources {
        for kind in required_capabilities(source.tag) {
            let before_coverage = before
                .capability(kind)
                .map(|capability| capability.coverage)
                .unwrap_or(Coverage::Unknown);
            let after_coverage = after
                .capability(kind)
                .map(|capability| capability.coverage)
                .unwrap_or(Coverage::Unknown);
            if after_coverage > before_coverage && !improvements.contains(&kind) {
                improvements.push(kind);
            }
        }
    }
    improvements
}

fn ledger_from_capability_facts(
    facts: CapabilityFacts,
    threats: &StrategyThreatProfileV1,
) -> StrategyThreatCoverageLedgerV1 {
    let mut ledger = StrategyThreatCoverageLedgerV1 {
        capabilities: vec![
            capability(
                Kind::SingleTargetFrontload,
                frontload_coverage(&facts),
                &facts,
                vec![format!(
                    "single_target_damage={} best_single_hit={} tactically_dense_single_target_damage={} best_tactically_dense_single_hit={}",
                    facts.single_target_damage,
                    facts.best_single_hit,
                    facts.tactically_dense_single_target_damage,
                    facts.best_tactically_dense_single_hit,
                )],
            ),
            capability(
                Kind::MultiTargetControl,
                multi_target_control_coverage(&facts),
                &facts,
                vec![format!(
                    "aoe_sources={} strong_aoe_sources={} context_dependent_aoe_sources={}",
                    facts.aoe_sources, facts.strong_aoe_sources, facts.context_dependent_aoe_sources
                )],
            ),
            capability(
                Kind::SustainedDefense,
                defense_coverage(&facts),
                &facts,
                vec![format!(
                    "block_total={} tactically_dense_block_total={} weak_sources={} reliable_weak_sources={} strength_down_sources={} persistent_strength_down_sources={} temporary_strength_down_sources={} context_dependent_block_sources={} broad_exhaust_generators={} exhaust_block_payoffs={}",
                    facts.block_total,
                    facts.tactically_dense_block_total,
                    facts.weak_sources,
                    facts.reliable_weak_sources,
                    facts.strength_down_sources,
                    facts.persistent_strength_down_sources,
                    facts.temporary_strength_down_sources,
                    facts.context_dependent_block_sources,
                    facts.broad_exhaust_generators,
                    facts.exhaust_block_payoffs
                )],
            ),
            capability(
                Kind::LongFightScaling,
                long_fight_coverage(&facts),
                &facts,
                vec![format!(
                    "scaling_sources={} scaling_payoffs={} exhaust_generators={} exhaust_payoffs={}",
                    facts.scaling_sources,
                    facts.scaling_payoffs,
                    facts.exhaust_generators,
                    facts.exhaust_payoffs
                )],
            ),
            capability(
                Kind::DrawEnergyConsistency,
                consistency_coverage(&facts),
                &facts,
                vec![format!(
                    "draw_sources={} energy_sources={} exhaust_generators={}",
                    facts.draw_sources, facts.energy_sources, facts.exhaust_generators
                )],
            ),
            capability(
                Kind::StatusControl,
                status_control_coverage(&facts),
                &facts,
                vec![format!(
                    "status_draw_payoffs={} status_damage_payoffs={} broad_status_exhaust_converters={} conditional_status_exhaust_converters={} medical_kit_access={}",
                    facts.status_draw_payoffs,
                    facts.status_damage_payoffs,
                    facts.broad_status_exhaust_converters,
                    facts.conditional_status_exhaust_converters,
                    facts.medical_kit_access,
                )],
            ),
            capability(
                Kind::PhaseControl,
                phase_coverage(&facts),
                &facts,
                vec![format!(
                    "best_single_hit={} best_tactically_dense_single_hit={} draw_sources={} scaling_sources={}",
                    facts.best_single_hit,
                    facts.best_tactically_dense_single_hit,
                    facts.draw_sources,
                    facts.scaling_sources
                )],
            ),
            capability(
                Kind::ArtifactStripAccess,
                artifact_strip_coverage(facts.debuff_sources),
                &facts,
                vec![format!(
                    "independent_debuff_applications={}",
                    facts.debuff_sources
                )],
            ),
            capability(
                Kind::CardPlayEfficiency,
                count_coverage(facts.dense_card_sources),
                &facts,
                vec![format!("dense_card_sources={}", facts.dense_card_sources)],
            ),
            capability(
                Kind::RetaliationSafeDamage,
                count_coverage(facts.retaliation_safe_sources),
                &facts,
                vec![format!(
                    "retaliation_safe_damage_sources={}",
                    facts.retaliation_safe_sources
                )],
            ),
            capability(
                Kind::TimedDamageRace,
                timed_race_coverage(&facts),
                &facts,
                vec![format!(
                    "single_target_damage={} draw_sources={} energy_sources={}",
                    facts.single_target_damage, facts.draw_sources, facts.energy_sources
                )],
            ),
        ],
        gaps: Vec::new(),
    };

    for source in &threats.sources {
        let required_capabilities = required_capabilities(source.tag);
        if required_capabilities.is_empty() {
            continue;
        }
        let uncovered = required_capabilities.iter().any(|kind| {
            ledger
                .capability(*kind)
                .map(|capability| capability.coverage.is_gap())
                .unwrap_or(false)
        });
        if uncovered {
            ledger.gaps.push(StrategyThreatCoverageGapV1 {
                tag: source.tag,
                source: source.source,
                subject: source.subject.clone(),
                required_capabilities,
                evidence: vec![source.evidence.clone()],
            });
        }
    }
    ledger.gaps.sort_by(|left, right| {
        (left.source, left.subject.as_str(), left.tag).cmp(&(
            right.source,
            right.subject.as_str(),
            right.tag,
        ))
    });
    ledger.gaps.dedup_by(|left, right| {
        left.source == right.source && left.subject == right.subject && left.tag == right.tag
    });
    ledger
}

fn capability(
    kind: Kind,
    coverage: Coverage,
    facts: &CapabilityFacts,
    evidence: Vec<String>,
) -> Capability {
    Capability {
        capability: kind,
        coverage,
        inputs: capability_inputs(kind, facts),
        evidence,
    }
}

fn capability_inputs(kind: Kind, facts: &CapabilityFacts) -> Vec<Input> {
    let count = |input, value: usize| Input {
        input,
        value: i32::try_from(value).unwrap_or(i32::MAX),
    };
    let scalar = |input, value| Input { input, value };

    match kind {
        Kind::SingleTargetFrontload => vec![
            scalar(InputKind::SingleTargetDamage, facts.single_target_damage),
            scalar(InputKind::BestSingleHit, facts.best_single_hit),
            scalar(
                InputKind::TacticallyDenseSingleTargetDamage,
                facts.tactically_dense_single_target_damage,
            ),
            scalar(
                InputKind::BestTacticallyDenseSingleHit,
                facts.best_tactically_dense_single_hit,
            ),
        ],
        Kind::MultiTargetControl => vec![
            count(InputKind::AoeSources, facts.aoe_sources),
            count(InputKind::StrongAoeSources, facts.strong_aoe_sources),
            count(
                InputKind::ContextDependentAoeSources,
                facts.context_dependent_aoe_sources,
            ),
        ],
        Kind::SustainedDefense => vec![
            scalar(InputKind::BlockTotal, facts.block_total),
            scalar(
                InputKind::TacticallyDenseBlockTotal,
                facts.tactically_dense_block_total,
            ),
            count(InputKind::WeakSources, facts.weak_sources),
            count(InputKind::ReliableWeakSources, facts.reliable_weak_sources),
            count(InputKind::StrengthDownSources, facts.strength_down_sources),
            count(
                InputKind::PersistentStrengthDownSources,
                facts.persistent_strength_down_sources,
            ),
            count(
                InputKind::TemporaryStrengthDownSources,
                facts.temporary_strength_down_sources,
            ),
            count(
                InputKind::ContextDependentBlockSources,
                facts.context_dependent_block_sources,
            ),
            count(InputKind::ExhaustGenerators, facts.exhaust_generators),
            count(
                InputKind::BroadExhaustGenerators,
                facts.broad_exhaust_generators,
            ),
            count(InputKind::ExhaustBlockPayoffs, facts.exhaust_block_payoffs),
        ],
        Kind::LongFightScaling => vec![
            count(InputKind::ScalingSources, facts.scaling_sources),
            count(InputKind::ScalingPayoffs, facts.scaling_payoffs),
            count(InputKind::ExhaustGenerators, facts.exhaust_generators),
            count(
                InputKind::RepeatableExhaustConverters,
                facts.repeatable_exhaust_converters,
            ),
            count(
                InputKind::OneShotExhaustGenerators,
                facts.one_shot_exhaust_generators,
            ),
            count(InputKind::ExhaustFuelCards, facts.exhaust_fuel_cards),
            count(
                InputKind::GeneratedExhaustFuelPerCycle,
                facts.generated_exhaust_fuel_per_cycle,
            ),
            count(InputKind::ExhaustPayoffs, facts.exhaust_payoffs),
        ],
        Kind::DrawEnergyConsistency => vec![
            count(InputKind::DrawSources, facts.draw_sources),
            count(InputKind::EnergySources, facts.energy_sources),
            count(InputKind::ExhaustGenerators, facts.exhaust_generators),
        ],
        Kind::StatusControl => vec![
            count(InputKind::StatusDrawPayoffs, facts.status_draw_payoffs),
            count(InputKind::StatusDamagePayoffs, facts.status_damage_payoffs),
            count(
                InputKind::BroadStatusExhaustConverters,
                facts.broad_status_exhaust_converters,
            ),
            count(
                InputKind::ConditionalStatusExhaustConverters,
                facts.conditional_status_exhaust_converters,
            ),
            count(InputKind::MedicalKitAccess, facts.medical_kit_access),
        ],
        Kind::PhaseControl => vec![
            scalar(InputKind::BestSingleHit, facts.best_single_hit),
            scalar(
                InputKind::BestTacticallyDenseSingleHit,
                facts.best_tactically_dense_single_hit,
            ),
            count(InputKind::DrawSources, facts.draw_sources),
            count(InputKind::ScalingSources, facts.scaling_sources),
        ],
        Kind::ArtifactStripAccess => vec![count(InputKind::DebuffSources, facts.debuff_sources)],
        Kind::CardPlayEfficiency => {
            vec![count(InputKind::DenseCardSources, facts.dense_card_sources)]
        }
        Kind::RetaliationSafeDamage => vec![count(
            InputKind::RetaliationSafeSources,
            facts.retaliation_safe_sources,
        )],
        Kind::TimedDamageRace => vec![
            scalar(InputKind::SingleTargetDamage, facts.single_target_damage),
            scalar(InputKind::BestSingleHit, facts.best_single_hit),
            scalar(
                InputKind::TacticallyDenseSingleTargetDamage,
                facts.tactically_dense_single_target_damage,
            ),
            scalar(
                InputKind::BestTacticallyDenseSingleHit,
                facts.best_tactically_dense_single_hit,
            ),
            count(InputKind::DrawSources, facts.draw_sources),
            count(InputKind::EnergySources, facts.energy_sources),
            count(InputKind::ExhaustGenerators, facts.exhaust_generators),
        ],
    }
}

fn count_coverage(count: usize) -> Coverage {
    match count {
        0 => Coverage::Missing,
        1 => Coverage::Supported,
        _ => Coverage::Strong,
    }
}

fn multi_target_control_coverage(facts: &CapabilityFacts) -> Coverage {
    if facts.strong_aoe_sources > 0 {
        Coverage::Strong
    } else if facts.aoe_sources > 0 {
        Coverage::Supported
    } else if facts.context_dependent_aoe_sources > 0 {
        Coverage::Thin
    } else {
        Coverage::Missing
    }
}

fn frontload_coverage(facts: &CapabilityFacts) -> Coverage {
    if facts.best_tactically_dense_single_hit >= 20
        || facts.tactically_dense_single_target_damage >= FRONTLOAD_STRONG_TOTAL_DAMAGE_V1
    {
        Coverage::Strong
    } else if facts.best_tactically_dense_single_hit >= 12
        || facts.tactically_dense_single_target_damage >= FRONTLOAD_SUPPORTED_TOTAL_DAMAGE_V1
    {
        Coverage::Supported
    } else if facts.single_target_damage > 0 {
        Coverage::Thin
    } else {
        Coverage::Missing
    }
}

fn defense_coverage(facts: &CapabilityFacts) -> Coverage {
    let supported_exhaust_block_engine =
        facts.exhaust_generators > 0 && facts.exhaust_block_payoffs > 0;
    let broad_exhaust_block_engine =
        facts.broad_exhaust_generators > 0 && facts.exhaust_block_payoffs > 0;
    if (facts.persistent_strength_down_sources > 0 || broad_exhaust_block_engine)
        && facts.tactically_dense_block_total >= 25
    {
        Coverage::Strong
    } else if facts.persistent_strength_down_sources > 0
        || facts.reliable_weak_sources > 0
        || facts.tactically_dense_block_total >= 30
        || supported_exhaust_block_engine
    {
        Coverage::Supported
    } else if facts.block_total > 0
        || facts.weak_sources > 0
        || facts.temporary_strength_down_sources > 0
        || facts.context_dependent_block_sources > 0
    {
        Coverage::Thin
    } else {
        Coverage::Missing
    }
}

fn long_fight_coverage(facts: &CapabilityFacts) -> Coverage {
    let direct_scaling_supported = facts.scaling_sources > 0;
    let direct_scaling_engine = facts.scaling_sources > 0 && facts.scaling_payoffs > 0;
    let exhaust_engine = facts.exhaust_generators > 0 && facts.exhaust_payoffs > 0;
    let repeatable_exhaust_engine = facts.repeatable_exhaust_converters > 0
        && facts.exhaust_payoffs > 0
        && (facts.exhaust_fuel_cards >= 3 || facts.generated_exhaust_fuel_per_cycle > 0);
    if facts.scaling_sources >= 2 || direct_scaling_engine || repeatable_exhaust_engine {
        Coverage::Strong
    } else if direct_scaling_supported || exhaust_engine {
        Coverage::Supported
    } else if facts.exhaust_generators > 0 {
        Coverage::Thin
    } else {
        Coverage::Missing
    }
}

fn artifact_strip_coverage(independent_applications: usize) -> Coverage {
    match independent_applications {
        0 => Coverage::Missing,
        1 => Coverage::Thin,
        2 => Coverage::Supported,
        _ => Coverage::Strong,
    }
}

fn consistency_coverage(facts: &CapabilityFacts) -> Coverage {
    if facts.draw_sources >= 2 && (facts.energy_sources > 0 || facts.exhaust_generators > 0) {
        Coverage::Strong
    } else if facts.draw_sources > 0 {
        Coverage::Supported
    } else if facts.energy_sources > 0 || facts.exhaust_generators > 0 {
        Coverage::Thin
    } else {
        Coverage::Missing
    }
}

fn status_control_coverage(facts: &CapabilityFacts) -> Coverage {
    let durable_sources = facts
        .status_draw_payoffs
        .saturating_add(facts.broad_status_exhaust_converters)
        .saturating_add(facts.medical_kit_access);
    let conditional_sources = facts
        .status_damage_payoffs
        .saturating_add(facts.conditional_status_exhaust_converters);
    if durable_sources >= 2 {
        Coverage::Strong
    } else if durable_sources == 1 {
        Coverage::Supported
    } else if conditional_sources > 0 {
        Coverage::Thin
    } else {
        Coverage::Missing
    }
}

fn phase_coverage(facts: &CapabilityFacts) -> Coverage {
    if facts.best_tactically_dense_single_hit >= 20
        && facts.draw_sources > 0
        && facts.scaling_sources > 0
    {
        Coverage::Strong
    } else if facts.best_tactically_dense_single_hit >= 15 && facts.draw_sources > 0 {
        Coverage::Supported
    } else if facts.best_tactically_dense_single_hit >= 12 || facts.scaling_sources > 0 {
        Coverage::Thin
    } else {
        Coverage::Missing
    }
}

fn timed_race_coverage(facts: &CapabilityFacts) -> Coverage {
    match (frontload_coverage(facts), consistency_coverage(facts)) {
        (Coverage::Strong, Coverage::Supported | Coverage::Strong) => Coverage::Strong,
        (Coverage::Supported | Coverage::Strong, _) => Coverage::Supported,
        (Coverage::Thin, _) => Coverage::Thin,
        _ => Coverage::Missing,
    }
}

fn required_capabilities(tag: Tag) -> Vec<Kind> {
    match tag {
        Tag::HighIncomingDamage
        | Tag::MultiHit
        | Tag::StrengthDebuffValuable
        | Tag::WeakValuable => {
            vec![Kind::SustainedDefense]
        }
        Tag::AoEValuable => vec![Kind::MultiTargetControl],
        Tag::ArtifactBlocksDebuff => vec![Kind::ArtifactStripAccess],
        Tag::StatusFlood => vec![Kind::StatusControl],
        Tag::SplitThreshold | Tag::ModeShiftThreshold => vec![Kind::PhaseControl],
        Tag::SkillPunish => vec![Kind::SingleTargetFrontload],
        Tag::PowerPunish | Tag::LongFightScaling | Tag::SetupWindow => {
            vec![Kind::LongFightScaling]
        }
        Tag::CardPlayLimit => vec![Kind::CardPlayEfficiency],
        Tag::RetaliationPunish => vec![Kind::RetaliationSafeDamage],
        Tag::TimedDamageRace => vec![Kind::TimedDamageRace],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sustained_defense_does_not_promote_temporary_strength_loss() {
        for id in [CardId::DarkShackles, CardId::PiercingWail] {
            let mut run = RunState::new(1, 0, false, "Ironclad");
            run.master_deck = vec![CombatCard::for_scenario(id, 1)];
            let ledger =
                threat_coverage_from_run_state_v1(&run, &StrategyThreatProfileV1::default());
            assert_eq!(
                ledger.capability(Kind::SustainedDefense).unwrap().coverage,
                Coverage::Thin,
                "{id:?} has a temporary contribution, not persistent mitigation"
            );
            let defense = ledger.capability(Kind::SustainedDefense).unwrap();
            assert_eq!(
                defense
                    .inputs
                    .iter()
                    .find(|v| v.input == InputKind::PersistentStrengthDownSources)
                    .unwrap()
                    .value,
                0
            );
            assert_eq!(
                defense
                    .inputs
                    .iter()
                    .find(|v| v.input == InputKind::TemporaryStrengthDownSources)
                    .unwrap()
                    .value,
                1
            );
            assert_eq!(
                ledger
                    .capability(Kind::ArtifactStripAccess)
                    .unwrap()
                    .coverage,
                Coverage::Thin
            );
        }
    }

    #[test]
    fn sustained_defense_block_total_excludes_unresolved_conditions() {
        for id in [
            CardId::Wish,
            CardId::EscapePlan,
            CardId::AutoShields,
            CardId::SecondWind,
            CardId::ReinforcedBody,
            CardId::Stack,
        ] {
            let mut run = RunState::new(1, 0, false, "Ironclad");
            let mut card = CombatCard::for_scenario(id, 1);
            card.upgrades = 1;
            run.master_deck = vec![card];
            let ledger =
                threat_coverage_from_run_state_v1(&run, &StrategyThreatProfileV1::default());
            let defense = ledger.capability(Kind::SustainedDefense).unwrap();
            assert_eq!(
                defense
                    .inputs
                    .iter()
                    .find(|v| v.input == InputKind::BlockTotal)
                    .unwrap()
                    .value,
                0,
                "{id:?} must retain its condition"
            );
            assert_eq!(
                defense
                    .inputs
                    .iter()
                    .find(|v| v.input == InputKind::ContextDependentBlockSources)
                    .unwrap()
                    .value,
                1
            );
            assert_eq!(defense.coverage, Coverage::Thin);
        }
    }

    #[test]
    fn sustained_defense_preserves_owned_growth_and_persistent_mitigation() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        let mut grown = CombatCard::for_scenario(CardId::GeneticAlgorithm, 1);
        grown.misc_value = 27;
        for (id, expected) in [
            (CardId::DarkShackles, Coverage::Thin),
            (CardId::Disarm, Coverage::Strong),
        ] {
            run.master_deck = vec![grown.clone(), CombatCard::for_scenario(id, 2)];
            let snapshot =
                crate::ai::noncombat_strategy_v1::build_run_strategy_snapshot_from_run_state_v2(
                    &run,
                );
            let defense = snapshot
                .threat_coverage
                .capability(Kind::SustainedDefense)
                .unwrap();
            assert_eq!(snapshot.v1.deck.total_block, 27);
            assert_eq!(
                defense
                    .inputs
                    .iter()
                    .find(|v| v.input == InputKind::BlockTotal)
                    .unwrap()
                    .value,
                27
            );
            assert_eq!(
                defense
                    .inputs
                    .iter()
                    .find(|v| v.input == InputKind::TacticallyDenseBlockTotal)
                    .unwrap()
                    .value,
                27
            );
            assert_eq!(defense.coverage, expected, "{id:?}");
        }
    }

    fn retaliation_coverage(cards: &[CardId]) -> Coverage {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.master_deck = cards
            .iter()
            .enumerate()
            .map(|(index, card)| CombatCard::for_scenario(*card, index as u32))
            .collect();
        threat_coverage_from_run_state_v1(&run, &StrategyThreatProfileV1::default())
            .capability(Kind::RetaliationSafeDamage)
            .expect("retaliation-safe capability")
            .coverage
    }

    #[test]
    fn repeated_weak_supports_defense_without_claiming_persistent_strength_down() {
        let weak_only = CapabilityFacts {
            block_total: 25,
            tactically_dense_block_total: 25,
            weak_sources: 2,
            reliable_weak_sources: 2,
            ..CapabilityFacts::default()
        };
        assert_eq!(defense_coverage(&weak_only), Coverage::Supported);

        let with_strength_down = CapabilityFacts {
            strength_down_sources: 1,
            persistent_strength_down_sources: 1,
            ..weak_only
        };
        assert_eq!(defense_coverage(&with_strength_down), Coverage::Strong);
    }

    #[test]
    fn status_flood_requires_status_control_not_generic_cycle_access() {
        let coverage = |card| {
            let mut run = RunState::new(1, 0, false, "Ironclad");
            run.master_deck = vec![CombatCard::for_scenario(card, 0)];
            threat_coverage_from_run_state_v1(&run, &StrategyThreatProfileV1::default())
                .capability(Kind::StatusControl)
                .expect("status control")
                .coverage
        };

        assert_eq!(
            required_capabilities(Tag::StatusFlood),
            vec![Kind::StatusControl]
        );
        assert_eq!(coverage(CardId::DeepBreath), Coverage::Missing);
        assert_eq!(coverage(CardId::Evolve), Coverage::Supported);
        assert_eq!(coverage(CardId::SecondWind), Coverage::Supported);
        assert_eq!(coverage(CardId::FireBreathing), Coverage::Thin);
        assert_eq!(coverage(CardId::SeverSoul), Coverage::Thin);
        assert_eq!(coverage(CardId::Corruption), Coverage::Missing);
    }

    #[test]
    fn expensive_low_density_attack_does_not_create_frontload_or_defense_coverage() {
        let coverage = |card, upgrades| {
            let mut run = RunState::new(1, 0, false, "Ironclad");
            let mut owned = CombatCard::for_scenario(card, 0);
            owned.upgrades = upgrades;
            run.master_deck = vec![owned];
            threat_coverage_from_run_state_v1(&run, &StrategyThreatProfileV1::default())
        };

        let clothesline = coverage(CardId::Clothesline, 0);
        assert_eq!(
            clothesline
                .capability(Kind::SingleTargetFrontload)
                .expect("frontload")
                .coverage,
            Coverage::Thin
        );
        assert_eq!(
            clothesline
                .capability(Kind::SustainedDefense)
                .expect("defense")
                .coverage,
            Coverage::Thin
        );
        assert_eq!(
            clothesline
                .capability(Kind::PhaseControl)
                .expect("phase control")
                .coverage,
            Coverage::Missing
        );

        let upgraded_clothesline = coverage(CardId::Clothesline, 1);
        assert_eq!(
            upgraded_clothesline
                .capability(Kind::SustainedDefense)
                .expect("upgraded weak defense")
                .coverage,
            Coverage::Supported
        );

        let carnage = coverage(CardId::Carnage, 0);
        assert_eq!(
            carnage
                .capability(Kind::SingleTargetFrontload)
                .expect("dense frontload")
                .coverage,
            Coverage::Strong
        );
    }

    #[test]
    fn conditional_aoe_does_not_claim_established_multi_target_control() {
        for id in [CardId::Whirlwind, CardId::GrandFinale, CardId::Blizzard] {
            let mut run = RunState::new(1, 0, false, "Ironclad");
            run.master_deck = vec![CombatCard::for_scenario(id, 1)];
            let ledger =
                threat_coverage_from_run_state_v1(&run, &StrategyThreatProfileV1::default());
            let area = ledger.capability(Kind::MultiTargetControl).unwrap();
            assert_eq!(area.coverage, Coverage::Thin, "{id:?}: {area:?}");
            assert_eq!(
                area.inputs
                    .iter()
                    .find(|input| input.input == InputKind::ContextDependentAoeSources)
                    .unwrap()
                    .value,
                1,
                "{id:?} retains its unresolved AoE mechanism",
            );
        }
    }

    #[test]
    fn multi_target_control_uses_owned_per_enemy_output() {
        for (id, damage, expected) in [
            (CardId::Immolate, 1, Coverage::Supported),
            (CardId::Immolate, 0, Coverage::Missing),
            (CardId::Cleave, 20, Coverage::Strong),
            (CardId::DaggerSpray, 10, Coverage::Strong),
            (CardId::SwordBoomerang, 20, Coverage::Missing),
        ] {
            let mut owned = CombatCard::for_scenario(id, 1);
            owned.base_damage_override = Some(damage);
            let mut run = RunState::new(1, 0, false, "Ironclad");
            run.master_deck = vec![owned];
            let ledger =
                threat_coverage_from_run_state_v1(&run, &StrategyThreatProfileV1::default());
            let area = ledger.capability(Kind::MultiTargetControl).unwrap();
            assert_eq!(area.coverage, expected, "{id:?}: {area:?}");
        }
    }

    #[test]
    fn adding_conditional_aoe_preserves_existing_fixed_coverage() {
        let mut run = RunState::new(1, 0, false, "Ironclad");
        run.master_deck = vec![CombatCard::for_scenario(CardId::Immolate, 1)];
        let after = threat_coverage_after_card_v1(
            &run,
            &StrategyThreatProfileV1::default(),
            CardId::Whirlwind,
            1,
        );
        assert_eq!(
            after.capability(Kind::MultiTargetControl).unwrap().coverage,
            Coverage::Strong
        );
    }

    #[test]
    fn several_light_aoe_sources_do_not_claim_strong_multi_target_control() {
        let two_light_sources = CapabilityFacts {
            aoe_sources: 2,
            strong_aoe_sources: 0,
            ..CapabilityFacts::default()
        };
        assert_eq!(
            multi_target_control_coverage(&two_light_sources),
            Coverage::Supported
        );

        let one_strong_source = CapabilityFacts {
            aoe_sources: 1,
            strong_aoe_sources: 1,
            ..CapabilityFacts::default()
        };
        assert_eq!(
            multi_target_control_coverage(&one_strong_source),
            Coverage::Strong
        );
    }

    #[test]
    fn unsupported_payoff_fragments_do_not_claim_long_fight_capability() {
        assert_eq!(
            long_fight_coverage(&CapabilityFacts {
                scaling_payoffs: 1,
                ..CapabilityFacts::default()
            }),
            Coverage::Missing
        );
        assert_eq!(
            long_fight_coverage(&CapabilityFacts {
                exhaust_payoffs: 1,
                ..CapabilityFacts::default()
            }),
            Coverage::Missing
        );
        assert_eq!(
            long_fight_coverage(&CapabilityFacts {
                scaling_sources: 1,
                scaling_payoffs: 1,
                ..CapabilityFacts::default()
            }),
            Coverage::Strong
        );
    }

    #[test]
    fn conditional_status_damage_requires_an_owned_status_generator() {
        assert_eq!(
            retaliation_coverage(&[CardId::FireBreathing]),
            Coverage::Missing
        );
        assert_eq!(
            retaliation_coverage(&[CardId::FireBreathing, CardId::Immolate]),
            Coverage::Missing
        );
        assert_eq!(
            retaliation_coverage(&[CardId::FireBreathing, CardId::RecklessCharge]),
            Coverage::Supported
        );
        assert_eq!(
            retaliation_coverage(&[CardId::FireBreathing, CardId::PowerThrough]),
            Coverage::Supported
        );
    }

    #[test]
    fn self_contained_retaliation_answer_remains_supported() {
        assert_eq!(
            retaliation_coverage(&[CardId::FlameBarrier]),
            Coverage::Supported
        );
    }
}
