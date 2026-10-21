#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PotionGateDecision {
    pub(super) allowed: bool,
    pub(super) reason: PotionGateReason,
    pub(super) role: Option<PotionTacticalRole>,
}

impl PotionGateDecision {
    pub(super) fn allow(reason: PotionGateReason, role: PotionTacticalRole) -> Self {
        Self {
            allowed: true,
            reason,
            role: Some(role),
        }
    }

    pub(super) fn reject(reason: PotionGateReason) -> Self {
        Self {
            allowed: false,
            reason,
            role: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum PotionTacticalRole {
    SustainResource,
    HighStakesResourceConversion,
    PreventUncoveredDamage,
    PreventVisibleLethal,
    LethalDamage,
}

impl PotionTacticalRole {
    pub(super) fn priority_rank(self) -> i32 {
        match self {
            PotionTacticalRole::LethalDamage => 50,
            PotionTacticalRole::PreventVisibleLethal => 40,
            PotionTacticalRole::PreventUncoveredDamage => 30,
            PotionTacticalRole::HighStakesResourceConversion => 20,
            PotionTacticalRole::SustainResource => 10,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PotionGateReason {
    DirectDamageCanKill,
    VisibleIncomingLethal,
    VisibleIncomingUncoveredByHandBlock,
    VisibleIncomingFullyBlockable,
    HighStakesNoVisibleHandLethal,
    PlayerWounded,
    InvalidPotionAction,
    PotionSlotMissing,
    PassiveOnly,
    InvalidTarget,
    NoLivingEnemy,
    NoVisibleIncomingHpLoss,
    NoTacticalPressure,
    NotWounded,
    NoSelectableCards,
    EscapeNotWin,
    RandomPotionGenerationUnsupported,
}
