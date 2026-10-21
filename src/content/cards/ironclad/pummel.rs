use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Pummel,
        name: "Pummel",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 2,
        base_block: 0,
        base_magic: 4,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn pummel_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<crate::EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Pummel requires a valid target!");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let mut actions = smallvec::SmallVec::new();
    let damage = evaluated.base_damage_mut;
    let amount = evaluated.base_magic_num_mut;

    for hit in 0..amount {
        let action = if hit + 1 == amount {
            Action::Damage(DamageInfo {
                source: 0,
                target,
                base: damage,
                output: damage,
                damage_type: DamageType::Normal,
                is_modified: false,
            })
        } else {
            Action::PummelDamage(DamageInfo {
                source: 0,
                target,
                base: damage,
                output: damage,
                damage_type: DamageType::Normal,
                is_modified: false,
            })
        };
        actions.push(ActionInfo {
            action,
            insertion_mode: AddTo::Bottom,
        });
    }

    actions
}
