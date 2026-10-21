/// Current exact combat-case wire contract.
///
/// This constant lives on the lightweight simulator surface because both the
/// full oracle runtime and combat-only tools decode the same `CombatPosition`
/// envelope. Artifact implementations may re-export it, but must not copy the
/// wire value into private loaders.
pub const COMBAT_CASE_SCHEMA: &str = "combat_case_v2";
