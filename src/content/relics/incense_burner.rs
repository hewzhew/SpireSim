use crate::runtime::action::{Action, ActionInfo, AddTo};
use smallvec::SmallVec;

pub const MECHANICS: &str = "On pickup, the turn counter starts at 0. At the start of each player turn, increment it by 1; at 6, reset it to 0 and gain 1 Intangible. The counter persists between combats, so the next fight's protection timing depends on the previous fight's ending counter. The displayed counter counts elapsed turn starts since the last trigger.";

/// One-based turn in the next combat for a normal persistent counter.
pub const fn next_combat_trigger_turn(counter: i32) -> Option<u32> {
    if counter >= 0 && counter < 6 {
        Some((6 - counter) as u32)
    } else {
        None
    }
}

/// Incense Burner: Every 6 turns, gain 1 Intangible.
/// Counter persists across combats.
/// Java: atTurnStart() → counter = (counter == -1) ? counter + 2 : ++counter;
///   if (counter == 6) { counter = 0; addToBot(ApplyPowerAction(IntangiblePlayerPower, 1)) }
/// Java: onEquip() → counter = 0
pub fn at_turn_start(
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();

    relic_state.counter = if relic_state.counter == -1 {
        relic_state.counter + 2
    } else {
        relic_state.counter + 1
    };

    if relic_state.counter == 6 {
        relic_state.counter = 0;
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::IntangiblePlayer,
                amount: 1,
            },
            insertion_mode: AddTo::Bottom, // Java: addToBot
        });
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::relics::{RelicId, RelicState};

    #[test]
    fn projected_next_combat_turn_matches_real_intangible_trigger() {
        for counter in 0..6 {
            let predicted = next_combat_trigger_turn(counter).unwrap();
            let mut relic = RelicState::new(RelicId::IncenseBurner);
            relic.counter = counter;
            for turn in 1..=predicted {
                let actions = at_turn_start(&mut relic);
                if turn < predicted {
                    assert!(actions.is_empty());
                } else {
                    assert!(matches!(
                        actions.as_slice(),
                        [ActionInfo {
                            action: Action::ApplyPower {
                                power_id: crate::content::powers::PowerId::IntangiblePlayer,
                                amount: 1,
                                ..
                            },
                            ..
                        }]
                    ));
                    assert_eq!(relic.counter, 0);
                }
            }
        }
        for counter in [-2, -1, 6, i32::MAX] {
            assert_eq!(next_combat_trigger_turn(counter), None);
        }
    }
}
