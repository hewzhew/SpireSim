use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

use crate::content::cards::mechanics::card_self_hp_loss_on_play_v1;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::JAX,
        name: "J.A.X.",
        card_type: CardType::Skill,
        rarity: CardRarity::Special,
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

pub fn jax_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let self_hp_loss = card_self_hp_loss_on_play_v1(card.id, card.upgrades)
        .expect("J.A.X. has direct self HP loss");
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
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Strength,
                amount: evaluated.base_magic_num_mut,
            },
            insertion_mode: AddTo::Bottom,
        }
    ]
}
