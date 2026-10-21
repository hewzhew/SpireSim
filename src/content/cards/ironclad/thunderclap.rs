use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::ThunderClap,
        name: "Thunderclap",
        card_type: CardType::Attack,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 4,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::AllEnemy,
        is_multi_damage: true,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 3,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn thunderclap_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let mut actions = smallvec::smallvec![ActionInfo {
        action: Action::DamageAllEnemies {
            source: 0,
            damages: evaluated.multi_damage.clone(),
            damage_type: DamageType::Normal,
            is_modified: false,
        },
        insertion_mode: AddTo::Bottom,
    }];
    for monster in &state.entities.monsters {
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: monster.id,
                power_id: crate::content::powers::PowerId::Vulnerable,
                amount: 1,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
