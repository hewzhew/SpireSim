//! Typed, read-only combat-plan projections.
//!
//! This layer describes the semantic job a combat line is currently trying
//! to complete. It deliberately does not rank actions, assign scalar value, or
//! claim that a finite-budget search result is correct. Exact simulation stays
//! authoritative; a future search adapter may only treat plan deviations as
//! non-authoritative discrepancy.

use serde::{Deserialize, Serialize};

use sts_card_semantics::ai::analysis::card_semantics::{
    card_definition_with_upgrades, CombatEvent, Mechanic, PlayEffect,
};
use sts_core::content::cards::{self, exhausts_when_played, get_card_definition, CardId, CardType};
use sts_core::content::monsters::EnemyId;
use sts_core::content::powers::PowerId;
use sts_core::runtime::combat::{CardLineageId, CombatCard, CombatState, MonsterEntity};
use sts_core::sim::combat::{combat_terminal, CombatPosition, CombatTerminal};
use sts_core::sim::combat_action_surface::{
    CombatSelectionActionFamilyV2, CombatSelectionReasonV2,
};
use sts_core::sim::combat_projection::{project_monster_move_preview_in_combat, VisibleIntentKind};
use sts_core::state::core::{ClientInput, HandSelectReason};

mod bronze_automaton;
mod champ;
mod double_thief;

pub use bronze_automaton::{bronze_automaton_combat_plan_v1, bronze_automaton_plan_transition_v1};
pub use champ::{champ_combat_plan_v1, champ_plan_transition_v1};
pub use double_thief::combat_plan_turn_prefix_proposal_v1;

pub const COMBAT_PLAN_SCHEMA_V1: &str = "typed-combat-plan/v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatPlanIdV1 {
    AwakenedOnePhaseControl,
    BronzeAutomatonControl,
    ChampPhaseControl,
    DonuAndDecaGrowthControl,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatPlanStageV1 {
    RemoveEscalatingAdds,
    ExposeAttackMitigationTarget,
    RecoverWithheldSurvivalResource,
    PrepareForcedAttackWindow,
    SurviveForcedAttackWindow,
    ExploitForcedAttackRecoveryWindow,
    PrepareFirstPhaseCommit,
    ExploitTransitionWindow,
    SurviveSecondPhaseOpening,
    PrepareThresholdCommit,
    AwaitDebuffCleanse,
    SurviveExecuteWindow,
    EliminateTeamGrowthSource,
    ConvertToLethal,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatPlanMilestoneV1 {
    EscalatingAddsRemoved,
    AttackMitigationTargetExposed,
    WithheldSurvivalResourceRecovered,
    ForcedAttackWindowReached,
    ForcedAttackWindowSurvived,
    ForcedAttackRecoveryWindowClosed,
    UntaxedTransitionWindowReached,
    TransitionWindowClosed,
    SecondPhaseOpeningSurvived,
    ThresholdCommitted,
    DebuffCleanseCompleted,
    ExecuteWindowSurvived,
    TeamGrowthSourceEliminated,
    EncounterDefeated,
}

/// One exact action would consume a plan-owned resource before the milestone
/// where that resource is meant to be deployed.
///
/// This is a categorical timing fact, not an action score. Search may use it
/// to propose a different legal action, but exact simulation remains
/// authoritative and every alternative stays searchable.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatPlanActionDeferralV1 {
    PreserveFiniteSkillConversionUntilUntaxedWindow,
    PreserveUndeployedPlanAsset,
    PreserveArtifactSensitiveMitigationUntilTargetExposed,
}

/// Plan-owned timing class for one exact action successor.
///
/// Search may use this categorical order before consulting its ordinary
/// action prior. It must not turn the class into terminal evidence or prune a
/// legal alternative.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatPlanActionTimingV1 {
    PreferNow,
    Neutral,
    Defer(CombatPlanActionDeferralV1),
}

/// One stable semantic step in an encounter-owned current-turn proposal.
///
/// Card identity is carried by lineage because hand indices shift after every
/// play. The planner resolves the lineage against each exact intermediate state
/// and still requires the resulting ordinary input to be legal.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CombatPlanPrefixStepV1 {
    PlayCard {
        card_lineage: CardLineageId,
        target: Option<usize>,
    },
    EndTurn,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatPlanPrefixKindV1 {
    SplitThiefPressureAroundDefensiveBridge,
    SecureThiefKillBehindExhaustBlock,
    PressSingleThiefEscapeWindow,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatPlanPrefixServiceScopeV1 {
    RootEligible,
    ContinuationOnly,
}

/// One non-authoritative exact-turn proposal owned by encounter semantics.
///
/// It supplies neither a terminal claim nor a pruning rule. Generic planning
/// resolves and simulates every step as an ordinary exact graph edge.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatPlanTurnPrefixProposalV1 {
    pub kind: CombatPlanPrefixKindV1,
    pub service_scope: CombatPlanPrefixServiceScopeV1,
    pub steps: Vec<CombatPlanPrefixStepV1>,
}

/// Opaque, plan-owned lexicographic guidance for one exact state.
///
/// Components have meaning only to the encounter plan which produced them.
/// Generic search may compare two ranks from this same guide lane, but must
/// not add the components together or interpret them as calibrated value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatPlanStateGuideRankV1 {
    components: Vec<i32>,
}

