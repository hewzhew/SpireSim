use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Catalyst,
        name: "Catalyst",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 2,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn catalyst_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Catalyst requires a valid target");
    let poison = state.get_power(target, PowerId::Poison).max(0);
    if poison == 0 {
        return smallvec::smallvec![];
    }

    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let extra = poison * (evaluated.base_magic_num_mut - 1).max(1);
    smallvec::smallvec![ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target,
            power_id: PowerId::Poison,
            amount: extra,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
