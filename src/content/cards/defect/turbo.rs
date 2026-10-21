use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Turbo,
        name: "Turbo",
        card_type: CardType::Skill,
        rarity: CardRarity::Common,
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
        upgrade_magic: 1,
    }
}

pub fn turbo_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::GainEnergy {
                amount: crate::content::cards::mechanics::card_energy_gain_v1(
                    card.id,
                    card.upgrades,
                    evaluated.base_magic_num_mut,
                )
                .expect("Turbo energy rule")
                .amount,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::MakeTempCardInDiscard {
                card_id: CardId::Void,
                amount: 1,
                upgraded: false,
            },
            insertion_mode: AddTo::Bottom,
        },
    ]
}
