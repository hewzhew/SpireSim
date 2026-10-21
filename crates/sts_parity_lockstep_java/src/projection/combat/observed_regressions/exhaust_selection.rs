use super::super::*;
use crate::{
    action::JavaHandSelectProfileV1::SingleExhaust,
    observed_regression_fixtures::{load_from, observation_from, validate_adjacent},
    RecordedJavaActionV1,
};
use sts_parity_lockstep::SemanticActionV1;

#[test]
fn recorded_burning_pact_preserves_combat_across_the_exhaust_choice() {
    let before = observation_from("burning_pact_selection", "before.json");
    let pending = observation_from("burning_pact_selection", "pending.json");
    let after = observation_from("burning_pact_selection", "after.json");
    let play: RecordedJavaActionV1 =
        serde_json::from_value(load_from("burning_pact_selection", "play.json")).unwrap();
    let select: RecordedJavaActionV1 =
        serde_json::from_value(load_from("burning_pact_selection", "select.json")).unwrap();
    assert!(validate_adjacent(&before, &play, &pending));
    assert!(validate_adjacent(&pending, &select, &after));
    assert!(
        matches!(play.action, SemanticActionV1::PlayCard { ref card_id, .. } if card_id == "burningpact")
    );
    assert!(matches!(
        select.action,
        SemanticActionV1::SubmitSelection { .. }
    ));
    assert_eq!(
        pending.frame["public_base"]["decision"]["hand_select"]["selection_reason"],
        "Exhaust"
    );

    // Aliases track the same Java objects across these frames only. They do
    // not bind Rust cards; full cross-runtime identity is tested by lockstep.
    let mut aliases = BTreeMap::new();
    for zone in ["draw", "hand", "discard", "exhaust", "limbo"] {
        for card in before.frame["parity_internal"]["zones"][zone]["cards"]
            .as_array()
            .unwrap()
        {
            let uuid = required_str(card, "instance_uuid").unwrap();
            assert!(aliases
                .insert(uuid.to_owned(), format!("java-object/{uuid}"))
                .is_none());
        }
    }
    let project = |raw, kind, profile| {
        let NaturalRunCombatV4::PlayerDecision { state } =
            project_combat_v4(raw, Some(&aliases), kind, profile).unwrap()
        else {
            panic!("selection must retain its active combat");
        };
        state
    };
    let start = project(&before, "natural_combat", None);
    let choice = project(&pending, "natural_hand_select", Some(SingleExhaust));
    let resolved = project(&after, "natural_combat", None);
    assert_eq!(choice.turn, start.turn);
    assert_eq!(choice.player.energy + 1, start.player.energy);
    assert_eq!(choice.player.current_hp, start.player.current_hp);
    assert_eq!(choice.player.block, start.player.block);
    assert_eq!(choice.monsters, start.monsters);
    assert_eq!(choice.player_powers, start.player_powers);
    assert_eq!(choice.zones.draw_next_first, start.zones.draw_next_first);
    assert_eq!(
        choice.zones.exhaust_by_binding,
        start.zones.exhaust_by_binding
    );
    assert_eq!(
        choice.zones.discard_bottom_first,
        start.zones.discard_bottom_first
    );
    assert_eq!(choice.zones.limbo.len(), 1);
    let played = &choice.zones.limbo[0];
    assert_eq!(played.card_id, "burningpact");
    assert_eq!(
        choice.zones.hand,
        start
            .zones
            .hand
            .iter()
            .filter(|c| c.binding != played.binding)
            .cloned()
            .collect::<Vec<_>>()
    );

    assert_eq!(resolved.player, choice.player);
    assert_eq!(resolved.monsters, choice.monsters);
    assert_eq!(resolved.zones.exhaust_by_binding.len(), 1);
    let exhausted = &resolved.zones.exhaust_by_binding[0];
    assert!(choice.zones.hand.contains(exhausted));
    let mut expected_hand = choice
        .zones
        .hand
        .iter()
        .filter(|c| c.binding != exhausted.binding)
        .cloned()
        .collect::<Vec<_>>();
    expected_hand.extend_from_slice(&choice.zones.draw_next_first[..2]);
    assert_eq!(resolved.zones.hand, expected_hand);
    assert_eq!(
        resolved.zones.draw_next_first,
        choice.zones.draw_next_first[2..]
    );
    assert_eq!(resolved.zones.discard_bottom_first, vec![played.clone()]);
    assert!(resolved.zones.limbo.is_empty());
}
