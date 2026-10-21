use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::DramaticEntrance,
        name: "Dramatic Entrance",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 8,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::AllEnemy,
        is_multi_damage: true,
        exhaust: true,
        ethereal: false,
        innate: true,
        tags: &[],
        upgrade_damage: 4,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn dramatic_entrance_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![ActionInfo {
        action: Action::DamageAllEnemies {
            source: 0,
            damages: evaluated.multi_damage.clone(),
            damage_type: DamageType::Normal,
            is_modified: evaluated.base_damage_mut
                != crate::content::cards::get_card_definition(card.id).base_damage,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
