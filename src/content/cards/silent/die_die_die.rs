use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::DieDieDie,
        name: "Die Die Die",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 1,
        base_damage: 13,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::AllEnemy,
        is_multi_damage: true,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 4,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn die_die_die_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::DamageAllEnemies {
            source: 0,
            damages: evaluated.multi_damage.clone(),
            damage_type: DamageType::Normal,
            is_modified: true,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
