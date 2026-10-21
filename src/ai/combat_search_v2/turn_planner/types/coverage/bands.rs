#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) struct TurnPlanCoverageKeyV1 {
    pub(in crate::ai::combat_search_v2) damage: TurnPlanDamageBandV1,
    pub(in crate::ai::combat_search_v2) block: TurnPlanBlockBandV1,
    pub(in crate::ai::combat_search_v2) debuff: TurnPlanDebuffClassV1,
    pub(in crate::ai::combat_search_v2) setup: TurnPlanSetupClassV1,
    pub(in crate::ai::combat_search_v2) resource: TurnPlanResourceClassV1,
    pub(in crate::ai::combat_search_v2) risk: TurnPlanRiskBandV1,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum TurnPlanDamageBandV1 {
    None,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum TurnPlanBlockBandV1 {
    None,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum TurnPlanDebuffClassV1 {
    None,
    Weak,
    Vulnerable,
    StrengthDown,
    Mixed,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum TurnPlanSetupClassV1 {
    None,
    Power,
    PlayerStrength,
    AccessGain,
    ExhaustOrQueueChange,
    Mixed,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum TurnPlanResourceClassV1 {
    Neutral,
    SpendsEnergy,
    UsesPotion,
    GainsAccess,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum TurnPlanRiskBandV1 {
    NoHpLoss,
    LowHpLoss,
    HighHpLoss,
    ForcedTurnEndOrReactiveLoss,
}

impl TurnPlanDamageBandV1 {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl TurnPlanBlockBandV1 {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl TurnPlanDebuffClassV1 {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Weak => "weak",
            Self::Vulnerable => "vulnerable",
            Self::StrengthDown => "strength_down",
            Self::Mixed => "mixed",
        }
    }
}

impl TurnPlanSetupClassV1 {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Power => "power",
            Self::PlayerStrength => "player_strength",
            Self::AccessGain => "access_gain",
            Self::ExhaustOrQueueChange => "exhaust_or_queue_change",
            Self::Mixed => "mixed",
        }
    }
}

impl TurnPlanResourceClassV1 {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::SpendsEnergy => "spends_energy",
            Self::UsesPotion => "uses_potion",
            Self::GainsAccess => "gains_access",
        }
    }
}

impl TurnPlanRiskBandV1 {
    pub(in crate::ai::combat_search_v2) fn label(self) -> &'static str {
        match self {
            Self::NoHpLoss => "no_hp_loss",
            Self::LowHpLoss => "low_hp_loss",
            Self::HighHpLoss => "high_hp_loss",
            Self::ForcedTurnEndOrReactiveLoss => "forced_turn_end_or_reactive_loss",
        }
    }
}
