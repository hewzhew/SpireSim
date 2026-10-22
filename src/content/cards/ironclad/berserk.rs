use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Berserk,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Berserk,
        name: "Berserk",
        card_type: CardType::Power,
        rarity: CardRarity::Rare,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 2,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: -1,
    }
}

pub fn berserk_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::Vulnerable,
                amount: evaluated.base_magic_num_mut,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: crate::content::powers::PowerId::Berserk,
                amount: crate::content::cards::mechanics::card_energy_gain_v1(
                    card.id,
                    card.upgrades,
                    evaluated.base_magic_num_mut,
                )
                .expect("Berserk energy rule")
                .amount,
            },
            insertion_mode: AddTo::Bottom,
        }
    ]
}