impl CombatPlanStateGuideRankV1 {
    pub fn components(&self) -> &[i32] {
        &self.components
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum FiniteSkillConversionStateV1 {
    #[default]
    Unavailable,
    Available,
    Active,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CombatPlanObligationV1 {
    EliminateEscalatingAdds {
        remaining: u8,
    },
    AccountForReactivePowerTax {
        strength_per_power: i32,
    },
    PreserveFiniteSkillFuel {
        remaining_skills: u16,
        conversion: FiniteSkillConversionStateV1,
    },
    PreservePostCleanseStrengthReduction {
        remaining_sources: u16,
    },
    PreserveExecuteSurvivalResources {
        remaining_sources: u16,
    },
    ManageLiveStatusBurden {
        live_status_cards: u16,
    },
    ProvePhaseTransitionSurvival,
    DeployHeldSetupInUntaxedWindow {
        undeployed_power_cards: u16,
    },
    ExposeAttackMitigationTarget {
        protected_attackers: u8,
    },
    RecoverWithheldSurvivalResource {
        remaining_sources: u16,
    },
    PrepareForcedAttackSurvival {
        player_turns_remaining: u8,
    },
    SurviveForcedAttackWindow,
    ExploitForcedAttackRecoveryWindow,
    EliminateTeamGrowthSource {
        remaining_hp_with_block: i32,
    },
    EstablishDurableScaling,
    SurviveSecondPhaseOpening,
    SurviveExecuteWindow,
    ConvertPreparedEngineToLethal,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatPlanResourcesV1 {
    pub undeployed_power_cards: u16,
    pub remaining_skill_fuel: u16,
    pub finite_skill_conversion: FiniteSkillConversionStateV1,
    /// Persistent Strength gained at later turn starts.
    ///
    /// This is an exact realized resource, not the value of a power card in
    /// hand.  Awakened One's reactive Strength tax must therefore be compared
    /// with the durable scaling which has actually been established.
    pub durable_strength_growth: i32,
    pub exhaust_draw_active: bool,
    pub exhaust_block_active: bool,
    pub status_draw_active: bool,
    /// Live cards which can still emit at least one exhaust event. This is
    /// exact fuel availability, not an estimate of how valuable the event is.
    #[serde(default)]
    pub remaining_exhaust_sources: u16,
    /// Live, unexhausted cards which can reduce enemy Strength.
    #[serde(default)]
    pub remaining_strength_reduction: u16,
    /// Live cards whose Weak or enemy-Strength-down payoff is blocked by
    /// Artifact. This is exact supply, not permission to spend one as an
    /// Artifact strip.
    #[serde(default)]
    pub remaining_artifact_sensitive_mitigation: u16,
    /// Live Apparition cards which can cover a later forced attack window.
    #[serde(default)]
    pub remaining_intangible_sources: u16,
    /// Block-producing cards in hand, draw, or discard. Cards held in a
    /// long-lived mechanic-owned limbo such as Stasis are not accessible.
    #[serde(default)]
    pub remaining_accessible_block_sources: u16,
    /// Block-producing cards already in the current hand. Unlike a source in
    /// draw or discard, these can answer the currently visible forced attack
    /// without assuming another draw or reshuffle.
    #[serde(default)]
    pub remaining_hand_block_sources: u16,
    /// Block-producing cards currently withheld by a recoverable combat
    /// mechanic. This is distinct from present access and carries no value
    /// estimate for the card or the action which would recover it.
    #[serde(default)]
    pub recoverable_withheld_block_sources: u16,
    /// Living enemies whose Artifact has already been removed.
    #[serde(default)]
    pub exposed_enemy_count: u8,
}

/// Exact, directly observed facts relevant to the current plan stage.
///
/// This is not a value function. In particular, lower first-phase HP can trade
/// against status burden, energy, or visible damage margin; callers must not
/// collapse this structure into an authoritative scalar.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatPlanStateEnvelopeV1 {
    pub player_hp: i32,
    pub player_block: i32,
    /// Exact turns of player Intangible already realized in combat state.
    #[serde(default)]
    pub player_intangible_turns: i32,
    pub visible_incoming_damage: i32,
    pub visible_damage_margin: i32,
    pub current_energy: u8,
    pub first_phase_hp_with_block: Option<i32>,
    pub awakened_strength: i32,
    pub live_status_cards: u16,
    /// HP plus Block on the encounter plan's current focus target.
    #[serde(default)]
    pub priority_target_hp_with_block: Option<i32>,
    /// Exact Artifact stacks on the encounter plan's current focus target.
    #[serde(default)]
    pub priority_target_artifact: Option<i32>,
    /// Exact damage required to cross the encounter's next phase threshold.
    #[serde(default)]
    pub phase_transition_damage_remaining: Option<i32>,
    /// Sum of realized Strength across living enemies.
    #[serde(default)]
    pub enemy_team_strength: i32,
    /// Deterministic player-decision boundaries remaining before a forced
    /// attack window. Zero means the attack is the currently visible intent.
    #[serde(default)]
    pub forced_attack_turns_remaining: Option<u8>,
    /// Whether the current boundary follows the forced attack and can be used
    /// for recovery or conversion without that attack landing again now.
    #[serde(default)]
    pub forced_attack_recovery_window: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CombatPlanTransitionEventV1 {
    FirstPhaseHpWithBlockChanged {
        before: Option<i32>,
        after: Option<i32>,
    },
    VisibleDamageMarginChanged {
        before: i32,
        after: i32,
    },
    PlayerIntangibleChanged {
        before: i32,
        after: i32,
    },
    PhaseTransitionDamageRemainingChanged {
        before: Option<i32>,
        after: Option<i32>,
    },
    ReactiveStrengthChanged {
        before: i32,
        after: i32,
    },
    LiveStatusBurdenChanged {
        before: u16,
        after: u16,
    },
    FiniteSkillConversionChanged {
        before: FiniteSkillConversionStateV1,
        after: FiniteSkillConversionStateV1,
    },
    ExhaustDrawChanged {
        before: bool,
        after: bool,
    },
    ExhaustBlockChanged {
        before: bool,
        after: bool,
    },
    StatusDrawChanged {
        before: bool,
        after: bool,
    },
    StrengthReductionSupplyChanged {
        before: u16,
        after: u16,
    },
    ArtifactSensitiveMitigationSupplyChanged {
        before: u16,
        after: u16,
    },
    PhaseSurvivalSupplyChanged {
        before: u16,
        after: u16,
    },
    ExposedEnemyCountChanged {
        before: u8,
        after: u8,
    },
    PriorityTargetHpWithBlockChanged {
        before: Option<i32>,
        after: Option<i32>,
    },
    PriorityTargetArtifactChanged {
        before: Option<i32>,
        after: Option<i32>,
    },
    EnemyTeamStrengthChanged {
        before: i32,
        after: i32,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatPlanProjectionV1 {
    pub schema: String,
    pub plan: CombatPlanIdV1,
    pub stage: CombatPlanStageV1,
    pub next_milestone: CombatPlanMilestoneV1,
    pub primary: CombatPlanObligationV1,
    pub supporting: Vec<CombatPlanObligationV1>,
    pub resources: CombatPlanResourcesV1,
    pub envelope: CombatPlanStateEnvelopeV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CombatPlanTransitionV1 {
    pub schema: String,
    pub plan: CombatPlanIdV1,
    pub before_stage: CombatPlanStageV1,
    pub after_stage: Option<CombatPlanStageV1>,
    pub completed_milestones: Vec<CombatPlanMilestoneV1>,
    pub events: Vec<CombatPlanTransitionEventV1>,
    pub resources_before: CombatPlanResourcesV1,
    pub resources_after: Option<CombatPlanResourcesV1>,
    pub envelope_before: CombatPlanStateEnvelopeV1,
    pub envelope_after: Option<CombatPlanStateEnvelopeV1>,
}

/// Encounter-owned transition evidence carried by a generic planner edge.
///
/// Adding another combat plan extends this enum in the strategy crate rather
/// than teaching the planner encounter-specific fields.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "plan", content = "transition")]
pub enum CombatPlanTransitionAnnotationV1 {
    AwakenedOnePhaseControl(CombatPlanTransitionV1),
    BronzeAutomatonControl(CombatPlanTransitionV1),
    ChampPhaseControl(CombatPlanTransitionV1),
    DonuAndDecaGrowthControl(CombatPlanTransitionV1),
}

impl CombatPlanTransitionAnnotationV1 {
    pub fn completed_milestones(&self) -> &[CombatPlanMilestoneV1] {
        match self {
            Self::AwakenedOnePhaseControl(transition)
            | Self::BronzeAutomatonControl(transition)
            | Self::ChampPhaseControl(transition)
            | Self::DonuAndDecaGrowthControl(transition) => &transition.completed_milestones,
        }
    }
}

/// Dispatches an exact transition to the combat plan that owns the encounter.
///
/// `None` is the ordinary result for encounters without a typed plan. This
/// function observes already-simulated states and cannot create or rank a
/// successor.
pub fn combat_plan_transition_annotation_v1(
    before: &CombatPosition,
    after: &CombatPosition,
) -> Option<CombatPlanTransitionAnnotationV1> {
    if awakened_one_combat_plan_v1(before).is_some() {
        awakened_one_plan_transition_v1(before, after)
            .map(CombatPlanTransitionAnnotationV1::AwakenedOnePhaseControl)
    } else if bronze_automaton_combat_plan_v1(before).is_some() {
        bronze_automaton_plan_transition_v1(before, after)
            .map(CombatPlanTransitionAnnotationV1::BronzeAutomatonControl)
    } else if champ_combat_plan_v1(before).is_some() {
        champ_plan_transition_v1(before, after)
            .map(CombatPlanTransitionAnnotationV1::ChampPhaseControl)
    } else {
        donu_and_deca_plan_transition_v1(before, after)
            .map(CombatPlanTransitionAnnotationV1::DonuAndDecaGrowthControl)
    }
}

/// Dispatches an exact state to the combat plan that owns the encounter.
///
/// The projection is read-only and does not imply that a plan stage is
/// reachable, desirable, or solved.
pub fn combat_plan_projection_v1(position: &CombatPosition) -> Option<CombatPlanProjectionV1> {
    awakened_one_combat_plan_v1(position)
        .or_else(|| bronze_automaton_combat_plan_v1(position))
        .or_else(|| champ_combat_plan_v1(position))
        .or_else(|| donu_and_deca_combat_plan_v1(position))
}

/// Returns the encounter-owned milestone which is ready for a bounded
/// complete-turn closure probe at this exact state.
///
/// This is deliberately conservative. It does not claim that the milestone
/// is reachable or rank any action; it only identifies a stage where one
/// complete player turn can authoritatively prove completion. Search and
/// terminal truth remain outside the strategy owner.
pub fn combat_plan_turn_closure_target_v1(
    position: &CombatPosition,
) -> Option<CombatPlanMilestoneV1> {
    let plan = combat_plan_projection_v1(position)?;
    match plan.stage {
        CombatPlanStageV1::SurviveForcedAttackWindow
            if plan.envelope.forced_attack_turns_remaining == Some(0) =>
        {
            Some(plan.next_milestone)
        }
        _ => None,
    }
}

/// Returns the encounter plan's independent state-guidance view.
///
/// This rank does not replace survival or progress guidance and cannot prove
/// a state viable. It exists so a generic scheduler periodically services
/// states which preserve the resources required by the current typed plan.
pub fn combat_plan_state_guide_rank_v1(
    position: &CombatPosition,
) -> Option<CombatPlanStateGuideRankV1> {
    let plan = combat_plan_projection_v1(position)?;
    if plan.plan == CombatPlanIdV1::BronzeAutomatonControl {
        return bronze_automaton::bronze_automaton_state_guide_rank_v1(position, &plan);
    }
    if plan.plan == CombatPlanIdV1::ChampPhaseControl {
        return None;
    }
    let durable_scaling_readiness = durable_scaling_readiness(position, &plan.resources);
    let components = match plan.stage {
        CombatPlanStageV1::RemoveEscalatingAdds => {
            let remaining_adds = match plan.primary {
                CombatPlanObligationV1::EliminateEscalatingAdds { remaining } => remaining,
                _ => 0,
            };
            vec![
                -(remaining_adds as i32),
                durable_scaling_readiness,
                reserved_conversion_rank(plan.resources.finite_skill_conversion),
                plan.resources.durable_strength_growth,
                -plan.envelope.awakened_strength,
                plan.resources.remaining_skill_fuel as i32,
                plan.envelope.visible_damage_margin,
                plan.envelope
                    .player_hp
                    .saturating_add(plan.envelope.player_block),
                -(plan.envelope.live_status_cards as i32),
            ]
        }
        CombatPlanStageV1::PrepareFirstPhaseCommit => vec![
            i32::from(plan.envelope.visible_damage_margin >= 0),
            durable_scaling_readiness,
            reserved_conversion_rank(plan.resources.finite_skill_conversion),
            plan.resources.durable_strength_growth,
            -plan.envelope.first_phase_hp_with_block.unwrap_or_default(),
            plan.envelope.visible_damage_margin,
            plan.resources.remaining_skill_fuel as i32,
            -plan.envelope.awakened_strength,
            plan.envelope
                .player_hp
                .saturating_add(plan.envelope.player_block),
            -(plan.envelope.live_status_cards as i32),
        ],
        CombatPlanStageV1::ExploitTransitionWindow => vec![
            durable_scaling_readiness,
            deployed_conversion_rank(plan.resources.finite_skill_conversion),
            plan.resources.status_draw_active as i32,
            plan.resources.exhaust_draw_active as i32,
            plan.resources.exhaust_block_active as i32,
            plan.resources.durable_strength_growth,
            -(plan.resources.undeployed_power_cards as i32),
            plan.resources.remaining_skill_fuel as i32,
            plan.envelope
                .player_hp
                .saturating_add(plan.envelope.player_block),
            -(plan.envelope.live_status_cards as i32),
        ],
        CombatPlanStageV1::SurviveSecondPhaseOpening => vec![
            plan.envelope.visible_damage_margin,
            plan.envelope
                .player_hp
                .saturating_add(plan.envelope.player_block),
            durable_scaling_readiness,
            deployed_conversion_rank(plan.resources.finite_skill_conversion),
            plan.resources.exhaust_draw_active as i32,
            plan.resources.exhaust_block_active as i32,
            plan.resources.status_draw_active as i32,
            plan.resources.durable_strength_growth,
            plan.resources.remaining_skill_fuel as i32,
            -(plan.envelope.live_status_cards as i32),
        ],
        CombatPlanStageV1::PrepareThresholdCommit
        | CombatPlanStageV1::AwaitDebuffCleanse
        | CombatPlanStageV1::SurviveExecuteWindow => {
            unreachable!("Champ plans are diagnostic-only and do not own production guidance")
        }
        CombatPlanStageV1::ExposeAttackMitigationTarget
        | CombatPlanStageV1::RecoverWithheldSurvivalResource
        | CombatPlanStageV1::PrepareForcedAttackWindow
        | CombatPlanStageV1::SurviveForcedAttackWindow
        | CombatPlanStageV1::ExploitForcedAttackRecoveryWindow => {
            unreachable!("Bronze Automaton guidance is owned by its encounter module")
        }
        CombatPlanStageV1::EliminateTeamGrowthSource => {
            let mitigation_ready = plan.resources.remaining_strength_reduction == 0
                || plan.resources.exposed_enemy_count > 0;
            vec![
                i32::from(plan.envelope.visible_damage_margin >= 0),
                i32::from(mitigation_ready),
                durable_scaling_readiness,
                plan.resources.durable_strength_growth,
                -plan
                    .envelope
                    .priority_target_hp_with_block
                    .unwrap_or_default(),
                -plan.envelope.enemy_team_strength,
                plan.envelope.visible_damage_margin,
                plan.envelope
                    .player_hp
                    .saturating_add(plan.envelope.player_block),
                plan.resources.remaining_skill_fuel as i32,
                -(plan.envelope.live_status_cards as i32),
            ]
        }
        CombatPlanStageV1::ConvertToLethal => vec![
            plan.envelope.visible_damage_margin,
            plan.envelope
                .player_hp
                .saturating_add(plan.envelope.player_block),
            durable_scaling_readiness,
            deployed_conversion_rank(plan.resources.finite_skill_conversion),
            plan.resources.exhaust_draw_active as i32,
            plan.resources.exhaust_block_active as i32,
            plan.resources.status_draw_active as i32,
            plan.resources.durable_strength_growth,
            plan.resources.remaining_skill_fuel as i32,
            -(plan.envelope.live_status_cards as i32),
        ],
    };
    Some(CombatPlanStateGuideRankV1 { components })
}

/// Projects the exact state into an Awakened One phase-control plan.
///
/// `None` means that this projector does not own the encounter. The returned
/// plan contains no action score and cannot establish a witness.
pub fn awakened_one_combat_plan_v1(position: &CombatPosition) -> Option<CombatPlanProjectionV1> {
    if combat_terminal(&position.engine, &position.combat) != CombatTerminal::Unresolved {
        return None;
    }
    let combat = &position.combat;
    let awakened = awakened_one(combat)?;
    let living_cultists = combat
        .entities
        .monsters
        .iter()
        .filter(|monster| enemy_id(monster) == EnemyId::Cultist && monster.is_alive_for_action())
        .count()
        .min(u8::MAX as usize) as u8;
    let resources = combat_plan_resources_v1(combat);
    let mut envelope = combat_plan_state_envelope_v1(combat);
    envelope.first_phase_hp_with_block = awakened.awakened_one.form1.then_some(
        awakened
            .current_hp
            .max(0)
            .saturating_add(awakened.block.max(0)),
    );
    envelope.awakened_strength = combat.get_power(awakened.id, PowerId::Strength);
    let stage = awakened_one_stage(awakened, living_cultists);
    let (next_milestone, primary) = match stage {
        CombatPlanStageV1::RemoveEscalatingAdds => (
            CombatPlanMilestoneV1::EscalatingAddsRemoved,
            CombatPlanObligationV1::EliminateEscalatingAdds {
                remaining: living_cultists,
            },
        ),
        CombatPlanStageV1::PrepareFirstPhaseCommit => (
            CombatPlanMilestoneV1::UntaxedTransitionWindowReached,
            CombatPlanObligationV1::ProvePhaseTransitionSurvival,
        ),
        CombatPlanStageV1::ExploitTransitionWindow => (
            CombatPlanMilestoneV1::TransitionWindowClosed,
            CombatPlanObligationV1::DeployHeldSetupInUntaxedWindow {
                undeployed_power_cards: resources.undeployed_power_cards,
            },
        ),
        CombatPlanStageV1::SurviveSecondPhaseOpening => (
            CombatPlanMilestoneV1::SecondPhaseOpeningSurvived,
            CombatPlanObligationV1::SurviveSecondPhaseOpening,
        ),
        CombatPlanStageV1::ConvertToLethal => (
            CombatPlanMilestoneV1::EncounterDefeated,
            CombatPlanObligationV1::ConvertPreparedEngineToLethal,
        ),
        CombatPlanStageV1::EliminateTeamGrowthSource => {
            unreachable!("Awakened One plan cannot enter Donu's growth-control stage")
        }
        CombatPlanStageV1::ExposeAttackMitigationTarget
        | CombatPlanStageV1::RecoverWithheldSurvivalResource
        | CombatPlanStageV1::PrepareForcedAttackWindow
        | CombatPlanStageV1::SurviveForcedAttackWindow
        | CombatPlanStageV1::ExploitForcedAttackRecoveryWindow => {
            unreachable!("Awakened One plan cannot enter Bronze Automaton's setup stage")
        }
        CombatPlanStageV1::PrepareThresholdCommit
        | CombatPlanStageV1::AwaitDebuffCleanse
        | CombatPlanStageV1::SurviveExecuteWindow => {
            unreachable!("Awakened One plan cannot enter Champ's threshold-control stages")
        }
    };

    let mut supporting = Vec::new();
    if awakened.awakened_one.form1 {
        let tax = combat.get_power(awakened.id, PowerId::Curiosity);
        if tax > 0 {
            supporting.push(CombatPlanObligationV1::AccountForReactivePowerTax {
                strength_per_power: tax,
            });
        }
    }
    if resources.remaining_skill_fuel > 0
        && resources.finite_skill_conversion != FiniteSkillConversionStateV1::Unavailable
    {
        supporting.push(CombatPlanObligationV1::PreserveFiniteSkillFuel {
            remaining_skills: resources.remaining_skill_fuel,
            conversion: resources.finite_skill_conversion,
        });
    }
    if envelope.live_status_cards > 0 && !resources.status_draw_active {
        supporting.push(CombatPlanObligationV1::ManageLiveStatusBurden {
            live_status_cards: envelope.live_status_cards,
        });
    }
    if stage == CombatPlanStageV1::RemoveEscalatingAdds {
        supporting.push(CombatPlanObligationV1::ProvePhaseTransitionSurvival);
    }

    Some(CombatPlanProjectionV1 {
        schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
        plan: CombatPlanIdV1::AwakenedOnePhaseControl,
        stage,
        next_milestone,
        primary,
        supporting,
        resources,
        envelope,
    })
}

/// Projects Donu and Deca into a growth-source control plan.
///
/// Donu owns the repeating team-wide Strength clock.  The projection therefore
/// keeps exact survival independent while exposing three encounter facts to a
/// dedicated guide lane: whether a live Strength-reduction card has an
/// Artifact-free target, whether durable player scaling has been established,
/// and how much HP remains on Donu.  It does not force a target or prune Deca
/// lines.
pub fn donu_and_deca_combat_plan_v1(position: &CombatPosition) -> Option<CombatPlanProjectionV1> {
    if combat_terminal(&position.engine, &position.combat) != CombatTerminal::Unresolved {
        return None;
    }
    let combat = &position.combat;
    let donu = combat
        .entities
        .monsters
        .iter()
        .find(|monster| enemy_id(monster) == EnemyId::Donu && !monster.is_escaped)?;
    let deca = combat
        .entities
        .monsters
        .iter()
        .find(|monster| enemy_id(monster) == EnemyId::Deca && !monster.is_escaped)?;
    if !donu.is_alive_for_action() && !deca.is_alive_for_action() {
        return None;
    }

    let resources = combat_plan_resources_v1(combat);
    let mut envelope = combat_plan_state_envelope_v1(combat);
    envelope.priority_target_hp_with_block = donu
        .is_alive_for_action()
        .then_some(donu.current_hp.max(0).saturating_add(donu.block.max(0)));
    let stage = if donu.is_alive_for_action() {
        CombatPlanStageV1::EliminateTeamGrowthSource
    } else {
        CombatPlanStageV1::ConvertToLethal
    };
    let (next_milestone, primary) = match stage {
        CombatPlanStageV1::EliminateTeamGrowthSource => (
            CombatPlanMilestoneV1::TeamGrowthSourceEliminated,
            CombatPlanObligationV1::EliminateTeamGrowthSource {
                remaining_hp_with_block: envelope.priority_target_hp_with_block.unwrap_or_default(),
            },
        ),
        CombatPlanStageV1::ConvertToLethal => (
            CombatPlanMilestoneV1::EncounterDefeated,
            CombatPlanObligationV1::ConvertPreparedEngineToLethal,
        ),
        _ => unreachable!("Donu and Deca plan uses only its two owned stages"),
    };

    let mut supporting = Vec::new();
    if resources.remaining_strength_reduction > 0 && resources.exposed_enemy_count == 0 {
        let protected_attackers = combat
            .entities
            .monsters
            .iter()
            .filter(|monster| {
                monster.is_alive_for_action()
                    && matches!(enemy_id(monster), EnemyId::Donu | EnemyId::Deca)
                    && combat.get_power(monster.id, PowerId::Artifact) > 0
            })
            .count()
            .min(u8::MAX as usize) as u8;
        supporting.push(CombatPlanObligationV1::ExposeAttackMitigationTarget {
            protected_attackers,
        });
    }
    if resources.durable_strength_growth == 0
        && live_cards(combat).any(|card| card.id == CardId::DemonForm)
    {
        supporting.push(CombatPlanObligationV1::EstablishDurableScaling);
    }
    if envelope.live_status_cards > 0 && !resources.status_draw_active {
        supporting.push(CombatPlanObligationV1::ManageLiveStatusBurden {
            live_status_cards: envelope.live_status_cards,
        });
    }

    Some(CombatPlanProjectionV1 {
        schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
        plan: CombatPlanIdV1::DonuAndDecaGrowthControl,
        stage,
        next_milestone,
        primary,
        supporting,
        resources,
        envelope,
    })
}

/// Reports monotone semantic progress between two exact states.
///
/// This is an observation, not a reward. In particular, an empty milestone
/// list does not mean that the transition was bad.
pub fn awakened_one_plan_transition_v1(
    before: &CombatPosition,
    after: &CombatPosition,
) -> Option<CombatPlanTransitionV1> {
    let before_plan = awakened_one_combat_plan_v1(before)?;
    if combat_terminal(&after.engine, &after.combat) == CombatTerminal::Win {
        return Some(CombatPlanTransitionV1 {
            schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
            plan: before_plan.plan,
            before_stage: before_plan.stage,
            after_stage: None,
            completed_milestones: vec![CombatPlanMilestoneV1::EncounterDefeated],
            events: Vec::new(),
            resources_before: before_plan.resources,
            resources_after: None,
            envelope_before: before_plan.envelope,
            envelope_after: None,
        });
    }

    let after_plan = awakened_one_combat_plan_v1(after);
    let mut completed_milestones = Vec::new();
    if let Some(after_plan) = &after_plan {
        let before_ordinal = stage_ordinal(before_plan.stage);
        let after_ordinal = stage_ordinal(after_plan.stage);
        if before_ordinal < 1 && after_ordinal >= 1 {
            completed_milestones.push(CombatPlanMilestoneV1::EscalatingAddsRemoved);
        }
        if before_ordinal < 2 && after_ordinal >= 2 {
            completed_milestones.push(CombatPlanMilestoneV1::UntaxedTransitionWindowReached);
        }
        if before_ordinal < 3 && after_ordinal >= 3 {
            completed_milestones.push(CombatPlanMilestoneV1::TransitionWindowClosed);
        }
        if before_ordinal < 4 && after_ordinal >= 4 {
            completed_milestones.push(CombatPlanMilestoneV1::SecondPhaseOpeningSurvived);
        }
    }

    let after_stage = after_plan.as_ref().map(|plan| plan.stage);
    let events = after_plan
        .as_ref()
        .map(|after_plan| combat_plan_transition_events_v1(&before_plan, after_plan))
        .unwrap_or_default();
    let resources_after = after_plan.as_ref().map(|plan| plan.resources);
    let envelope_after = after_plan.as_ref().map(|plan| plan.envelope);
    Some(CombatPlanTransitionV1 {
        schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
        plan: before_plan.plan,
        before_stage: before_plan.stage,
        after_stage,
        completed_milestones,
        events,
        resources_before: before_plan.resources,
        resources_after,
        envelope_before: before_plan.envelope,
        envelope_after,
    })
}

pub fn donu_and_deca_plan_transition_v1(
    before: &CombatPosition,
    after: &CombatPosition,
) -> Option<CombatPlanTransitionV1> {
    let before_plan = donu_and_deca_combat_plan_v1(before)?;
    if combat_terminal(&after.engine, &after.combat) == CombatTerminal::Win {
        return Some(CombatPlanTransitionV1 {
            schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
            plan: before_plan.plan,
            before_stage: before_plan.stage,
            after_stage: None,
            completed_milestones: vec![CombatPlanMilestoneV1::EncounterDefeated],
            events: Vec::new(),
            resources_before: before_plan.resources,
            resources_after: None,
            envelope_before: before_plan.envelope,
            envelope_after: None,
        });
    }

    let after_plan = donu_and_deca_combat_plan_v1(after);
    let mut completed_milestones = Vec::new();
    if before_plan.stage == CombatPlanStageV1::EliminateTeamGrowthSource
        && after_plan
            .as_ref()
            .is_some_and(|plan| plan.stage == CombatPlanStageV1::ConvertToLethal)
    {
        completed_milestones.push(CombatPlanMilestoneV1::TeamGrowthSourceEliminated);
    }
    let events = after_plan
        .as_ref()
        .map(|after_plan| combat_plan_transition_events_v1(&before_plan, after_plan))
        .unwrap_or_default();
    Some(CombatPlanTransitionV1 {
        schema: COMBAT_PLAN_SCHEMA_V1.to_owned(),
        plan: before_plan.plan,
        before_stage: before_plan.stage,
        after_stage: after_plan.as_ref().map(|plan| plan.stage),
        completed_milestones,
        events,
        resources_before: before_plan.resources,
        resources_after: after_plan.as_ref().map(|plan| plan.resources),
        envelope_before: before_plan.envelope,
        envelope_after: after_plan.as_ref().map(|plan| plan.envelope),
    })
}

/// Classifies an exact action successor which spends a reserved finite
/// conversion before Awakened One's untaxed transition window.
///
/// The classification is deliberately narrow:
///
/// * the first phase is still live after the action;
/// * the current visible attack is already survivable without deploying the
///   conversion; and
/// * the action changed the conversion from available to active.
///
/// Consequently this does not discourage a lethal phase commit, emergency
/// deployment needed to survive, or deployment during/after the transition.
pub fn combat_plan_action_deferral_v1(
    before: &CombatPosition,
    after: &CombatPosition,
) -> Option<CombatPlanActionDeferralV1> {
    match combat_plan_action_timing_v1(before, after) {
        CombatPlanActionTimingV1::Defer(reason) => Some(reason),
        CombatPlanActionTimingV1::PreferNow | CombatPlanActionTimingV1::Neutral => None,
    }
}

/// Classifies exact resource timing around encounter-owned plan boundaries.
///
/// All alternatives remain legal. This categorical signal is consumed only by
/// explicitly plan-compatible proposal lanes; it is not an ordinary action
/// weight or pruning rule.
pub fn combat_plan_action_timing_v1(
    before_position: &CombatPosition,
    after_position: &CombatPosition,
) -> CombatPlanActionTimingV1 {
    if bronze_automaton_combat_plan_v1(before_position).is_some() {
        return bronze_automaton::bronze_automaton_action_timing_v1(
            before_position,
            after_position,
        );
    }
    let Some(before) = awakened_one_combat_plan_v1(before_position) else {
        return CombatPlanActionTimingV1::Neutral;
    };
    let Some(after) = awakened_one_combat_plan_v1(after_position) else {
        return CombatPlanActionTimingV1::Neutral;
    };
    let activates_conversion = before.resources.finite_skill_conversion
        == FiniteSkillConversionStateV1::Available
        && after.resources.finite_skill_conversion == FiniteSkillConversionStateV1::Active;
    let realizes_held_setup = after.resources.durable_strength_growth
        > before.resources.durable_strength_growth
        || !before.resources.exhaust_draw_active && after.resources.exhaust_draw_active
        || !before.resources.exhaust_block_active && after.resources.exhaust_block_active
        || !before.resources.status_draw_active && after.resources.status_draw_active;
    let destroys_held_setup = live_undeployed_plan_asset_count(before_position, &before.resources)
        > live_undeployed_plan_asset_count(after_position, &after.resources)
        && !realizes_held_setup;
    if (activates_conversion || realizes_held_setup)
        && matches!(
            before.stage,
            CombatPlanStageV1::ExploitTransitionWindow
                | CombatPlanStageV1::SurviveSecondPhaseOpening
        )
    {
        CombatPlanActionTimingV1::PreferNow
    } else if activates_conversion
        && before.stage == CombatPlanStageV1::PrepareFirstPhaseCommit
        && after.stage == CombatPlanStageV1::PrepareFirstPhaseCommit
        && before.envelope.visible_damage_margin >= 0
    {
        CombatPlanActionTimingV1::Defer(
            CombatPlanActionDeferralV1::PreserveFiniteSkillConversionUntilUntaxedWindow,
        )
    } else if destroys_held_setup
        && matches!(
            before.stage,
            CombatPlanStageV1::RemoveEscalatingAdds | CombatPlanStageV1::PrepareFirstPhaseCommit
        )
        && matches!(
            after.stage,
            CombatPlanStageV1::RemoveEscalatingAdds | CombatPlanStageV1::PrepareFirstPhaseCommit
        )
        && before.envelope.visible_damage_margin >= 0
    {
        CombatPlanActionTimingV1::Defer(CombatPlanActionDeferralV1::PreserveUndeployedPlanAsset)
    } else {
        CombatPlanActionTimingV1::Neutral
    }
}

/// Classifies one member of a structured selection before it is executed.
///
/// Forced exhaust choices are a separate semantic boundary from ordinary
/// card plays: the selected card disappears without realizing its effect.
/// The Awakened One plan therefore defers exhausting an undeployed asset that
/// still supplies one of its explicit resources. This remains an ordering
/// preference; if every legal member is deferred, the caller must still pick
/// one and preserve exact legality.
pub fn combat_plan_selection_member_timing_v1(
    position: &CombatPosition,
    family: &CombatSelectionActionFamilyV2,
    member: &ClientInput,
) -> CombatPlanActionTimingV1 {
    if !matches!(
        family.reason,
        CombatSelectionReasonV2::Hand(HandSelectReason::Exhaust)
    ) {
        return CombatPlanActionTimingV1::Neutral;
    }
    let Some(plan) = awakened_one_combat_plan_v1(position) else {
        return CombatPlanActionTimingV1::Neutral;
    };
    let ClientInput::SubmitSelection(resolution) = member else {
        return CombatPlanActionTimingV1::Neutral;
    };
    let selected = resolution.selected_card_lineages();
    let consumes_plan_asset = position
        .combat
        .zones
        .hand
        .iter()
        .filter(|card| selected.contains(&card.lineage))
        .any(|card| undeployed_card_supplies_plan_resource(card, &plan.resources));
    if consumes_plan_asset {
        CombatPlanActionTimingV1::Defer(CombatPlanActionDeferralV1::PreserveUndeployedPlanAsset)
    } else {
        CombatPlanActionTimingV1::Neutral
    }
}

/// Reports whether the current plan has a resource whose exact deployment
/// timing can distinguish `PreferNow` from otherwise neutral legal actions.
///
/// Generic search may use this only to keep scanning an already ranked action
/// surface for a plan-owned timing preference. The encounter plan remains the
/// sole owner of stage and resource semantics.
pub fn combat_plan_has_timed_action_preference_v1(position: &CombatPosition) -> bool {
    combat_plan_supports_initial_policy_prefix_v1(position)
        || awakened_one_combat_plan_v1(position).is_some_and(|plan| {
            let held_setup_available = live_cards(&position.combat)
                .any(|card| undeployed_card_supplies_plan_resource(card, &plan.resources));
            matches!(
                plan.stage,
                CombatPlanStageV1::ExploitTransitionWindow
                    | CombatPlanStageV1::SurviveSecondPhaseOpening
            ) && (plan.resources.finite_skill_conversion == FiniteSkillConversionStateV1::Available
                || held_setup_available)
        })
}

/// Admits only encounter plans with exact-root evidence that one bounded
/// policy prefix improves the production search corridor.
///
/// Timed action preferences alone are insufficient: they remain available to
/// explicit laboratory proposals without silently changing production.
pub fn combat_plan_supports_initial_policy_prefix_v1(position: &CombatPosition) -> bool {
    bronze_automaton_combat_plan_v1(position).is_some_and(|plan| {
        plan.stage == CombatPlanStageV1::ExposeAttackMitigationTarget
            && plan.resources.remaining_artifact_sensitive_mitigation > 0
    })
}

fn undeployed_card_supplies_plan_resource(
    card: &CombatCard,
    resources: &CombatPlanResourcesV1,
) -> bool {
    match card.id {
        CardId::Corruption => {
            resources.finite_skill_conversion == FiniteSkillConversionStateV1::Available
        }
        CardId::DemonForm => resources.durable_strength_growth == 0,
        CardId::DarkEmbrace => !resources.exhaust_draw_active,
        CardId::FeelNoPain => !resources.exhaust_block_active,
        CardId::Evolve => !resources.status_draw_active,
        _ => false,
    }
}

fn live_undeployed_plan_asset_count(
    position: &CombatPosition,
    resources: &CombatPlanResourcesV1,
) -> usize {
    live_cards(&position.combat)
        .filter(|card| undeployed_card_supplies_plan_resource(card, resources))
        .count()
}

fn durable_scaling_readiness(position: &CombatPosition, resources: &CombatPlanResourcesV1) -> i32 {
    if resources.durable_strength_growth > 0 {
        2
    } else if live_cards(&position.combat).any(|card| card.id == CardId::DemonForm) {
        1
    } else {
        0
    }
}

fn awakened_one(combat: &CombatState) -> Option<&MonsterEntity> {
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| enemy_id(monster) == EnemyId::AwakenedOne && !monster.is_escaped)
}

fn awakened_one_stage(awakened: &MonsterEntity, living_cultists: u8) -> CombatPlanStageV1 {
    if awakened.awakened_one.form1 {
        if living_cultists > 0 {
            CombatPlanStageV1::RemoveEscalatingAdds
        } else {
            CombatPlanStageV1::PrepareFirstPhaseCommit
        }
    } else if awakened.half_dead || awakened.current_hp <= 0 {
        CombatPlanStageV1::ExploitTransitionWindow
    } else if awakened.awakened_one.first_turn {
        CombatPlanStageV1::SurviveSecondPhaseOpening
    } else {
        CombatPlanStageV1::ConvertToLethal
    }
}

const fn stage_ordinal(stage: CombatPlanStageV1) -> u8 {
    match stage {
        CombatPlanStageV1::RemoveEscalatingAdds => 0,
        CombatPlanStageV1::ExposeAttackMitigationTarget => 0,
        CombatPlanStageV1::RecoverWithheldSurvivalResource => 1,
        CombatPlanStageV1::PrepareForcedAttackWindow => 2,
        CombatPlanStageV1::SurviveForcedAttackWindow => 3,
        CombatPlanStageV1::ExploitForcedAttackRecoveryWindow => 4,
        CombatPlanStageV1::PrepareFirstPhaseCommit => 1,
        CombatPlanStageV1::ExploitTransitionWindow => 2,
        CombatPlanStageV1::SurviveSecondPhaseOpening => 3,
        CombatPlanStageV1::PrepareThresholdCommit => 0,
        CombatPlanStageV1::AwaitDebuffCleanse => 1,
        CombatPlanStageV1::SurviveExecuteWindow => 2,
        CombatPlanStageV1::EliminateTeamGrowthSource => 0,
        CombatPlanStageV1::ConvertToLethal => 4,
    }
}

fn combat_plan_resources_v1(combat: &CombatState) -> CombatPlanResourcesV1 {
    let player = combat.entities.player.id;
    let corruption_active =
        sts_core::content::powers::store::has_power(combat, player, PowerId::Corruption);
    let finite_skill_conversion =
        if sts_core::content::powers::store::has_power(combat, player, PowerId::Corruption) {
            FiniteSkillConversionStateV1::Active
        } else if live_cards(combat).any(|card| card.id == CardId::Corruption) {
            FiniteSkillConversionStateV1::Available
        } else {
            FiniteSkillConversionStateV1::Unavailable
        };
    CombatPlanResourcesV1 {
        undeployed_power_cards: count_live_cards(combat, |card| {
            get_card_definition(card.id).card_type == CardType::Power
        }),
        remaining_skill_fuel: count_live_cards(combat, |card| {
            get_card_definition(card.id).card_type == CardType::Skill
        }),
        finite_skill_conversion,
        durable_strength_growth: sts_core::content::powers::store::power_amount(
            combat,
            player,
            PowerId::DemonForm,
        )
        .max(0),
        exhaust_draw_active: sts_core::content::powers::store::has_power(
            combat,
            player,
            PowerId::DarkEmbrace,
        ),
        exhaust_block_active: sts_core::content::powers::store::has_power(
            combat,
            player,
            PowerId::FeelNoPain,
        ),
        status_draw_active: sts_core::content::powers::store::has_power(
            combat,
            player,
            PowerId::Evolve,
        ),
        remaining_exhaust_sources: count_live_cards(combat, |card| {
            corruption_active && get_card_definition(card.id).card_type == CardType::Skill
                || card_can_emit_exhaust_event(card)
        }),
        remaining_strength_reduction: count_live_cards(combat, |card| {
            matches!(card.id, CardId::Disarm | CardId::DarkShackles)
        }),
        remaining_artifact_sensitive_mitigation: 0,
        remaining_intangible_sources: count_live_cards(combat, |card| {
            card.id == CardId::Apparition
        }),
        remaining_accessible_block_sources: count_accessible_cards(combat, card_supplies_block),
        remaining_hand_block_sources: combat
            .zones
            .hand
            .iter()
            .filter(|card| card_supplies_block(card))
            .count()
            .min(u16::MAX as usize) as u16,
        recoverable_withheld_block_sources: 0,
        exposed_enemy_count: combat
            .entities
            .monsters
            .iter()
            .filter(|monster| {
                monster.is_alive_for_action()
                    && combat.get_power(monster.id, PowerId::Artifact) <= 0
            })
            .count()
            .min(u8::MAX as usize) as u8,
    }
}

fn card_can_emit_exhaust_event(card: &CombatCard) -> bool {
    if exhausts_when_played(card) {
        return true;
    }
    card_definition_with_upgrades(card.id, card.upgrades)
        .play_effects
        .iter()
        .any(|effect| {
            matches!(
                effect,
                PlayEffect::EmitEvent(CombatEvent::CardExhausted)
                    | PlayEffect::PlayTopCardAndExhaust
            )
        })
}

fn combat_plan_state_envelope_v1(combat: &CombatState) -> CombatPlanStateEnvelopeV1 {
    let player = combat.entities.player.id;
    let player_intangible_turns = combat.get_power(player, PowerId::IntangiblePlayer).max(0);
    let player_intangible = player_intangible_turns > 0;
    let visible_incoming_damage = combat
        .entities
        .monsters
        .iter()
        .filter(|monster| monster.is_alive_for_action())
        .map(|monster| {
            let preview = project_monster_move_preview_in_combat(combat, monster);
            if player_intangible && preview.total_damage.is_some() {
                i32::from(preview.hits)
            } else {
                preview.total_damage.unwrap_or(0)
            }
        })
        .sum();
    CombatPlanStateEnvelopeV1 {
        player_hp: combat.entities.player.current_hp,
        player_block: combat.entities.player.block,
        player_intangible_turns,
        visible_incoming_damage,
        visible_damage_margin: combat
            .entities
            .player
            .current_hp
            .saturating_add(combat.entities.player.block)
            .saturating_sub(visible_incoming_damage),
        current_energy: combat.turn.energy,
        first_phase_hp_with_block: None,
        awakened_strength: 0,
        live_status_cards: count_live_cards(combat, |card| {
            get_card_definition(card.id).card_type == CardType::Status
        }),
        priority_target_hp_with_block: None,
        priority_target_artifact: None,
        phase_transition_damage_remaining: None,
        enemy_team_strength: combat
            .entities
            .monsters
            .iter()
            .filter(|monster| monster.is_alive_for_action())
            .map(|monster| combat.get_power(monster.id, PowerId::Strength).max(0))
            .sum(),
        forced_attack_turns_remaining: None,
        forced_attack_recovery_window: false,
    }
}

fn combat_plan_transition_events_v1(
    before: &CombatPlanProjectionV1,
    after: &CombatPlanProjectionV1,
) -> Vec<CombatPlanTransitionEventV1> {
    let mut events = Vec::new();
    if before.envelope.first_phase_hp_with_block != after.envelope.first_phase_hp_with_block {
        events.push(CombatPlanTransitionEventV1::FirstPhaseHpWithBlockChanged {
            before: before.envelope.first_phase_hp_with_block,
            after: after.envelope.first_phase_hp_with_block,
        });
    }
    if before.envelope.visible_damage_margin != after.envelope.visible_damage_margin {
        events.push(CombatPlanTransitionEventV1::VisibleDamageMarginChanged {
            before: before.envelope.visible_damage_margin,
            after: after.envelope.visible_damage_margin,
        });
    }
    if before.envelope.player_intangible_turns != after.envelope.player_intangible_turns {
        events.push(CombatPlanTransitionEventV1::PlayerIntangibleChanged {
            before: before.envelope.player_intangible_turns,
            after: after.envelope.player_intangible_turns,
        });
    }
    if before.envelope.phase_transition_damage_remaining
        != after.envelope.phase_transition_damage_remaining
    {
        events.push(
            CombatPlanTransitionEventV1::PhaseTransitionDamageRemainingChanged {
                before: before.envelope.phase_transition_damage_remaining,
                after: after.envelope.phase_transition_damage_remaining,
            },
        );
    }
    if before.envelope.awakened_strength != after.envelope.awakened_strength {
        events.push(CombatPlanTransitionEventV1::ReactiveStrengthChanged {
            before: before.envelope.awakened_strength,
            after: after.envelope.awakened_strength,
        });
    }
    if before.envelope.live_status_cards != after.envelope.live_status_cards {
        events.push(CombatPlanTransitionEventV1::LiveStatusBurdenChanged {
            before: before.envelope.live_status_cards,
            after: after.envelope.live_status_cards,
        });
    }
    if before.resources.finite_skill_conversion != after.resources.finite_skill_conversion {
        events.push(CombatPlanTransitionEventV1::FiniteSkillConversionChanged {
            before: before.resources.finite_skill_conversion,
            after: after.resources.finite_skill_conversion,
        });
    }
    if before.resources.exhaust_draw_active != after.resources.exhaust_draw_active {
        events.push(CombatPlanTransitionEventV1::ExhaustDrawChanged {
            before: before.resources.exhaust_draw_active,
            after: after.resources.exhaust_draw_active,
        });
    }
    if before.resources.exhaust_block_active != after.resources.exhaust_block_active {
        events.push(CombatPlanTransitionEventV1::ExhaustBlockChanged {
            before: before.resources.exhaust_block_active,
            after: after.resources.exhaust_block_active,
        });
    }
    if before.resources.status_draw_active != after.resources.status_draw_active {
        events.push(CombatPlanTransitionEventV1::StatusDrawChanged {
            before: before.resources.status_draw_active,
            after: after.resources.status_draw_active,
        });
    }
    if before.resources.remaining_strength_reduction != after.resources.remaining_strength_reduction
    {
        events.push(
            CombatPlanTransitionEventV1::StrengthReductionSupplyChanged {
                before: before.resources.remaining_strength_reduction,
                after: after.resources.remaining_strength_reduction,
            },
        );
    }
    if before.resources.remaining_artifact_sensitive_mitigation
        != after.resources.remaining_artifact_sensitive_mitigation
    {
        events.push(
            CombatPlanTransitionEventV1::ArtifactSensitiveMitigationSupplyChanged {
                before: before.resources.remaining_artifact_sensitive_mitigation,
                after: after.resources.remaining_artifact_sensitive_mitigation,
            },
        );
    }
    if before.resources.remaining_intangible_sources != after.resources.remaining_intangible_sources
    {
        events.push(CombatPlanTransitionEventV1::PhaseSurvivalSupplyChanged {
            before: before.resources.remaining_intangible_sources,
            after: after.resources.remaining_intangible_sources,
        });
    }
    if before.resources.exposed_enemy_count != after.resources.exposed_enemy_count {
        events.push(CombatPlanTransitionEventV1::ExposedEnemyCountChanged {
            before: before.resources.exposed_enemy_count,
            after: after.resources.exposed_enemy_count,
        });
    }
    if before.envelope.priority_target_hp_with_block != after.envelope.priority_target_hp_with_block
    {
        events.push(
            CombatPlanTransitionEventV1::PriorityTargetHpWithBlockChanged {
                before: before.envelope.priority_target_hp_with_block,
                after: after.envelope.priority_target_hp_with_block,
            },
        );
    }
    if before.envelope.priority_target_artifact != after.envelope.priority_target_artifact {
        events.push(CombatPlanTransitionEventV1::PriorityTargetArtifactChanged {
            before: before.envelope.priority_target_artifact,
            after: after.envelope.priority_target_artifact,
        });
    }
    if before.envelope.enemy_team_strength != after.envelope.enemy_team_strength {
        events.push(CombatPlanTransitionEventV1::EnemyTeamStrengthChanged {
            before: before.envelope.enemy_team_strength,
            after: after.envelope.enemy_team_strength,
        });
    }
    events
}

fn accessible_cards(combat: &CombatState) -> impl Iterator<Item = &CombatCard> {
    combat
        .zones
        .hand
        .iter()
        .chain(combat.zones.draw_pile.iter())
        .chain(combat.zones.discard_pile.iter())
}

fn live_cards(combat: &CombatState) -> impl Iterator<Item = &CombatCard> {
    accessible_cards(combat)
}

fn card_supplies_block(card: &CombatCard) -> bool {
    card_definition_with_upgrades(card.id, card.upgrades)
        .play_effects
        .contains(&PlayEffect::Provide(Mechanic::Block))
}

fn count_accessible_cards(combat: &CombatState, predicate: impl Fn(&CombatCard) -> bool) -> u16 {
    accessible_cards(combat)
        .filter(|card| predicate(card))
        .count()
        .min(u16::MAX as usize) as u16
}

fn count_live_cards(combat: &CombatState, predicate: impl Fn(&CombatCard) -> bool) -> u16 {
    live_cards(combat)
        .filter(|card| predicate(card))
        .count()
        .min(u16::MAX as usize) as u16
}

fn enemy_id(monster: &MonsterEntity) -> EnemyId {
    monster.enemy_id
}

fn reserved_conversion_rank(conversion: FiniteSkillConversionStateV1) -> i32 {
    match conversion {
        FiniteSkillConversionStateV1::Available => 2,
        FiniteSkillConversionStateV1::Unavailable => 1,
        FiniteSkillConversionStateV1::Active => 0,
    }
}

fn deployed_conversion_rank(conversion: FiniteSkillConversionStateV1) -> i32 {
    match conversion {
        FiniteSkillConversionStateV1::Active => 2,
        FiniteSkillConversionStateV1::Unavailable => 1,
        FiniteSkillConversionStateV1::Available => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::powers::store;
    use sts_core::runtime::combat::{Power, PowerPayload};
    use sts_core::state::core::{EngineState, PendingChoice};
    use sts_core::state::selection::{SelectionResolution, SelectionScope};
    use sts_core::test_support::{blank_test_combat, planned_monster, test_monster};

    fn power(power_type: PowerId, amount: i32) -> Power {
        Power {
            power_type,
            instance_id: None,
            amount,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    fn awakened_position(cultists: usize) -> CombatPosition {
        let mut combat = blank_test_combat();
        let mut awakened = test_monster(EnemyId::AwakenedOne);
        awakened.id = 10;
        awakened.slot = 2;
        combat.entities.monsters.push(awakened);
        for index in 0..cultists {
            let mut cultist = test_monster(EnemyId::Cultist);
            cultist.id = 20 + index;
            cultist.slot = index as u8;
            combat.entities.monsters.push(cultist);
        }
        store::set_powers_for(&mut combat, 10, vec![power(PowerId::Curiosity, 1)]);
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    fn double_thief_bridge_position() -> CombatPosition {
        let mut combat = blank_test_combat();
        combat.turn.energy = 3;
        let mut looter = planned_monster(EnemyId::Looter, 1);
        looter.id = 10;
        looter.current_hp = 43;
        looter.max_hp = 47;
        let mut mugger = planned_monster(EnemyId::Mugger, 1);
        mugger.id = 20;
        mugger.current_hp = 14;
        mugger.max_hp = 48;
        combat.entities.monsters = vec![looter, mugger];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 3),
            CombatCard::for_scenario(CardId::DarkEmbrace, 10002),
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 6),
            CombatCard::for_scenario(CardId::PowerThrough, 10000),
        ];
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    fn double_thief_exhaust_block_position() -> CombatPosition {
        let mut combat = blank_test_combat();
        combat.turn.energy = 3;
        let mut looter = planned_monster(EnemyId::Looter, 1);
        looter.id = 10;
        looter.current_hp = 34;
        looter.max_hp = 47;
        looter.thief.stolen_gold = 30;
        let mut mugger = planned_monster(EnemyId::Mugger, 1);
        mugger.id = 20;
        mugger.current_hp = 5;
        mugger.max_hp = 48;
        mugger.thief.stolen_gold = 30;
        combat.entities.monsters = vec![looter, mugger];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Bash, 9),
            CombatCard::for_scenario(CardId::SecondWind, 10001),
            CombatCard::for_scenario(CardId::Defend, 7),
            CombatCard::for_scenario(CardId::Strike, 4),
            CombatCard::for_scenario(CardId::ShrugItOff, 10004),
            CombatCard::for_scenario(CardId::Strike, 1),
        ];
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    fn escaping_single_thief_position(move_id: u8) -> CombatPosition {
        let mut combat = blank_test_combat();
        combat.turn.energy = 3;
        let mut looter = planned_monster(EnemyId::Looter, move_id);
        looter.id = 10;
        looter.current_hp = 22;
        looter.max_hp = 47;
        looter.thief.stolen_gold = 45;
        combat.entities.monsters = vec![looter];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Wound, 20),
            CombatCard::for_scenario(CardId::Strike, 1),
        ];
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    #[test]
    fn double_thief_bridge_proposal_uses_stable_card_and_target_identity() {
        let position = double_thief_bridge_position();

        assert!(!combat_plan_supports_initial_policy_prefix_v1(&position));
        let proposal = combat_plan_turn_prefix_proposal_v1(&position).expect("typed thief prefix");

        assert_eq!(
            proposal.kind,
            CombatPlanPrefixKindV1::SplitThiefPressureAroundDefensiveBridge
        );
        assert_eq!(
            proposal.service_scope,
            CombatPlanPrefixServiceScopeV1::RootEligible
        );
        assert_eq!(
            proposal.steps,
            vec![
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(3),
                    target: Some(10),
                },
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(10000),
                    target: None,
                },
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(1),
                    target: Some(20),
                },
                CombatPlanPrefixStepV1::EndTurn,
            ]
        );
    }

    #[test]
    fn double_thief_bridge_proposal_does_not_replace_missing_components() {
        let mut position = double_thief_bridge_position();
        position
            .combat
            .zones
            .hand
            .retain(|card| card.id != CardId::PowerThrough);

        assert!(combat_plan_turn_prefix_proposal_v1(&position).is_none());
    }

    #[test]
    fn double_thief_continuation_secures_lethal_behind_existing_exhaust_block() {
        let position = double_thief_exhaust_block_position();

        let proposal =
            combat_plan_turn_prefix_proposal_v1(&position).expect("typed thief continuation");

        assert_eq!(
            proposal.kind,
            CombatPlanPrefixKindV1::SecureThiefKillBehindExhaustBlock
        );
        assert_eq!(
            proposal.service_scope,
            CombatPlanPrefixServiceScopeV1::ContinuationOnly
        );
        assert_eq!(
            proposal.steps,
            vec![
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(10004),
                    target: None,
                },
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(4),
                    target: Some(20),
                },
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(10001),
                    target: None,
                },
                CombatPlanPrefixStepV1::EndTurn,
            ]
        );
    }

