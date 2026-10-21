use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

use crate::content::cards::mechanics::card_self_hp_loss_on_play_v1;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Offering,
        name: "Offering",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 2,
    }
}

pub fn offering_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let self_hp_loss = card_self_hp_loss_on_play_v1(card.id, card.upgrades)
        .expect("Offering has direct self HP loss");
    smallvec::smallvec![
        ActionInfo {
            action: Action::LoseHp {
                target: 0,
                amount: self_hp_loss.base_hp_loss,
                triggers_rupture: self_hp_loss.triggers_rupture,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::GainEnergy {
                amount: crate::content::cards::mechanics::card_energy_gain_v1(
                    card.id,
                    card.upgrades,
                    evaluated.base_magic_num_mut,
                )
                .expect("Offering energy rule")
                .amount,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::DrawCards(evaluated.base_magic_num_mut as u32),
            insertion_mode: AddTo::Bottom,
        }
    ]
}
