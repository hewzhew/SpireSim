use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Uppercut,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Uppercut,
        name: "Uppercut",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 2,
        base_damage: 13,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::Enemy,
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

pub fn uppercut_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<crate::EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Uppercut requires a valid target!");
    let mut actions = smallvec::SmallVec::new();
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let damage = evaluated.base_damage_mut;
    let amount = evaluated.base_magic_num_mut;

    actions.push(ActionInfo {
        action: Action::Damage(DamageInfo {
            source: 0,
            target,
            base: damage,
            output: damage,
            damage_type: DamageType::Normal,
            is_modified: false,
        }),
        insertion_mode: AddTo::Bottom,
    });

    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target,
            power_id: PowerId::Weak,
            amount,
        },
        insertion_mode: AddTo::Bottom,
    });

    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target,
            power_id: PowerId::Vulnerable,
            amount,
        },
        insertion_mode: AddTo::Bottom,
    });

    actions
}
