use crate::runtime::action::{Action, ActionInfo, AddTo, NO_SOURCE};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Every third Skill played in the same turn queues 5 damage to ALL enemies and resets the Skill counter to 0. The counter also resets at the start of each turn. Attacks and Powers do not increment it. This is relic damage, not another card play, so it does not trigger Beat of Death again.";

/// LetterOpener: Every time you play 3 Skills in a single turn, deal 5 damage to ALL enemies.
/// Java: onUseCard() → ++counter; if counter % 3 == 0: counter=0, addToBot(DamageAllEnemiesAction(5, THORNS))
pub fn at_turn_start(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = 0;
}

pub fn on_use_card(
    state: &crate::runtime::combat::CombatState,
    card_id: crate::content::cards::CardId,
    relic_state: &mut crate::content::relics::RelicState,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    let def = crate::content::cards::get_card_definition(card_id);

    if def.card_type == crate::content::cards::CardType::Skill {
        let current = relic_state.counter.max(0);
        let next_counter = if current + 1 >= 3 { 0 } else { current + 1 };
        relic_state.counter = next_counter;

        if next_counter == 0 {
            // Java: addToBot(new DamageAllEnemiesAction(null, DamageInfo.createDamageMatrix(5, true), THORNS))
            let damages: smallvec::SmallVec<[i32; 5]> =
                state.entities.monsters.iter().map(|_| 5i32).collect();
            actions.push(ActionInfo {
                action: Action::DamageAllEnemies {
                    source: NO_SOURCE,
                    damages,
                    damage_type: crate::runtime::action::DamageType::Thorns,
                    is_modified: false,
                },
                insertion_mode: AddTo::Bottom, // Java: addToBot
            });
        }
    }

    actions
}

pub fn on_victory(relic_state: &mut crate::content::relics::RelicState) {
    relic_state.counter = -1;
}
