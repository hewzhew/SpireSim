use crate::ai::combat_search_v2::CombatSearchV2PotionPolicy;

pub const DEFAULT_COMBAT_AUTO_SEARCH_WALL_MS: u64 = 5_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatAutoHpLossGateV1 {
    Absent,
    Limited,
    Unlimited,
}

impl CombatAutoHpLossGateV1 {
    pub fn is_explicit_acceptance(self) -> bool {
        !matches!(self, Self::Absent)
    }

    pub fn is_limited(self) -> bool {
        matches!(self, Self::Limited)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatAutoSearchContextV1 {
    pub high_stakes_potion_budget: Option<u32>,
    pub has_usable_potion: bool,
    pub command_wall_ms_set: bool,
    pub session_wall_ms_set: bool,
    pub command_potion_policy_set: bool,
    pub session_potion_policy_set: bool,
    pub command_max_potions_used_set: bool,
    pub session_max_potions_used_set: bool,
    pub hp_loss_gate: CombatAutoHpLossGateV1,
}

impl CombatAutoSearchContextV1 {
    pub fn has_potion_policy_override(self) -> bool {
        self.command_potion_policy_set || self.session_potion_policy_set
    }

    pub fn has_max_potions_override(self) -> bool {
        self.command_max_potions_used_set || self.session_max_potions_used_set
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CombatAutoSearchPlanV1 {
    pub default_wall_ms: Option<u64>,
    pub requires_explicit_hp_loss_gate: bool,
    pub primary_potion_policy: Option<CombatSearchV2PotionPolicy>,
    pub primary_max_potions_used: Option<u32>,
    pub no_potion_first: bool,
    pub potion_rescue_policy: Option<CombatSearchV2PotionPolicy>,
    pub potion_rescue_max_potions_used: Option<u32>,
}
