use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Combust,
        name: "Combust",
        card_type: CardType::Power,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 5,
        target: CardTarget::SelfTarget,
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

pub fn combust_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let effect = crate::content::cards::mechanics::card_end_of_turn_hp_loss_and_damage_v1(
        card.id,
        evaluated.base_magic_num_mut,
    )
    .expect("Combust follow-up rule");
    smallvec::smallvec![ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Combust,
            amount: effect.damage_per_enemy_per_application,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
