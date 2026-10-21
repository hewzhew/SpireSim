use std::collections::HashMap;
use std::hash::Hash;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ai::combat_search_v2) struct ResourceVector {
    pub(in crate::ai::combat_search_v2) hp: i32,
    pub(in crate::ai::combat_search_v2) block: i32,
    pub(in crate::ai::combat_search_v2) potions_used: u32,
    pub(in crate::ai::combat_search_v2) potions_discarded: u32,
    pub(in crate::ai::combat_search_v2) cards_played: u32,
    pub(in crate::ai::combat_search_v2) action_count: usize,
}

pub(in crate::ai::combat_search_v2) fn is_resource_covered<K: Eq + Hash>(
    table: &mut HashMap<K, Vec<ResourceVector>>,
    key: K,
    candidate: ResourceVector,
) -> bool {
    let bucket = table.entry(key).or_default();
    if bucket.iter().any(|existing| existing.covers(candidate)) {
        return true;
    }
    bucket.retain(|existing| !candidate.covers(*existing));
    bucket.push(candidate);
    false
}

impl ResourceVector {
    pub(in crate::ai::combat_search_v2) fn diagnostic_parts(self) -> ResourceVectorDiagnosticParts {
        ResourceVectorDiagnosticParts {
            hp: self.hp,
            block: self.block,
            potions_used: self.potions_used,
            potions_discarded: self.potions_discarded,
            cards_played: self.cards_played,
            action_count: self.action_count,
        }
    }

    fn covers(self, other: ResourceVector) -> bool {
        self.hp >= other.hp
            && self.block >= other.block
            && self.potions_used <= other.potions_used
            && self.potions_discarded <= other.potions_discarded
            && self.cards_played <= other.cards_played
            && self.action_count <= other.action_count
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) struct ResourceVectorDiagnosticParts {
    pub(in crate::ai::combat_search_v2) hp: i32,
    pub(in crate::ai::combat_search_v2) block: i32,
    pub(in crate::ai::combat_search_v2) potions_used: u32,
    pub(in crate::ai::combat_search_v2) potions_discarded: u32,
    pub(in crate::ai::combat_search_v2) cards_played: u32,
    pub(in crate::ai::combat_search_v2) action_count: usize,
}
