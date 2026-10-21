use sts_oracle_runtime::eval::combat_case::CombatCase;

pub(super) fn combat_case_with_player_hp(case: &CombatCase, hp: i32) -> CombatCase {
    let mut case = case.clone();
    let max_hp = case.core.position.combat.entities.player.max_hp.max(1);
    let hp = hp.clamp(1, max_hp);
    case.core.position.combat.entities.player.current_hp = hp;
    case.refresh_derived_summaries_and_clear_production_context();
    case
}

pub(super) fn counterfactual_hp_targets(
    levels: &str,
    original_hp: i32,
    max_hp: i32,
) -> Vec<(String, i32)> {
    let mut targets = Vec::new();
    for token in levels
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let normalized = token.to_ascii_lowercase();
        let hp = match normalized.as_str() {
            "real" | "original" => Some(original_hp),
            "half" => Some((max_hp + 1) / 2),
            "full" | "max" => Some(max_hp),
            _ => normalized.parse::<i32>().ok(),
        };
        if let Some(hp) = hp {
            let hp = hp.clamp(1, max_hp);
            if !targets.iter().any(|(_, existing_hp)| *existing_hp == hp) {
                targets.push((token.to_string(), hp));
            }
        }
    }
    if targets.is_empty() {
        targets.push(("real".to_string(), original_hp.clamp(1, max_hp)));
    }
    targets
}
