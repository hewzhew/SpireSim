use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::DeadlyPoison,
        name: "Deadly Poison",
        card_type: CardType::Skill,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 5,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 2,
    }
}

pub fn deadly_poison_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Deadly Poison requires a valid target");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    smallvec::smallvec![ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target,
            power_id: crate::content::powers::PowerId::Poison,
            amount: evaluated.base_magic_num_mut,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
