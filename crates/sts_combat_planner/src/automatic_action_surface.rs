use sts_core::sim::combat::{CombatPosition, CombatStepper};
use sts_core::sim::combat_action_equivalence::canonical_combat_action_representatives_v1;
use sts_core::sim::combat_action_surface::CombatLegalActionSurfaceV2;

/// The single planner-side projection from exact engine legality to actions
/// worth materializing in automatic search or proposal producers.
pub(crate) fn automatic_action_surface(
    stepper: &dyn CombatStepper,
    position: &CombatPosition,
) -> CombatLegalActionSurfaceV2 {
    let mut surface = stepper.legal_action_surface(position);
    surface.apply_automatic_action_admission(&position.combat);
    let representatives = canonical_combat_action_representatives_v1(
        &position.engine,
        &position.combat,
        &surface.atomic_actions,
    );
    surface.atomic_actions = surface
        .atomic_actions
        .into_iter()
        .enumerate()
        .filter_map(|(ordinal, input)| (representatives[ordinal] == ordinal).then_some(input))
        .collect();
    surface
}

#[cfg(test)]
mod tests {
    use sts_core::content::{cards::CardId, monsters::EnemyId};
    use sts_core::runtime::combat::CombatCard;
    use sts_core::sim::combat::EngineCombatStepper;
    use sts_core::state::core::{ClientInput, EngineState};

    use super::*;

    #[test]
    fn automatic_surface_preserves_distinct_starter_instances() {
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.entities.monsters =
            vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
        let mut free_defend = CombatCard::for_scenario(CardId::Defend, 12);
        free_defend.free_to_play_once = true;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
            free_defend,
        ];
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, combat);

        let surface = automatic_action_surface(&EngineCombatStepper, &position);
        let card_indices = surface
            .atomic_actions
            .iter()
            .filter_map(|input| match input {
                ClientInput::PlayCard { card_index, .. } => Some(*card_index),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(card_indices, vec![0, 1, 2]);
    }
}
