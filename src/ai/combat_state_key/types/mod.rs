mod combat;
mod pending_choice;
mod postcombat;
mod reward;
mod shop;
mod stable;

pub(crate) use combat::*;
pub use combat::{CombatDominanceKey, CombatExactStateKey};
pub(crate) use pending_choice::{
    StableBossRelicKey, StableChoiceCandidateKey, StablePendingChoiceKey,
    StableRunPendingChoiceKey, StableRunPendingReturnKey, StableTreasureChestKey,
};
pub(crate) use postcombat::{
    StableMetaChangeKey, StableMetaKey, StablePostcombatPlayerKey, StablePostcombatRuntimeKey,
};
pub(crate) use reward::{StableRewardCardKey, StableRewardItemKey, StableRewardKey};
pub(crate) use shop::{StableShopKey, StableShopRowKey};
pub use stable::StableOutcomeKey;
pub(crate) use stable::{
    StableCombatPlayerKey, StableEngineKey, StableOutcomePayload, StableTurnKey, StableZonesKey,
};
trait DiagnosticKey {
    fn diagnostic_string(&self) -> String;
}

impl DiagnosticKey for StableMetaChangeKey {
    fn diagnostic_string(&self) -> String {
        StableMetaChangeKey::diagnostic_string(self)
    }
}

impl DiagnosticKey for StableRewardItemKey {
    fn diagnostic_string(&self) -> String {
        StableRewardItemKey::diagnostic_string(self)
    }
}

impl DiagnosticKey for StableRewardCardKey {
    fn diagnostic_string(&self) -> String {
        StableRewardCardKey::diagnostic_string(self)
    }
}

impl DiagnosticKey for StableShopRowKey {
    fn diagnostic_string(&self) -> String {
        StableShopRowKey::diagnostic_string(self)
    }
}

impl DiagnosticKey for StableChoiceCandidateKey {
    fn diagnostic_string(&self) -> String {
        StableChoiceCandidateKey::diagnostic_string(self)
    }
}

fn join_diagnostic_strings<T: DiagnosticKey>(values: &[T]) -> String {
    values
        .iter()
        .map(DiagnosticKey::diagnostic_string)
        .collect::<Vec<_>>()
        .join("|")
}
