use crate::ai::card_semantics_v1::{card_reward_semantic_profile_v1, CardRewardSemanticRoleV1};
use crate::content::cards::{get_card_definition, CardId, CardTag, CardType};
use crate::content::monsters::factory::EncounterId;
use crate::content::relics::RelicId;
use crate::runtime::combat::CombatCard;
use crate::state::core::{run_pending_choice_allows_card_for_run, RunPendingChoiceReason};
use crate::state::rewards::RewardCard;
use crate::state::run::RunState;

#[derive(Clone, Debug, PartialEq)]
pub struct OpeningHandTargetPlanV1 {
    pub kind: OpeningHandTargetKindV1,
    pub candidates: Vec<OpeningHandTargetCandidateV1>,
    pub best_deck_index: Option<usize>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpeningHandTargetCandidateV1 {
    pub deck_index: Option<usize>,
    pub card: CardId,
    pub upgrades: u8,
    pub label: String,
    pub kind: OpeningHandTargetKindV1,
    pub roles: Vec<OpeningHandTargetRoleV1>,
    pub verdict: OpeningHandTargetVerdictV1,
    pub debt_tier: OpeningHandDebtTierV1,
    pub score_hint: i32,
    pub evidence: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpeningHandTargetKindV1 {
    Attack,
    Skill,
    Power,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OpeningHandTargetRoleV1 {
    Frontload,
    HighImpactFrontload,
    AoeDamage,
    DebuffAccess,
    Mitigation,
    AccessAcceleration,
    EnergyAcceleration,
    EngineSetup,
    ScalingSetup,
    SupportDependentPayoff,
    ContextDependentOutput,
    LowValueStarter,
    BossConflict,
    HighEnergyBurden,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum OpeningHandTargetVerdictV1 {
    AvoidUnlessForced,
    Situational,
    AcceptableFallback,
    GoodTarget,
    PremiumTarget,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub enum OpeningHandDebtTierV1 {
    #[default]
    None,
    Mild,
    Situational,
    High,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpeningHandTargetProfileV1 {
    pub verdict: Option<OpeningHandTargetVerdictV1>,
    pub debt_tier: OpeningHandDebtTierV1,
    pub score_hint: i32,
    pub signals: Vec<String>,
    pub risks: Vec<String>,
}

pub fn plan_opening_hand_targets_v1(
    run_state: &RunState,
    reason: RunPendingChoiceReason,
) -> Option<OpeningHandTargetPlanV1> {
    let kind = opening_hand_target_kind_for_reason_v1(reason)?;
    let mut candidates = run_state
        .master_deck
        .iter()
        .enumerate()
        .filter(|(_, card)| run_pending_choice_allows_card_for_run(&reason, card, run_state))
        .map(|(idx, card)| opening_hand_target_candidate_v1(run_state, kind, Some(idx), card))
        .collect::<Vec<_>>();
    candidates.sort_by(compare_opening_hand_target_candidates_v1);
    let best_deck_index = candidates
        .iter()
        .find(|candidate| candidate.verdict > OpeningHandTargetVerdictV1::AvoidUnlessForced)
        .or_else(|| candidates.first())
        .and_then(|candidate| candidate.deck_index);
    Some(OpeningHandTargetPlanV1 {
        kind,
        candidates,
        best_deck_index,
        notes: vec!["opening hand target plan is behavior_policy_not_teacher".to_string()],
    })
}

pub fn opening_hand_target_profile_for_card_v1(
    run_state: &RunState,
    reason: RunPendingChoiceReason,
    card: &CombatCard,
) -> OpeningHandTargetProfileV1 {
    let Some(kind) = opening_hand_target_kind_for_reason_v1(reason) else {
        return OpeningHandTargetProfileV1::default();
    };
    opening_hand_target_candidate_v1(run_state, kind, None, card).into()
}

pub fn opening_hand_target_debt_tags_v1(
    run_state: &RunState,
    relic: RelicId,
    card: CardId,
    upgrades: u8,
) -> Vec<String> {
    let Some(kind) = opening_hand_target_kind_for_relic_v1(relic) else {
        return Vec::new();
    };
    let candidate = opening_hand_target_candidate_from_card_id_v1(run_state, kind, card, upgrades);
    let mut tags = Vec::new();
    match candidate.debt_tier {
        OpeningHandDebtTierV1::High => tags.push("bottle_debt:high_opening_hand".to_string()),
        OpeningHandDebtTierV1::Situational => {
            tags.push("bottle_debt:situational_opening_hand".to_string())
        }
        OpeningHandDebtTierV1::Mild | OpeningHandDebtTierV1::None => {}
    }
    if candidate
        .roles
        .contains(&OpeningHandTargetRoleV1::BossConflict)
    {
        tags.push("bottle_debt:power_vs_awakened_one".to_string());
    }
    if candidate
        .roles
        .contains(&OpeningHandTargetRoleV1::SupportDependentPayoff)
    {
        let semantic = card_reward_semantic_profile_v1(&RewardCard::new(card, upgrades));
        if semantic
            .roles
            .contains(&CardRewardSemanticRoleV1::TemporaryStrengthBurst)
        {
            tags.push("bottle_debt:temporary_strength_burst".to_string());
        }
    }
    tags
}

pub fn opening_hand_target_kind_for_reason_v1(
    reason: RunPendingChoiceReason,
) -> Option<OpeningHandTargetKindV1> {
    match reason {
        RunPendingChoiceReason::BottleFlame => Some(OpeningHandTargetKindV1::Attack),
        RunPendingChoiceReason::BottleLightning => Some(OpeningHandTargetKindV1::Skill),
        RunPendingChoiceReason::BottleTornado => Some(OpeningHandTargetKindV1::Power),
        _ => None,
    }
}

pub fn opening_hand_target_kind_for_relic_v1(relic: RelicId) -> Option<OpeningHandTargetKindV1> {
    match relic {
        RelicId::BottledFlame => Some(OpeningHandTargetKindV1::Attack),
        RelicId::BottledLightning => Some(OpeningHandTargetKindV1::Skill),
        RelicId::BottledTornado => Some(OpeningHandTargetKindV1::Power),
        _ => None,
    }
}

fn opening_hand_target_candidate_v1(
    run_state: &RunState,
    kind: OpeningHandTargetKindV1,
    deck_index: Option<usize>,
    card: &CombatCard,
) -> OpeningHandTargetCandidateV1 {
    opening_hand_target_candidate_from_card_id_with_index_v1(
        run_state,
        kind,
        deck_index,
        card.id,
        card.upgrades,
    )
}

fn opening_hand_target_candidate_from_card_id_v1(
    run_state: &RunState,
    kind: OpeningHandTargetKindV1,
    card: CardId,
    upgrades: u8,
) -> OpeningHandTargetCandidateV1 {
    opening_hand_target_candidate_from_card_id_with_index_v1(run_state, kind, None, card, upgrades)
}

fn opening_hand_target_candidate_from_card_id_with_index_v1(
    run_state: &RunState,
    kind: OpeningHandTargetKindV1,
    deck_index: Option<usize>,
    card: CardId,
    upgrades: u8,
) -> OpeningHandTargetCandidateV1 {
    let semantic = card_reward_semantic_profile_v1(&RewardCard::new(card, upgrades));
    let definition = get_card_definition(card);
    let has = |role| semantic.roles.contains(&role);
    let mut roles = Vec::new();
    let mut evidence = Vec::new();
    let mut risks = Vec::new();

    match kind {
        OpeningHandTargetKindV1::Attack => {
            if has(CardRewardSemanticRoleV1::AoeDamage) {
                push_role(&mut roles, OpeningHandTargetRoleV1::AoeDamage);
            }
            if has(CardRewardSemanticRoleV1::Vulnerable) {
                push_role(&mut roles, OpeningHandTargetRoleV1::DebuffAccess);
            }
            if has(CardRewardSemanticRoleV1::FrontloadDamage) {
                push_role(&mut roles, OpeningHandTargetRoleV1::Frontload);
            }
            if definition.base_damage >= 18 || definition.base_magic >= 4 {
                push_role(&mut roles, OpeningHandTargetRoleV1::HighImpactFrontload);
            }
            if has(CardRewardSemanticRoleV1::StrengthPayoff)
                || has(CardRewardSemanticRoleV1::BlockPayoff)
                || has(CardRewardSemanticRoleV1::ConditionalPlayability)
            {
                push_role(&mut roles, OpeningHandTargetRoleV1::SupportDependentPayoff);
            }
        }
        OpeningHandTargetKindV1::Skill => {
            if has(CardRewardSemanticRoleV1::CardDraw) {
                push_role(&mut roles, OpeningHandTargetRoleV1::AccessAcceleration);
            }
            if has(CardRewardSemanticRoleV1::EnergySource) {
                push_role(&mut roles, OpeningHandTargetRoleV1::EnergyAcceleration);
            }
            if has(CardRewardSemanticRoleV1::Weak)
                || has(CardRewardSemanticRoleV1::EnemyStrengthDown)
            {
                push_role(&mut roles, OpeningHandTargetRoleV1::Mitigation);
            }
            if has(CardRewardSemanticRoleV1::ExhaustGenerator) {
                push_role(&mut roles, OpeningHandTargetRoleV1::EngineSetup);
            }
            if has(CardRewardSemanticRoleV1::TemporaryStrengthBurst)
                || has(CardRewardSemanticRoleV1::EnergyOnExhaust)
                || has(CardRewardSemanticRoleV1::ConditionalPlayability)
                || has(CardRewardSemanticRoleV1::RandomOutput)
            {
                push_role(&mut roles, OpeningHandTargetRoleV1::SupportDependentPayoff);
            }
        }
        OpeningHandTargetKindV1::Power => {
            if has(CardRewardSemanticRoleV1::ScalingSource) {
                push_role(&mut roles, OpeningHandTargetRoleV1::ScalingSetup);
            }
            if has(CardRewardSemanticRoleV1::ExhaustGenerator)
                || has(CardRewardSemanticRoleV1::ExhaustPayoff)
                || has(CardRewardSemanticRoleV1::BlockRetention)
            {
                push_role(&mut roles, OpeningHandTargetRoleV1::EngineSetup);
            }
            if has(CardRewardSemanticRoleV1::StatusPayoff)
                || has(CardRewardSemanticRoleV1::PackagePayoff)
                || has(CardRewardSemanticRoleV1::ConditionalPlayability)
                || has(CardRewardSemanticRoleV1::RandomOutput)
            {
                push_role(&mut roles, OpeningHandTargetRoleV1::ContextDependentOutput);
            }
            if run_state.boss_key == Some(EncounterId::AwakenedOne)
                && roles.contains(&OpeningHandTargetRoleV1::ContextDependentOutput)
            {
                push_role(&mut roles, OpeningHandTargetRoleV1::BossConflict);
            }
        }
    }

    if is_low_value_starter(definition.card_type, definition.tags) {
        push_role(&mut roles, OpeningHandTargetRoleV1::LowValueStarter);
        risks.push("starter card is a low-value fixed opening draw".to_string());
    }
    if definition.cost >= 2 {
        push_role(&mut roles, OpeningHandTargetRoleV1::HighEnergyBurden);
        risks.push("opening target has high energy cost".to_string());
    }

    let verdict = opening_hand_target_verdict(&roles);
    let debt_tier = opening_hand_debt_tier(verdict, &roles);
    let score_hint = opening_hand_target_score_hint(verdict, debt_tier, &roles);
    evidence.push(format!("opening_hand_target_verdict={verdict:?}"));
    evidence.push(format!("opening_hand_debt={debt_tier:?}"));
    if !roles.is_empty() {
        evidence.push(format!(
            "opening_hand_roles={}",
            roles
                .iter()
                .map(|role| role.label())
                .collect::<Vec<_>>()
                .join(",")
        ));
    }

    OpeningHandTargetCandidateV1 {
        deck_index,
        card,
        upgrades,
        label: card_label(card, upgrades),
        kind,
        roles,
        verdict,
        debt_tier,
        score_hint,
        evidence,
        risks,
    }
}

fn opening_hand_target_verdict(roles: &[OpeningHandTargetRoleV1]) -> OpeningHandTargetVerdictV1 {
    if roles.contains(&OpeningHandTargetRoleV1::LowValueStarter)
        || roles.contains(&OpeningHandTargetRoleV1::BossConflict)
    {
        return OpeningHandTargetVerdictV1::AvoidUnlessForced;
    }
    if roles.contains(&OpeningHandTargetRoleV1::SupportDependentPayoff)
        || roles.contains(&OpeningHandTargetRoleV1::ContextDependentOutput)
    {
        return OpeningHandTargetVerdictV1::Situational;
    }
    let premium = roles.contains(&OpeningHandTargetRoleV1::EnergyAcceleration)
        || roles.contains(&OpeningHandTargetRoleV1::AccessAcceleration)
        || roles.contains(&OpeningHandTargetRoleV1::EngineSetup)
        || roles.contains(&OpeningHandTargetRoleV1::ScalingSetup);
    if premium {
        return OpeningHandTargetVerdictV1::PremiumTarget;
    }
    let good = roles.contains(&OpeningHandTargetRoleV1::DebuffAccess)
        || roles.contains(&OpeningHandTargetRoleV1::Mitigation)
        || roles.contains(&OpeningHandTargetRoleV1::AoeDamage)
        || roles.contains(&OpeningHandTargetRoleV1::HighImpactFrontload);
    if good {
        return OpeningHandTargetVerdictV1::GoodTarget;
    }
    if roles.contains(&OpeningHandTargetRoleV1::Frontload) {
        return OpeningHandTargetVerdictV1::AcceptableFallback;
    }
    OpeningHandTargetVerdictV1::Situational
}

fn opening_hand_debt_tier(
    verdict: OpeningHandTargetVerdictV1,
    roles: &[OpeningHandTargetRoleV1],
) -> OpeningHandDebtTierV1 {
    match verdict {
        OpeningHandTargetVerdictV1::AvoidUnlessForced => OpeningHandDebtTierV1::High,
        OpeningHandTargetVerdictV1::Situational => OpeningHandDebtTierV1::Situational,
        OpeningHandTargetVerdictV1::AcceptableFallback => {
            if roles.contains(&OpeningHandTargetRoleV1::HighEnergyBurden) {
                OpeningHandDebtTierV1::Mild
            } else {
                OpeningHandDebtTierV1::None
            }
        }
        OpeningHandTargetVerdictV1::GoodTarget | OpeningHandTargetVerdictV1::PremiumTarget => {
            if roles.contains(&OpeningHandTargetRoleV1::HighEnergyBurden) {
                OpeningHandDebtTierV1::Mild
            } else {
                OpeningHandDebtTierV1::None
            }
        }
    }
}

fn opening_hand_target_score_hint(
    verdict: OpeningHandTargetVerdictV1,
    debt_tier: OpeningHandDebtTierV1,
    roles: &[OpeningHandTargetRoleV1],
) -> i32 {
    let verdict_rank = match verdict {
        OpeningHandTargetVerdictV1::PremiumTarget => 1_200,
        OpeningHandTargetVerdictV1::GoodTarget => 900,
        OpeningHandTargetVerdictV1::AcceptableFallback => 600,
        OpeningHandTargetVerdictV1::Situational => 250,
        OpeningHandTargetVerdictV1::AvoidUnlessForced => 0,
    };
    let debt_penalty = match debt_tier {
        OpeningHandDebtTierV1::None => 0,
        OpeningHandDebtTierV1::Mild => 80,
        OpeningHandDebtTierV1::Situational => 220,
        OpeningHandDebtTierV1::High => 420,
    };
    let role_bonus = roles
        .iter()
        .filter(|role| {
            !matches!(
                role,
                OpeningHandTargetRoleV1::HighEnergyBurden
                    | OpeningHandTargetRoleV1::LowValueStarter
                    | OpeningHandTargetRoleV1::BossConflict
            )
        })
        .count() as i32
        * 20;
    verdict_rank + role_bonus - debt_penalty
}

fn compare_opening_hand_target_candidates_v1(
    left: &OpeningHandTargetCandidateV1,
    right: &OpeningHandTargetCandidateV1,
) -> std::cmp::Ordering {
    right
        .score_hint
        .cmp(&left.score_hint)
        .then_with(|| right.verdict.cmp(&left.verdict))
        .then_with(|| left.label.cmp(&right.label))
}

fn is_low_value_starter(card_type: CardType, tags: &[CardTag]) -> bool {
    card_type != CardType::Curse
        && (tags.contains(&CardTag::StarterStrike) || tags.contains(&CardTag::StarterDefend))
}

fn push_role(roles: &mut Vec<OpeningHandTargetRoleV1>, role: OpeningHandTargetRoleV1) {
    if !roles.contains(&role) {
        roles.push(role);
    }
}

fn card_label(card: CardId, upgrades: u8) -> String {
    let name = get_card_definition(card).name;
    match upgrades {
        0 => name.to_string(),
        1 => format!("{name}+"),
        upgrades => format!("{name}+{upgrades}"),
    }
}

impl OpeningHandTargetRoleV1 {
    fn label(self) -> &'static str {
        match self {
            OpeningHandTargetRoleV1::Frontload => "frontload",
            OpeningHandTargetRoleV1::HighImpactFrontload => "high_impact_frontload",
            OpeningHandTargetRoleV1::AoeDamage => "aoe_damage",
            OpeningHandTargetRoleV1::DebuffAccess => "debuff_access",
            OpeningHandTargetRoleV1::Mitigation => "mitigation",
            OpeningHandTargetRoleV1::AccessAcceleration => "access_acceleration",
            OpeningHandTargetRoleV1::EnergyAcceleration => "energy_acceleration",
            OpeningHandTargetRoleV1::EngineSetup => "engine_setup",
            OpeningHandTargetRoleV1::ScalingSetup => "scaling_setup",
            OpeningHandTargetRoleV1::SupportDependentPayoff => "support_dependent_payoff",
            OpeningHandTargetRoleV1::ContextDependentOutput => "context_dependent_output",
            OpeningHandTargetRoleV1::LowValueStarter => "low_value_starter",
            OpeningHandTargetRoleV1::BossConflict => "boss_conflict",
            OpeningHandTargetRoleV1::HighEnergyBurden => "high_energy_burden",
        }
    }
}

impl From<OpeningHandTargetCandidateV1> for OpeningHandTargetProfileV1 {
    fn from(candidate: OpeningHandTargetCandidateV1) -> Self {
        let mut signals = candidate.evidence;
        signals.push(format!("opening_hand_score_hint={}", candidate.score_hint));
        OpeningHandTargetProfileV1 {
            verdict: Some(candidate.verdict),
            debt_tier: candidate.debt_tier,
            score_hint: candidate.score_hint,
            signals,
            risks: candidate.risks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::combat::CombatCard;

    #[test]
    fn opening_hand_target_marks_starter_strike_as_forced_fallback() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        let profile = opening_hand_target_profile_for_card_v1(
            &run_state,
            RunPendingChoiceReason::BottleFlame,
            &CombatCard::for_scenario(CardId::Strike, 1),
        );

        assert_eq!(
            profile.verdict,
            Some(OpeningHandTargetVerdictV1::AvoidUnlessForced)
        );
        assert_eq!(profile.debt_tier, OpeningHandDebtTierV1::High);
        assert!(profile
            .signals
            .iter()
            .any(|signal| signal.contains("low_value_starter")));
    }

    #[test]
    fn opening_hand_target_prefers_bash_over_starter_strike_for_flame() {
        let run_state = RunState::new(1, 0, false, "Ironclad");
        let plan = plan_opening_hand_targets_v1(&run_state, RunPendingChoiceReason::BottleFlame)
            .expect("bottle flame plan");

        let best = plan.candidates.first().expect("best candidate");
        assert_eq!(best.card, CardId::Bash);
        assert!(best.roles.contains(&OpeningHandTargetRoleV1::DebuffAccess));
    }
}
