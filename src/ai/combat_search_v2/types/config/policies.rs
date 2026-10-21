use serde::{Deserialize, Serialize};

/// Diagnostic-only switches for identifying which legacy frontier signals
/// contribute useful search capability. Production search always uses
/// `Baseline`; the other variants are exposed only through explicit lab
/// session construction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CombatSearchV2PriorityAblation {
    #[default]
    Baseline,
    NoActionGuidance,
    NoStateValue,
    NoActionGuidanceOrStateValue,
}

impl CombatSearchV2PriorityAblation {
    pub fn label(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::NoActionGuidance => "no_action_guidance",
            Self::NoStateValue => "no_state_value",
            Self::NoActionGuidanceOrStateValue => "no_action_guidance_or_state_value",
        }
    }

    pub(in crate::ai::combat_search_v2) fn uses_action_guidance(self) -> bool {
        matches!(self, Self::Baseline | Self::NoStateValue)
    }

    pub(in crate::ai::combat_search_v2) fn uses_state_value(self) -> bool {
        matches!(self, Self::Baseline | Self::NoActionGuidance)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CombatSearchV2SetupBiasPolicy {
    #[default]
    Default,
    KeyCardOnline,
}

impl CombatSearchV2SetupBiasPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::KeyCardOnline => "key_card_online",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CombatSearchV2ExpansionPolicy {
    #[default]
    AtomicActions,
    HierarchicalTurnBoundary,
}

impl CombatSearchV2ExpansionPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::AtomicActions => "atomic_actions",
            Self::HierarchicalTurnBoundary => "hierarchical_turn_boundary",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CombatSearchV2PhaseGuardPolicy {
    #[default]
    Default,
    ChampSplitGuard,
    TimeEaterClockHint,
}

impl CombatSearchV2PhaseGuardPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::ChampSplitGuard => "champ_split_guard",
            Self::TimeEaterClockHint => "time_eater_clock_hint",
        }
    }
}

/// Compatibility name retained inside the atomic-v2 module. Potion admission
/// shared with the production portfolio is owned by the neutral contract.
pub type CombatSearchV2PotionPolicy =
    crate::ai::combat_witness_contract::CombatWitnessPotionPolicyV1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CombatSearchV2ChildRolloutPolicy {
    Immediate,
    #[default]
    LazyOnPop,
}

impl CombatSearchV2ChildRolloutPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::LazyOnPop => "lazy_on_pop",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CombatSearchV2RolloutPolicy {
    Disabled,
    #[serde(alias = "adaptive_no_potion")]
    #[default]
    EnemyMechanicsAdaptiveNoPotion,
    ConservativeNoPotion,
    PhaseAwareNoPotion,
    TurnBeamNoPotion,
}

impl CombatSearchV2RolloutPolicy {
    pub fn label(self) -> &'static str {
        match self {
            CombatSearchV2RolloutPolicy::Disabled => "disabled",
            CombatSearchV2RolloutPolicy::EnemyMechanicsAdaptiveNoPotion => {
                "enemy_mechanics_adaptive_no_potion"
            }
            CombatSearchV2RolloutPolicy::ConservativeNoPotion => "conservative_no_potion",
            CombatSearchV2RolloutPolicy::PhaseAwareNoPotion => "phase_aware_no_potion",
            CombatSearchV2RolloutPolicy::TurnBeamNoPotion => "turn_beam_no_potion",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CombatSearchV2TurnPlanPolicy {
    #[default]
    Disabled,
    DiagnosticOnly,
    RootFrontierSeed,
    TurnBoundaryFrontierSeed,
    #[serde(alias = "support_enemy_turn_boundary_frontier_seed")]
    TacticalEnemyTurnBoundaryFrontierSeed,
}

impl CombatSearchV2TurnPlanPolicy {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::DiagnosticOnly => "diagnostic_only",
            Self::RootFrontierSeed => "root_frontier_seed",
            Self::TurnBoundaryFrontierSeed => "turn_boundary_frontier_seed",
            Self::TacticalEnemyTurnBoundaryFrontierSeed => {
                "tactical_enemy_turn_boundary_frontier_seed"
            }
        }
    }

    pub(in crate::ai::combat_search_v2) fn seeds_root_frontier(self) -> bool {
        matches!(
            self,
            Self::RootFrontierSeed | Self::TurnBoundaryFrontierSeed
        )
    }

    pub(in crate::ai::combat_search_v2) fn seeds_turn_boundary_frontier(self) -> bool {
        matches!(
            self,
            Self::TurnBoundaryFrontierSeed | Self::TacticalEnemyTurnBoundaryFrontierSeed
        )
    }

    pub(in crate::ai::combat_search_v2) fn requires_tactical_enemy_gate(self) -> bool {
        matches!(self, Self::TacticalEnemyTurnBoundaryFrontierSeed)
    }

    pub(in crate::ai::combat_search_v2) fn observes_root_diagnostics(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}
