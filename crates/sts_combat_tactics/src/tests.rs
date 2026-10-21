use super::*;

#[test]
fn selection_preferences_are_bounded_even_for_an_infinite_source() {
    let preferences = CombatSelectionPreferences::new(std::iter::repeat(ClientInput::Cancel));
    assert_eq!(
        preferences.members().len(),
        CombatSelectionPreferences::LIMIT
    );
}

#[test]
fn retained_semantic_lane_ids_are_stable_and_distinct() {
    assert_eq!(
        [
            COMBAT_PROGRESS_GUIDE_LANE_V1.value(),
            COMBAT_SURVIVAL_GUIDE_LANE_V1.value(),
            COMBAT_HORIZON_GUIDE_LANE_V1.value(),
            COMBAT_SETUP_GUIDE_LANE_V1.value(),
            COMBAT_TURN_DEPTH_GUIDE_LANE_V1.value(),
            COMBAT_FEED_PAYOFF_GUIDE_LANE_V1.value(),
        ],
        [1, 2, 3, 4, 5, 6]
    );
}
