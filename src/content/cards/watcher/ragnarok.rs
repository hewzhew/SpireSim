use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Ragnarok,
        name: "Ragnarok",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 3,
        base_damage: 5,
        base_block: 0,
        base_magic: 5,
        target: CardTarget::AllEnemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 1,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn ragnarok_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let mut actions = SmallVec::new();
    for _ in 0..evaluated.base_magic_num_mut.max(0) {
        actions.push(ActionInfo {
            action: Action::AttackDamageRandomEnemyCard {
                card: Box::new(card.clone()),
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
