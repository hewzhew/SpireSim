use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::SwordBoomerang,
        name: "Sword Boomerang",
        card_type: CardType::Attack,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 3,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::AllEnemy,
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

pub fn sword_boomerang_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = SmallVec::new();
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);

    for _ in 0..evaluated.base_magic_num_mut {
        actions.push(ActionInfo {
            action: Action::AttackDamageRandomEnemyCard {
                card: Box::new(card.clone()),
            },
            insertion_mode: AddTo::Bottom,
        });
    }

    actions
}
