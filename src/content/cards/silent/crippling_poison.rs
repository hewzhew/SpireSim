use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::CripplingPoison,
        name: "Crippling Poison",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 2,
        base_damage: 0,
        base_block: 0,
        base_magic: 4,
        target: CardTarget::AllEnemy,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 3,
    }
}

pub fn crippling_poison_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let mut actions = SmallVec::new();

    if state.are_monsters_basically_dead_java() {
        return actions;
    }

    for monster in state.entities.monsters.iter().filter(|monster| {
        !monster.is_dying && !monster.is_escaped && !monster.half_dead && monster.current_hp > 0
    }) {
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: monster.id,
                power_id: PowerId::Poison,
                amount: evaluated.base_magic_num_mut,
            },
            insertion_mode: AddTo::Bottom,
        });
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: monster.id,
                power_id: PowerId::Weak,
                amount: 2,
            },
            insertion_mode: AddTo::Bottom,
        });
    }

    actions
}
