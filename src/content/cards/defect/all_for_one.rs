use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::AllForOne,
        name: "All For One",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 10,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 4,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn all_for_one_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<usize>,
) -> SmallVec<[ActionInfo; 4]> {
    let Some(target) = target else {
        return smallvec::smallvec![];
    };
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    smallvec::smallvec![
        ActionInfo {
            action: Action::Damage(DamageInfo {
                source: 0,
                target,
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: DamageType::Normal,
                is_modified: true,
            }),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::AllCostToHand { cost_target: 0 },
            insertion_mode: AddTo::Bottom,
        },
    ]
}
