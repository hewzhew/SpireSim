mod policy;
mod types;

pub use policy::plan_combat_auto_search_v1;
pub use types::{
    CombatAutoHpLossGateV1, CombatAutoSearchContextV1, CombatAutoSearchPlanV1,
    DEFAULT_COMBAT_AUTO_SEARCH_WALL_MS,
};

#[cfg(test)]
mod tests;