    #[test]
    fn double_thief_continuation_requires_held_exhaust_fuel_for_visible_margin() {
        let mut position = double_thief_exhaust_block_position();
        position
            .combat
            .zones
            .hand
            .retain(|card| card.id != CardId::Defend);

        assert!(combat_plan_turn_prefix_proposal_v1(&position).is_none());
    }

    #[test]
    fn single_thief_smoke_bomb_window_spends_safe_energy_on_pressure() {
        let position = escaping_single_thief_position(2);

        let proposal =
            combat_plan_turn_prefix_proposal_v1(&position).expect("typed smoke-bomb pressure");

        assert_eq!(
            proposal.kind,
            CombatPlanPrefixKindV1::PressSingleThiefEscapeWindow
        );
        assert_eq!(
            proposal.service_scope,
            CombatPlanPrefixServiceScopeV1::ContinuationOnly
        );
        assert_eq!(
            proposal.steps,
            vec![
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(2),
                    target: Some(10),
                },
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(1),
                    target: Some(10),
                },
                CombatPlanPrefixStepV1::EndTurn,
            ]
        );
    }

    #[test]
    fn single_thief_escape_window_applies_vulnerability_before_strikes() {
        let mut position = escaping_single_thief_position(3);
        position
            .combat
            .zones
            .hand
            .push(CombatCard::for_scenario(CardId::ThunderClap, 10003));

        let proposal =
            combat_plan_turn_prefix_proposal_v1(&position).expect("typed escape lethal pressure");

        assert_eq!(
            proposal.steps,
            vec![
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(10003),
                    target: None,
                },
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(2),
                    target: Some(10),
                },
                CombatPlanPrefixStepV1::PlayCard {
                    card_lineage: CardLineageId::constructed_scenario(1),
                    target: Some(10),
                },
                CombatPlanPrefixStepV1::EndTurn,
            ]
        );
    }

    #[test]
    fn plan_resources_count_realized_exhaust_block_with_live_fuel() {
        let mut unsupported = awakened_position(2);
        unsupported.combat.zones.draw_pile =
            vec![CombatCard::for_scenario(CardId::SecondWind, 1)].into();
        let mut supported = unsupported.clone();
        store::set_powers_for(
            &mut supported.combat,
            0,
            vec![power(PowerId::FeelNoPain, 4)],
        );

        let unsupported_plan = awakened_one_combat_plan_v1(&unsupported).expect("unsupported plan");
        let supported_plan = awakened_one_combat_plan_v1(&supported).expect("supported plan");
        assert_eq!(unsupported_plan.resources.remaining_exhaust_sources, 1);
        assert_eq!(supported_plan.resources.remaining_exhaust_sources, 1);
        assert!(supported_plan.resources.exhaust_block_active);
    }

    #[test]
    fn plan_resources_do_not_invent_exhaust_fuel_from_an_active_power() {
        let unsupported = awakened_position(2);
        let mut no_fuel = unsupported.clone();
        store::set_powers_for(&mut no_fuel.combat, 0, vec![power(PowerId::FeelNoPain, 4)]);

        let plan = awakened_one_combat_plan_v1(&no_fuel).expect("no-fuel plan");
        assert_eq!(plan.resources.remaining_exhaust_sources, 0);
        assert!(plan.resources.exhaust_block_active);
    }

    fn donu_deca_position() -> CombatPosition {
        let mut combat = blank_test_combat();
        let mut deca = test_monster(EnemyId::Deca);
        deca.id = 10;
        deca.slot = 0;
        let mut donu = test_monster(EnemyId::Donu);
        donu.id = 11;
        donu.slot = 1;
        combat.entities.monsters = vec![deca, donu];
        store::set_powers_for(&mut combat, 10, vec![power(PowerId::Artifact, 2)]);
        store::set_powers_for(&mut combat, 11, vec![power(PowerId::Artifact, 2)]);
        CombatPosition::new(EngineState::CombatPlayerTurn, combat)
    }

    #[test]
    fn donu_and_deca_plan_exposes_growth_control_without_forcing_a_target() {
        let mut position = donu_deca_position();
        position.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Disarm, 1),
            CombatCard::for_scenario(CardId::DemonForm, 2),
        ];

        let plan = donu_and_deca_combat_plan_v1(&position).expect("Donu and Deca plan");

        assert_eq!(plan.plan, CombatPlanIdV1::DonuAndDecaGrowthControl);
        assert_eq!(plan.stage, CombatPlanStageV1::EliminateTeamGrowthSource);
        assert_eq!(
            plan.next_milestone,
            CombatPlanMilestoneV1::TeamGrowthSourceEliminated
        );
        assert!(plan
            .supporting
            .contains(&CombatPlanObligationV1::ExposeAttackMitigationTarget {
                protected_attackers: 2
            }));
        assert!(plan
            .supporting
            .contains(&CombatPlanObligationV1::EstablishDurableScaling));
    }

    #[test]
    fn donu_plan_guide_prefers_equal_progress_on_the_growth_source() {
        let base = donu_deca_position();
        let mut damaged_deca = base.clone();
        damaged_deca.combat.entities.monsters[0].current_hp -= 20;
        let mut damaged_donu = base;
        damaged_donu.combat.entities.monsters[1].current_hp -= 20;

        let deca_rank = combat_plan_state_guide_rank_v1(&damaged_deca).expect("Deca-damage rank");
        let donu_rank = combat_plan_state_guide_rank_v1(&damaged_donu).expect("Donu-damage rank");

        assert!(donu_rank.components() > deca_rank.components());
    }

    #[test]
    fn mitigation_exposure_matters_only_while_strength_reduction_remains() {
        let mut protected = donu_deca_position();
        protected.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Disarm, 1)];
        let mut exposed = protected.clone();
        store::set_powers_for(&mut exposed.combat, 10, Vec::new());

        let protected_rank = combat_plan_state_guide_rank_v1(&protected).expect("protected rank");
        let exposed_rank = combat_plan_state_guide_rank_v1(&exposed).expect("exposed rank");
        assert!(exposed_rank.components() > protected_rank.components());

        protected.combat.zones.hand.clear();
        exposed.combat.zones.hand.clear();
        let protected_without_reduction =
            combat_plan_state_guide_rank_v1(&protected).expect("protected no-reduction rank");
        let exposed_without_reduction =
            combat_plan_state_guide_rank_v1(&exposed).expect("exposed no-reduction rank");
        assert_eq!(
            protected_without_reduction.components(),
            exposed_without_reduction.components()
        );
    }

    #[test]
    fn killing_donu_completes_the_growth_source_milestone() {
        let before = donu_deca_position();
        let mut after = before.clone();
        after.combat.entities.monsters[1].current_hp = 0;

        let transition =
            donu_and_deca_plan_transition_v1(&before, &after).expect("Donu transition");

        assert_eq!(
            transition.completed_milestones,
            vec![CombatPlanMilestoneV1::TeamGrowthSourceEliminated]
        );
        assert_eq!(
            transition.after_stage,
            Some(CombatPlanStageV1::ConvertToLethal)
        );
    }

    #[test]
    fn form_one_starts_by_removing_live_scaling_adds() {
        let mut position = awakened_position(2);
        position.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Corruption, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
            CombatCard::for_scenario(CardId::DarkEmbrace, 3),
        ];

        let plan = awakened_one_combat_plan_v1(&position).expect("Awakened One plan");

        assert_eq!(plan.stage, CombatPlanStageV1::RemoveEscalatingAdds);
        assert_eq!(
            plan.primary,
            CombatPlanObligationV1::EliminateEscalatingAdds { remaining: 2 }
        );
        assert!(plan
            .supporting
            .contains(&CombatPlanObligationV1::AccountForReactivePowerTax {
                strength_per_power: 1
            }));
        assert!(plan
            .supporting
            .contains(&CombatPlanObligationV1::PreserveFiniteSkillFuel {
                remaining_skills: 1,
                conversion: FiniteSkillConversionStateV1::Available,
            }));
    }

    #[test]
    fn cleared_adds_switch_to_a_survival_gated_phase_commit() {
        let position = awakened_position(0);

        let plan = awakened_one_combat_plan_v1(&position).expect("Awakened One plan");

        assert_eq!(plan.stage, CombatPlanStageV1::PrepareFirstPhaseCommit);
        assert_eq!(
            plan.primary,
            CombatPlanObligationV1::ProvePhaseTransitionSurvival
        );
        assert_eq!(
            plan.next_milestone,
            CombatPlanMilestoneV1::UntaxedTransitionWindowReached
        );
    }

    #[test]
    fn live_status_burden_is_an_obligation_until_status_draw_is_active() {
        let mut position = awakened_position(0);
        position.combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Wound, 1),
            CombatCard::for_scenario(CardId::Wound, 2),
        ]
        .into();

        let unresolved = awakened_one_combat_plan_v1(&position).expect("Awakened One plan");
        assert!(unresolved
            .supporting
            .contains(&CombatPlanObligationV1::ManageLiveStatusBurden {
                live_status_cards: 2,
            }));

        store::set_powers_for(&mut position.combat, 0, vec![power(PowerId::Evolve, 1)]);
        let managed = awakened_one_combat_plan_v1(&position).expect("Awakened One plan");
        assert!(!managed.supporting.iter().any(|obligation| matches!(
            obligation,
            CombatPlanObligationV1::ManageLiveStatusBurden { .. }
        )));
        assert!(managed.resources.status_draw_active);
    }

    #[test]
    fn sentinel_corruption_power_is_active_finite_skill_conversion() {
        let mut position = awakened_position(0);
        position.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 1)];
        store::set_powers_for(
            &mut position.combat,
            0,
            vec![power(PowerId::Corruption, -1)],
        );

        let plan = awakened_one_combat_plan_v1(&position).expect("Awakened One plan");

        assert_eq!(
            plan.resources.finite_skill_conversion,
            FiniteSkillConversionStateV1::Active
        );
        assert!(plan
            .supporting
            .contains(&CombatPlanObligationV1::PreserveFiniteSkillFuel {
                remaining_skills: 1,
                conversion: FiniteSkillConversionStateV1::Active,
            }));
    }

    #[test]
    fn safe_first_phase_action_that_spends_reserved_conversion_is_deferred() {
        let mut before = awakened_position(0);
        before.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Corruption, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        let mut after = before.clone();
        after.combat.zones.hand.remove(0);
        store::set_powers_for(&mut after.combat, 0, vec![power(PowerId::Corruption, -1)]);

        assert_eq!(
            combat_plan_action_deferral_v1(&before, &after),
            Some(CombatPlanActionDeferralV1::PreserveFiniteSkillConversionUntilUntaxedWindow)
        );
    }

    #[test]
    fn forced_exhaust_preserves_an_undeployed_plan_asset_when_an_alternative_exists() {
        let mut position = awakened_position(2);
        position.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::DemonForm, 11),
            CombatCard::for_scenario(CardId::Strike, 12),
        ];
        position.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: vec![
                CardLineageId::constructed_scenario(11),
                CardLineageId::constructed_scenario(12),
            ],
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: HandSelectReason::Exhaust,
        });
        let surface = sts_core::sim::combat_action_surface::combat_legal_action_surface_v2(
            &position.engine,
            &position.combat,
        );
        let family = surface
            .selection_families
            .first()
            .expect("forced exhaust family");
        let exhaust_demon = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Hand,
            [CardLineageId::constructed_scenario(11)],
        ));
        let exhaust_strike = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Hand,
            [CardLineageId::constructed_scenario(12)],
        ));

        assert_eq!(
            combat_plan_selection_member_timing_v1(&position, family, &exhaust_demon),
            CombatPlanActionTimingV1::Defer(
                CombatPlanActionDeferralV1::PreserveUndeployedPlanAsset
            )
        );
        assert_eq!(
            combat_plan_selection_member_timing_v1(&position, family, &exhaust_strike),
            CombatPlanActionTimingV1::Neutral
        );
    }

    #[test]
    fn conversion_is_not_deferred_when_the_visible_attack_is_lethal() {
        let mut before = awakened_position(0);
        before.combat.entities.player.current_hp = 1;
        before.combat.entities.monsters[0].set_planned_move_id(1);
        before.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Corruption, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        let mut after = before.clone();
        after.combat.zones.hand.remove(0);
        store::set_powers_for(&mut after.combat, 0, vec![power(PowerId::Corruption, -1)]);

        assert!(
            awakened_one_combat_plan_v1(&before)
                .expect("Awakened One plan")
                .envelope
                .visible_damage_margin
                < 0
        );
        assert_eq!(combat_plan_action_deferral_v1(&before, &after), None);
    }

    #[test]
    fn conversion_is_not_deferred_after_the_first_phase_commit() {
        let mut before = awakened_position(0);
        before.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Corruption, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        before.combat.entities.monsters[0].awakened_one.form1 = false;
        before.combat.entities.monsters[0].half_dead = true;
        before.combat.entities.monsters[0].current_hp = 0;
        let mut after = before.clone();
        after.combat.zones.hand.remove(0);
        store::set_powers_for(&mut after.combat, 0, vec![power(PowerId::Corruption, -1)]);

        assert_eq!(combat_plan_action_deferral_v1(&before, &after), None);
        assert_eq!(
            combat_plan_action_timing_v1(&before, &after),
            CombatPlanActionTimingV1::PreferNow
        );
    }

    #[test]
    fn untaxed_window_prefers_realizing_held_demon_form() {
        let mut before = awakened_position(0);
        let awakened = &mut before.combat.entities.monsters[0];
        awakened.awakened_one.form1 = false;
        awakened.half_dead = true;
        awakened.current_hp = 0;
        before.combat.zones.hand = vec![CombatCard::for_scenario(CardId::DemonForm, 1)];
        let mut after = before.clone();
        after.combat.zones.hand.clear();
        store::set_powers_for(&mut after.combat, 0, vec![power(PowerId::DemonForm, 3)]);

        assert!(combat_plan_has_timed_action_preference_v1(&before));
        assert!(!combat_plan_supports_initial_policy_prefix_v1(&before));
        assert_eq!(
            combat_plan_action_timing_v1(&before, &after),
            CombatPlanActionTimingV1::PreferNow
        );
    }

    #[test]
    fn live_first_phase_does_not_force_demon_form_deployment() {
        let mut before = awakened_position(0);
        before.combat.zones.hand = vec![CombatCard::for_scenario(CardId::DemonForm, 1)];
        let mut after = before.clone();
        after.combat.zones.hand.clear();
        store::set_powers_for(&mut after.combat, 0, vec![power(PowerId::DemonForm, 3)]);
        store::set_powers_for(
            &mut after.combat,
            10,
            vec![power(PowerId::Curiosity, 1), power(PowerId::Strength, 1)],
        );

        assert!(!combat_plan_has_timed_action_preference_v1(&before));
        assert_eq!(
            combat_plan_action_timing_v1(&before, &after),
            CombatPlanActionTimingV1::Neutral
        );
    }

    #[test]
    fn safe_first_phase_bulk_exhaust_that_destroys_demon_form_is_deferred() {
        let mut before = awakened_position(0);
        before.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::DemonForm, 1),
            CombatCard::for_scenario(CardId::SeverSoul, 2),
        ];
        let mut after = before.clone();
        after.combat.zones.hand.clear();

        assert_eq!(
            combat_plan_action_timing_v1(&before, &after),
            CombatPlanActionTimingV1::Defer(
                CombatPlanActionDeferralV1::PreserveUndeployedPlanAsset
            )
        );
    }

    #[test]
    fn bulk_exhaust_asset_loss_is_not_deferred_when_it_commits_the_first_phase() {
        let mut before = awakened_position(0);
        before.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::DemonForm, 1),
            CombatCard::for_scenario(CardId::SeverSoul, 2),
        ];
        let mut after = before.clone();
        after.combat.zones.hand.clear();
        let awakened = &mut after.combat.entities.monsters[0];
        awakened.awakened_one.form1 = false;
        awakened.half_dead = true;
        awakened.current_hp = 0;

        assert_eq!(
            combat_plan_action_timing_v1(&before, &after),
            CombatPlanActionTimingV1::Neutral
        );
    }

    #[test]
    fn first_phase_death_opens_an_untaxed_setup_window() {
        let mut position = awakened_position(0);
        let awakened = &mut position.combat.entities.monsters[0];
        awakened.awakened_one.form1 = false;
        awakened.half_dead = true;
        awakened.current_hp = 0;
        position.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Corruption, 1),
            CombatCard::for_scenario(CardId::DarkEmbrace, 2),
        ];

        let plan = awakened_one_combat_plan_v1(&position).expect("Awakened One plan");

        assert_eq!(plan.stage, CombatPlanStageV1::ExploitTransitionWindow);
        assert_eq!(
            plan.next_milestone,
            CombatPlanMilestoneV1::TransitionWindowClosed
        );
        assert_eq!(
            plan.primary,
            CombatPlanObligationV1::DeployHeldSetupInUntaxedWindow {
                undeployed_power_cards: 2
            }
        );
        assert!(!plan.supporting.iter().any(|obligation| matches!(
            obligation,
            CombatPlanObligationV1::AccountForReactivePowerTax { .. }
        )));
    }

    #[test]
    fn prepare_stage_guide_keeps_reserved_conversion_as_an_independent_lane() {
        let mut reserved = awakened_position(0);
        reserved.combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Corruption, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        let mut consumed = reserved.clone();
        consumed.combat.zones.hand.remove(0);
        store::set_powers_for(
            &mut consumed.combat,
            0,
            vec![power(PowerId::Corruption, -1)],
        );

        let reserved_rank = combat_plan_state_guide_rank_v1(&reserved).expect("reserved plan rank");
        let consumed_rank = combat_plan_state_guide_rank_v1(&consumed).expect("consumed plan rank");

        assert!(reserved_rank.components() > consumed_rank.components());
    }

    #[test]
    fn prepare_stage_does_not_trade_away_demon_form_for_local_boss_damage() {
        let mut retained = awakened_position(0);
        retained.combat.entities.monsters[0].current_hp = 200;
        retained.combat.zones.hand = vec![CombatCard::for_scenario(CardId::DemonForm, 1)];
        let mut destroyed = retained.clone();
        destroyed.combat.entities.monsters[0].current_hp = 30;
        destroyed.combat.zones.hand.clear();

        let retained_rank =
            combat_plan_state_guide_rank_v1(&retained).expect("retained scaling rank");
        let destroyed_rank =
            combat_plan_state_guide_rank_v1(&destroyed).expect("destroyed scaling rank");

        assert!(retained_rank.components() > destroyed_rank.components());
    }

    #[test]
    fn transition_window_does_not_mistake_destroyed_demon_form_for_deployment() {
        let mut retained = awakened_position(0);
        let awakened = &mut retained.combat.entities.monsters[0];
        awakened.awakened_one.form1 = false;
        awakened.half_dead = true;
        awakened.current_hp = 0;
        retained.combat.zones.hand = vec![CombatCard::for_scenario(CardId::DemonForm, 1)];
        let mut destroyed = retained.clone();
        destroyed.combat.zones.hand.clear();

        let retained_rank =
            combat_plan_state_guide_rank_v1(&retained).expect("retained transition rank");
        let destroyed_rank =
            combat_plan_state_guide_rank_v1(&destroyed).expect("destroyed transition rank");

        assert!(retained_rank.components() > destroyed_rank.components());
    }

    #[test]
    fn live_second_form_first_requires_surviving_dark_echo_window() {
        let mut position = awakened_position(0);
        let awakened = &mut position.combat.entities.monsters[0];
        awakened.awakened_one.form1 = false;
        awakened.awakened_one.first_turn = true;
        awakened.half_dead = false;
        awakened.current_hp = 300;

        let plan = awakened_one_combat_plan_v1(&position).expect("Awakened One plan");

        assert_eq!(plan.stage, CombatPlanStageV1::SurviveSecondPhaseOpening);
        assert_eq!(
            plan.primary,
            CombatPlanObligationV1::SurviveSecondPhaseOpening
        );
    }

    #[test]
    fn established_second_form_converts_preparation_to_lethal() {
        let mut position = awakened_position(0);
        let awakened = &mut position.combat.entities.monsters[0];
        awakened.awakened_one.form1 = false;
        awakened.awakened_one.first_turn = false;
        awakened.half_dead = false;
        awakened.current_hp = 210;

        let plan = awakened_one_combat_plan_v1(&position).expect("Awakened One plan");

        assert_eq!(plan.stage, CombatPlanStageV1::ConvertToLethal);
        assert_eq!(
            plan.primary,
            CombatPlanObligationV1::ConvertPreparedEngineToLethal
        );
    }

    #[test]
    fn ordinary_skill_identity_does_not_change_the_plan() {
        let mut first = awakened_position(0);
        first.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Bloodletting, 1)];
        let mut second = awakened_position(0);
        second.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Flex, 1)];

        assert_eq!(
            awakened_one_combat_plan_v1(&first),
            awakened_one_combat_plan_v1(&second)
        );
    }

    #[test]
    fn unrelated_encounter_has_no_awakened_one_plan() {
        let mut combat = blank_test_combat();
        combat
            .entities
            .monsters
            .push(test_monster(EnemyId::JawWorm));
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

        assert_eq!(awakened_one_combat_plan_v1(&position), None);
    }

    #[test]
    fn exact_transition_reports_add_cleanup_without_scoring_the_action() {
        let before = awakened_position(1);
        let mut after = before.clone();
        after.combat.entities.monsters[1].current_hp = 0;
        after.combat.entities.monsters[1].is_dying = true;

        let transition = awakened_one_plan_transition_v1(&before, &after).expect("plan transition");

        assert_eq!(
            transition.completed_milestones,
            vec![CombatPlanMilestoneV1::EscalatingAddsRemoved]
        );
        assert_eq!(
            transition.after_stage,
            Some(CombatPlanStageV1::PrepareFirstPhaseCommit)
        );
    }

    #[test]
    fn generic_transition_dispatch_preserves_the_typed_plan_variant() {
        let before = awakened_position(0);
        let mut after = before.clone();
        after.combat.entities.monsters[0].current_hp -= 5;

        let annotation =
            combat_plan_transition_annotation_v1(&before, &after).expect("typed annotation");

        assert!(matches!(
            annotation,
            CombatPlanTransitionAnnotationV1::AwakenedOnePhaseControl(_)
        ));
    }

    #[test]
    fn exact_transition_reports_the_untaxed_phase_window() {
        let before = awakened_position(0);
        let mut after = before.clone();
        let awakened = &mut after.combat.entities.monsters[0];
        awakened.awakened_one.form1 = false;
        awakened.half_dead = true;
        awakened.current_hp = 0;

        let transition = awakened_one_plan_transition_v1(&before, &after).expect("plan transition");

        assert_eq!(
            transition.completed_milestones,
            vec![CombatPlanMilestoneV1::UntaxedTransitionWindowReached]
        );
        assert_eq!(
            transition.after_stage,
            Some(CombatPlanStageV1::ExploitTransitionWindow)
        );
    }

    #[test]
    fn second_form_revival_closes_the_untaxed_transition_window() {
        let mut before = awakened_position(0);
        let awakened = &mut before.combat.entities.monsters[0];
        awakened.awakened_one.form1 = false;
        awakened.half_dead = true;
        awakened.current_hp = 0;

        let mut after = before.clone();
        let awakened = &mut after.combat.entities.monsters[0];
        awakened.awakened_one.first_turn = true;
        awakened.half_dead = false;
        awakened.current_hp = 300;

        let transition = awakened_one_plan_transition_v1(&before, &after).expect("plan transition");

        assert_eq!(
            transition.completed_milestones,
            vec![CombatPlanMilestoneV1::TransitionWindowClosed]
        );
        assert_eq!(
            transition.after_stage,
            Some(CombatPlanStageV1::SurviveSecondPhaseOpening)
        );
    }

    #[test]
    fn same_stage_transition_does_not_invent_progress_or_failure() {
        let before = awakened_position(0);
        let before_phase_hp = awakened_one_combat_plan_v1(&before)
            .expect("before plan")
            .envelope
            .first_phase_hp_with_block
            .expect("first phase HP");
        let mut after = before.clone();
        after.combat.entities.monsters[0].current_hp -= 20;

        let transition = awakened_one_plan_transition_v1(&before, &after).expect("plan transition");

        assert!(transition.completed_milestones.is_empty());
        assert_eq!(
            transition.after_stage,
            Some(CombatPlanStageV1::PrepareFirstPhaseCommit)
        );
        assert!(transition.events.contains(
            &CombatPlanTransitionEventV1::FirstPhaseHpWithBlockChanged {
                before: Some(before_phase_hp),
                after: Some(before_phase_hp - 20),
            }
        ));
    }

    #[test]
    fn terminal_victory_has_no_live_plan_but_preserves_defeat_milestone_evidence() {
        let before = awakened_position(0);
        let mut after = before.clone();
        after.engine = EngineState::GameOver(sts_core::state::core::RunResult::Victory);

        assert!(awakened_one_combat_plan_v1(&after).is_none());
        let transition = awakened_one_plan_transition_v1(&before, &after)
            .expect("terminal transition still belongs to the prior plan");
        assert_eq!(
            transition.completed_milestones,
            vec![CombatPlanMilestoneV1::EncounterDefeated]
        );
        assert_eq!(transition.after_stage, None);
    }

    #[test]
    fn exact_transition_observes_status_burden_without_judging_the_tradeoff() {
        let before = awakened_position(0);
        let mut after = before.clone();
        after
            .combat
            .zones
            .draw_pile
            .push(CombatCard::for_scenario(CardId::Wound, 50));

        let transition = awakened_one_plan_transition_v1(&before, &after).expect("plan transition");

        assert!(transition.events.contains(
            &CombatPlanTransitionEventV1::LiveStatusBurdenChanged {
                before: 0,
                after: 1,
            }
        ));
        assert!(transition.completed_milestones.is_empty());
    }

    #[test]
    fn exact_transition_observes_visible_damage_margin_without_calling_it_value() {
        let before = awakened_position(0);
        let mut after = before.clone();
        after.combat.entities.player.block = 8;

        let transition = awakened_one_plan_transition_v1(&before, &after).expect("plan transition");

        assert!(transition.events.contains(
            &CombatPlanTransitionEventV1::VisibleDamageMarginChanged {
                before: transition.envelope_before.visible_damage_margin,
                after: transition
                    .envelope_after
                    .expect("successor envelope")
                    .visible_damage_margin,
            }
        ));
        assert_eq!(
            transition
                .envelope_after
                .expect("successor envelope")
                .player_block,
            8
        );
    }

    #[test]
    fn exact_transition_observes_reactive_strength_tax_as_a_fact() {
        let mut before = awakened_position(0);
        store::set_powers_for(
            &mut before.combat,
            10,
            vec![power(PowerId::Curiosity, 1), power(PowerId::Strength, 2)],
        );
        let mut after = before.clone();
        store::set_powers_for(
            &mut after.combat,
            10,
            vec![power(PowerId::Curiosity, 1), power(PowerId::Strength, 3)],
        );

        let transition = awakened_one_plan_transition_v1(&before, &after).expect("plan transition");

        assert!(transition.events.contains(
            &CombatPlanTransitionEventV1::ReactiveStrengthChanged {
                before: 2,
                after: 3,
            }
        ));
    }

    #[test]
    fn phase_commit_progress_can_outweigh_reactive_tax_after_survival() {
        let mut stalled = awakened_position(0);
        stalled.combat.entities.monsters[0].current_hp = 120;
        let mut advanced = stalled.clone();
        advanced.combat.entities.monsters[0].current_hp = 30;
        store::set_powers_for(
            &mut advanced.combat,
            10,
            vec![power(PowerId::Curiosity, 1), power(PowerId::Strength, 2)],
        );

        let stalled_rank = combat_plan_state_guide_rank_v1(&stalled).expect("stalled rank");
        let advanced_rank = combat_plan_state_guide_rank_v1(&advanced).expect("advanced rank");

        assert!(advanced_rank.components() > stalled_rank.components());
    }

    #[test]
    fn reactive_tax_still_breaks_ties_at_equal_phase_progress() {
        let untaxed = awakened_position(0);
        let mut taxed = untaxed.clone();
        store::set_powers_for(
            &mut taxed.combat,
            10,
            vec![power(PowerId::Curiosity, 1), power(PowerId::Strength, 2)],
        );

        let untaxed_rank = combat_plan_state_guide_rank_v1(&untaxed).expect("untaxed rank");
        let taxed_rank = combat_plan_state_guide_rank_v1(&taxed).expect("taxed rank");

        assert!(untaxed_rank.components() > taxed_rank.components());
    }

    #[test]
    fn realized_durable_scaling_is_not_misread_as_only_reactive_power_tax() {
        let unscaled = awakened_position(2);
        let mut scaled = unscaled.clone();
        store::set_powers_for(&mut scaled.combat, 0, vec![power(PowerId::DemonForm, 3)]);
        store::set_powers_for(
            &mut scaled.combat,
            10,
            vec![power(PowerId::Curiosity, 1), power(PowerId::Strength, 1)],
        );

        let unscaled_plan = awakened_one_combat_plan_v1(&unscaled).expect("unscaled plan");
        let scaled_plan = awakened_one_combat_plan_v1(&scaled).expect("scaled plan");
        assert_eq!(unscaled_plan.resources.durable_strength_growth, 0);
        assert_eq!(scaled_plan.resources.durable_strength_growth, 3);

        let unscaled_rank =
            combat_plan_state_guide_rank_v1(&unscaled).expect("unscaled guide rank");
        let scaled_rank = combat_plan_state_guide_rank_v1(&scaled).expect("scaled guide rank");
        assert!(
            scaled_rank.components() > unscaled_rank.components(),
            "realized persistent scaling must be visible before the reactive-tax tie break"
        );
    }
}
