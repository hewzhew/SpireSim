//! Display vocabulary only. Never use these lookups to bind or execute an action.
use sts_oracle_run_control::content::{
    identity::{
        gameplay_display_label_v1, gameplay_identity_from_input_alias_v1, GameplayIdentitySurface,
    },
    powers::{uses_sentinel_amount, PowerId},
};

pub(crate) fn name<T: GameplayIdentitySurface>(id: &str) -> String {
    // Canonical snapshots use normalized stable IDs. Prefer that exact
    // vocabulary before aliases shared by several cards (e.g. starter Strike).
    let canonical = sts_java_parity_protocol::canonical_game_id(id);
    let mut identities = T::STABLE_IDS
        .iter()
        .filter(|s| sts_java_parity_protocol::canonical_game_id(s) == canonical);
    let exact = identities
        .next()
        .filter(|_| identities.next().is_none())
        .and_then(|s| T::from_stable_id(s));
    exact
        .or_else(|| gameplay_identity_from_input_alias_v1::<T>(id))
        .map(|value| gameplay_display_label_v1(value).to_owned())
        .unwrap_or_else(|| id.to_owned())
}

pub(crate) fn power(id: &str, amount: i64) -> String {
    let title = name::<PowerId>(id);
    if gameplay_identity_from_input_alias_v1::<PowerId>(id).is_some_and(uses_sentinel_amount) {
        title
    } else {
        format!("{title} {amount}")
    }
}

pub(crate) fn room(id: &str) -> &str {
    match id {
        "monster" => "Monster",
        "elite" => "Elite",
        "rest" => "Rest Site",
        "shop" => "Shop",
        "treasure" => "Treasure",
        "question_mark" | "event" => "Unknown Room",
        "boss" => "Boss",
        _ => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_oracle_run_control::content::{cards::CardId, monsters::EnemyId, relics::RelicId};

    #[test]
    fn canonical_ids_precede_shared_aliases_and_unknown_names_remain_visible() {
        assert_eq!(name::<CardId>("strike"), "Strike");
        assert_eq!(name::<CardId>("strikeg"), "Strike");
        assert_eq!(name::<EnemyId>("spikeslimem"), "Spike Slime (M)");
        assert_eq!(name::<RelicId>("oddlysmoothstone"), "Oddly Smooth Stone");
        assert_eq!(name::<CardId>("unknown-content"), "unknown-content");
    }

    #[test]
    fn presence_powers_do_not_hide_signed_gameplay_amounts() {
        assert_eq!(power("corruption", -1), "Corruption");
        assert_eq!(power("strength", -2), "Strength -2");
        assert_eq!(power("dexterity", 1), "Dexterity 1");
        assert_eq!(power("unknown-power", -1), "unknown-power -1");
    }
}
