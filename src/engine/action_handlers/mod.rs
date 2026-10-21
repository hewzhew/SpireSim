//! action_handlers — Unified action executor with domain-split sub-modules.
//!
//! Sub-modules:
//!   - dispatcher: Thin Action enum dispatcher.
//!   - death:      Revive and monster death boundary handling.
//!   - damage:     Combat damage pipeline (Damage, FiendFire, Feed, Vampire, etc.).
//!   - cards:      Card pile management (Draw, Exhaust, MakeTemp, PlayCardDirect, etc.).
//!   - powers:     Power lifecycle (ApplyPower, RemovePower, Artifact, Stasis, etc.).
//!   - spawning:   Monster lifecycle (Spawn, Escape, Suicide, RollMove, relics, etc.).
//!   - orbs:       Orb slot/channel/fission actions.
//!   - stances:    Stance changes and stance-adjacent Watcher actions.

pub mod cards;
pub mod damage;
mod death;
mod dispatcher;
mod orbs;
pub mod powers;
pub mod spawning;
mod stances;

pub use death::{
    available_revival_requests, check_and_trigger_monster_death, try_revive, RevivalRequest,
    RevivalSource,
};
pub use dispatcher::execute_action;
pub(crate) use dispatcher::execute_action_with_profiler;

#[cfg(test)]
mod tests {
    use super::execute_action;
    use super::stances::handle_enter_stance;
    use crate::content::cards::CardId;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::powers::PowerId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::action::Action;
    use crate::runtime::combat::{CombatPhase, Power, PowerPayload, StanceId};
    use crate::test_support::blank_test_combat;

    #[test]
    fn cannot_change_stance_power_blocks_change_stance_action() {
        let mut state = blank_test_combat();
        state.entities.player.stance = StanceId::Calm;
        state.turn.energy = 0;
        state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::CannotChangeStance,
                instance_id: None,
                amount: -1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        handle_enter_stance("Wrath", &mut state);

        assert_eq!(state.entities.player.stance, StanceId::Calm);
        assert_eq!(
            state.turn.energy, 0,
            "Java ChangeStanceAction returns before oldStance.onExitStance when CannotChangeStancePower is present"
        );
    }

    #[test]
    fn queued_discard_potion_action_respects_java_can_discard_affordance() {
        let mut state = blank_test_combat();
        state.entities.potions = vec![
            Some(Potion::with_affordance_truth(
                PotionId::FirePotion,
                101,
                false,
                false,
                false,
            )),
            Some(Potion::with_affordance_truth(
                PotionId::BlockPotion,
                102,
                false,
                true,
                false,
            )),
        ];

        execute_action(Action::DiscardPotion { slot: 0 }, &mut state);
        assert!(
            state.entities.potions[0].is_some(),
            "Java PotionPopUp checks potion.canDiscard before destroying the slot"
        );

        execute_action(Action::DiscardPotion { slot: 1 }, &mut state);
        assert!(state.entities.potions[1].is_none());
    }

    #[test]
    fn time_warp_trigger_resets_counter_ends_turn_and_grants_monster_strength() {
        let mut state = blank_test_combat();
        let mut eater =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::TimeEater);
        eater.id = 1;
        state.entities.monsters = vec![eater];
        state.entities.power_db.insert(
            1,
            vec![Power {
                power_type: PowerId::TimeWarp,
                instance_id: None,
                amount: 12,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        execute_action(Action::TriggerTimeWarpEndTurn { owner: 1 }, &mut state);

        assert_eq!(
            crate::content::powers::store::power_amount(&state, 1, PowerId::TimeWarp),
            0
        );
        assert!(state.turn.counters.early_end_turn_pending);
        assert_eq!(
            state.pop_next_action(),
            Some(Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::Strength,
                amount: 2,
            })
        );
        execute_action(
            Action::ApplyPower {
                source: 1,
                target: 1,
                power_id: PowerId::Strength,
                amount: 2,
            },
            &mut state,
        );
        execute_action(
            Action::UseCardDone {
                should_exhaust: false,
                trigger_after_use_hooks: false,
            },
            &mut state,
        );

        assert_eq!(
            crate::content::powers::store::power_amount(&state, 1, PowerId::Strength),
            2
        );
        assert_eq!(state.turn.current_phase, CombatPhase::TurnTransition);
        assert_eq!(state.pop_next_action(), Some(Action::EndTurnTrigger));
    }

    #[test]
    fn stance_energy_is_queued_in_java_change_stance_order() {
        let mut state = blank_test_combat();
        state.entities.player.stance = StanceId::Calm;
        state.turn.energy = 0;
        state
            .entities
            .player
            .add_relic(RelicState::new(RelicId::VioletLotus));
        state.zones.discard_pile = vec![crate::runtime::combat::CombatCard::for_scenario(
            CardId::FlurryOfBlows,
            91001,
        )]
        .into();
        state.entities.power_db.insert(
            0,
            vec![Power {
                power_type: PowerId::RushdownPower,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            }],
        );

        handle_enter_stance("Wrath", &mut state);

        assert_eq!(state.entities.player.stance, StanceId::Wrath);
        assert_eq!(
            state.turn.energy, 0,
            "Java CalmStance.onExitStance queues GainEnergyAction instead of mutating energy immediately"
        );
        assert_eq!(state.pop_next_action(), Some(Action::DrawCards(2)));
        assert_eq!(
            state.pop_next_action(),
            Some(Action::GainEnergy { amount: 1 })
        );
        assert_eq!(
            state.pop_next_action(),
            Some(Action::GainEnergy { amount: 2 })
        );
        assert_eq!(
            state.pop_next_action(),
            Some(Action::DiscardToHand {
                card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(91001),
                cost_for_turn: None,
            })
        );
        assert!(state.pop_next_action().is_none());
    }

    #[test]
    fn divinity_enter_energy_is_queued_after_stance_changes() {
        let mut state = blank_test_combat();
        state.turn.energy = 0;

        handle_enter_stance("Divinity", &mut state);

        assert_eq!(state.entities.player.stance, StanceId::Divinity);
        assert_eq!(
            state.turn.energy, 0,
            "Java DivinityStance.onEnterStance queues GainEnergyAction instead of mutating energy immediately"
        );
        assert_eq!(
            state.pop_next_action(),
            Some(Action::GainEnergy { amount: 3 })
        );
        assert!(state.pop_next_action().is_none());
    }
}
