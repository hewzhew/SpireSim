use crate::ai::analysis::card_semantics::{
    card_definition, CardDefinition, DeckMechanicContext, EventHandler, PayoffRequirement,
    PlayEffect, TriggeredEffect,
};
use crate::ai::strategy::package_state::{assess_package_state, PackageStateReport};
use crate::ai::strategy::package_transition::{
    assess_package_transition, PackageStateChange, PackageTransitionReport,
};
use crate::content::cards::CardId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardSemanticProbeReport {
    pub deck_package: PackageStateReport,
    pub candidates: Vec<RewardCandidateSemanticProbe>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardCandidateSemanticProbe {
    pub card: CardId,
    pub transition: PackageTransitionReport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardSemanticExplanationV1 {
    pub deck_package: PackageStateReport,
    pub candidates: Vec<RewardCandidateSemanticExplanationV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardCandidateSemanticExplanationV1 {
    pub card: CardId,
    pub coverage: RewardSemanticCoverageV1,
    pub package_changes: Vec<String>,
    pub closes: Vec<String>,
    pub opens: Vec<String>,
    pub provides: Vec<String>,
    pub damage: Vec<String>,
    pub scales: Vec<String>,
    pub damage_uses: Vec<String>,
    pub emits: Vec<String>,
    pub run_rewards: Vec<String>,
    pub rules: Vec<String>,
    pub handlers: Vec<String>,
    pub burdens: Vec<String>,
    pub duplicates: Vec<String>,
    pub new_mechanics: Vec<String>,
    pub new_streams: Vec<String>,
    pub new_rules: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewardSemanticCoverageV1 {
    pub status: RewardSemanticCoverageStatusV1,
    pub explained_fields: Vec<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RewardSemanticCoverageStatusV1 {
    Explained,
    Empty,
    DeferredSequenceTactical,
}

pub fn assess_reward_semantics(
    deck: &[CardDefinition],
    candidates: &[CardDefinition],
) -> RewardSemanticProbeReport {
    let deck_context = DeckMechanicContext::from_definitions(deck);
    RewardSemanticProbeReport {
        deck_package: assess_package_state(&deck_context),
        candidates: candidates
            .iter()
            .cloned()
            .map(|candidate| RewardCandidateSemanticProbe {
                card: candidate.card,
                transition: assess_package_transition(deck, candidate),
            })
            .collect(),
    }
}

pub fn assess_reward_semantics_from_cards(
    deck: &[CardId],
    candidates: &[CardId],
) -> RewardSemanticProbeReport {
    let deck_definitions = deck
        .iter()
        .copied()
        .map(card_definition)
        .collect::<Vec<_>>();
    let candidate_definitions = candidates
        .iter()
        .copied()
        .map(card_definition)
        .collect::<Vec<_>>();
    assess_reward_semantics(&deck_definitions, &candidate_definitions)
}

pub fn explain_reward_semantics_v1(
    report: &RewardSemanticProbeReport,
) -> RewardSemanticExplanationV1 {
    RewardSemanticExplanationV1 {
        deck_package: report.deck_package.clone(),
        candidates: report
            .candidates
            .iter()
            .map(explain_reward_candidate_semantics_v1)
            .collect(),
    }
}

fn explain_reward_candidate_semantics_v1(
    probe: &RewardCandidateSemanticProbe,
) -> RewardCandidateSemanticExplanationV1 {
    let transition = &probe.transition;
    let play_effects = explain_play_effects(&transition.candidate_play_effects);
    let package_changes = transition
        .package_changes
        .iter()
        .map(package_change_label)
        .collect::<Vec<_>>();
    let closes = transition
        .newly_closed_requirements
        .iter()
        .map(payoff_requirement_label)
        .collect::<Vec<_>>();
    let opens = transition
        .newly_open_requirements
        .iter()
        .map(payoff_requirement_label)
        .collect::<Vec<_>>();
    let provides = play_effects.provides;
    let damage = play_effects.damage;
    let scales = play_effects.scales;
    let damage_uses = play_effects.damage_uses;
    let emits = play_effects.emits;
    let run_rewards = play_effects.run_rewards;
    let rules = transition
        .candidate_installed_rules
        .iter()
        .map(debug_label)
        .collect::<Vec<_>>();
    let handlers = transition
        .candidate_event_handlers
        .iter()
        .map(event_handler_label)
        .collect::<Vec<_>>();
    let burdens = transition
        .candidate_burdens
        .iter()
        .map(debug_label)
        .collect::<Vec<_>>();
    let duplicates = transition
        .candidate_duplicate_behaviors
        .iter()
        .map(debug_label)
        .collect::<Vec<_>>();
    let new_mechanics = transition
        .new_mechanics
        .iter()
        .map(debug_label)
        .collect::<Vec<_>>();
    let new_streams = transition
        .new_event_streams
        .iter()
        .map(debug_label)
        .collect::<Vec<_>>();
    let new_rules = transition
        .new_installed_rules
        .iter()
        .map(debug_label)
        .collect::<Vec<_>>();
    let coverage = reward_semantic_coverage_v1(
        probe.card,
        &[
            ("package_changes", package_changes.as_slice()),
            ("closes", closes.as_slice()),
            ("opens", opens.as_slice()),
            ("provides", provides.as_slice()),
            ("damage", damage.as_slice()),
            ("scales", scales.as_slice()),
            ("damage_uses", damage_uses.as_slice()),
            ("emits", emits.as_slice()),
            ("run_rewards", run_rewards.as_slice()),
            ("rules", rules.as_slice()),
            ("handlers", handlers.as_slice()),
            ("burdens", burdens.as_slice()),
            ("duplicates", duplicates.as_slice()),
            ("new_mechanics", new_mechanics.as_slice()),
            ("new_streams", new_streams.as_slice()),
            ("new_rules", new_rules.as_slice()),
        ],
    );
    RewardCandidateSemanticExplanationV1 {
        card: probe.card,
        coverage,
        package_changes,
        closes,
        opens,
        provides,
        damage,
        scales,
        damage_uses,
        emits,
        run_rewards,
        rules,
        handlers,
        burdens,
        duplicates,
        new_mechanics,
        new_streams,
        new_rules,
    }
}

fn reward_semantic_coverage_v1(
    card: CardId,
    fields: &[(&'static str, &[String])],
) -> RewardSemanticCoverageV1 {
    let explained_fields = fields
        .iter()
        .filter_map(|(name, values)| (!values.is_empty()).then_some(*name))
        .collect::<Vec<_>>();
    let status = if !explained_fields.is_empty() {
        RewardSemanticCoverageStatusV1::Explained
    } else if is_deferred_sequence_tactical_candidate_v1(card) {
        RewardSemanticCoverageStatusV1::DeferredSequenceTactical
    } else {
        RewardSemanticCoverageStatusV1::Empty
    };
    RewardSemanticCoverageV1 {
        status,
        explained_fields,
    }
}

fn is_deferred_sequence_tactical_candidate_v1(card: CardId) -> bool {
    matches!(card, CardId::Rage)
}

#[derive(Default)]
struct PlayEffectExplanation {
    provides: Vec<String>,
    damage: Vec<String>,
    scales: Vec<String>,
    damage_uses: Vec<String>,
    emits: Vec<String>,
    run_rewards: Vec<String>,
}

fn explain_play_effects(effects: &[PlayEffect]) -> PlayEffectExplanation {
    let mut explanation = PlayEffectExplanation::default();
    for effect in effects {
        match effect {
            PlayEffect::Provide(mechanic) => {
                explanation.provides.push(debug_label(mechanic));
            }
            PlayEffect::FrontloadDamage => {
                explanation.damage.push("Frontload".to_string());
            }
            PlayEffect::AreaDamage => {
                explanation.damage.push("AoE".to_string());
            }
            PlayEffect::DamageUses(mechanic) => {
                explanation.damage_uses.push(debug_label(mechanic));
            }
            PlayEffect::DamageScalesWith(axis) => {
                explanation.scales.push(debug_label(axis));
            }
            PlayEffect::EmitEvent(event) => {
                explanation.emits.push(debug_label(event));
            }
            PlayEffect::ExhaustsSelf => {
                explanation.emits.push("SelfExhaust".to_string());
            }
            PlayEffect::RunReward(reward) => {
                explanation.run_rewards.push(debug_label(reward));
            }
            PlayEffect::RecoverCurrentHp => {
                explanation
                    .run_rewards
                    .push("CurrentHpRecovery".to_string());
            }
            PlayEffect::CostReducedByHpLossThisCombat => {
                explanation
                    .scales
                    .push("CostReducedByHpLossThisCombat".to_string());
            }
            PlayEffect::AddCombatDeckClutter => {
                explanation.emits.push("CombatDeckClutter".to_string());
            }
            PlayEffect::PlayTopCardAndExhaust => {
                explanation
                    .emits
                    .push("TopCardPlayedAndExhausted".to_string());
            }
            PlayEffect::CombatUpgradeSingle => {
                explanation.provides.push("CombatUpgradeSingle".to_string());
            }
            PlayEffect::CombatUpgradeAll => {
                explanation.provides.push("CombatUpgradeAll".to_string());
            }
        }
    }
    explanation
}

fn package_change_label(change: &PackageStateChange) -> String {
    format!("{:?}:{:?}->{:?}", change.package, change.from, change.to)
}

fn payoff_requirement_label(requirement: &PayoffRequirement) -> String {
    match requirement {
        PayoffRequirement::WantsMechanic(mechanic) => {
            format!("needs:{}", debug_label(mechanic))
        }
        PayoffRequirement::WantsEventStream(event) => {
            format!("needs_event:{}", debug_label(event))
        }
    }
}

fn event_handler_label(handler: &EventHandler) -> String {
    format!(
        "on:{}->{}",
        debug_label(&handler.on),
        triggered_effect_label(&handler.effect)
    )
}

fn triggered_effect_label(effect: &TriggeredEffect) -> String {
    match effect {
        TriggeredEffect::Provide(mechanic) => format!("Provide({})", debug_label(mechanic)),
        TriggeredEffect::LoseHpFromCard => "LoseHpFromCard".to_string(),
        TriggeredEffect::DealAllDamage => "DealAllDamage".to_string(),
    }
}

fn debug_label<T: std::fmt::Debug>(item: &T) -> String {
    format!("{item:?}")
}
