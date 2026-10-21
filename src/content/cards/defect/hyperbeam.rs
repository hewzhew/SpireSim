use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Hyperbeam,
        name: "Hyperbeam",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 26,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::AllEnemy,
        is_multi_damage: true,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 8,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn hyperbeam_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::DamageAllEnemies {
                source: 0,
                damages: evaluated.multi_damage.clone(),
                damage_type: DamageType::Normal,
                is_modified: false,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Focus,
                amount: -evaluated.base_magic_num_mut,
            },
            insertion_mode: AddTo::Bottom,
        },
    ]
}
