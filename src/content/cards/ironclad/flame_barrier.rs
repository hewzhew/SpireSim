use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::FlameBarrier,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::FlameBarrier,
        name: "Flame Barrier",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 2,
        base_damage: 0,
        base_block: 12,
        base_magic: 4,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 4,
        upgrade_magic: 2,
    }
}

pub fn flame_barrier_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: evaluated.base_block_mut as i32
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::FlameBarrier,
                amount: evaluated.base_magic_num_mut as i32,
            },
            insertion_mode: AddTo::Bottom,
        }
    ]
}
