use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Disarm,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Disarm,
        name: "Disarm",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 2,
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

pub fn disarm_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<crate::EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Disarm requires a valid target!");
    let mut actions = smallvec::SmallVec::new();
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let amount = evaluated.base_magic_num_mut;

    actions.push(ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target,
            power_id: PowerId::Strength,
            amount: -amount, // Reduces strength
        },
        insertion_mode: AddTo::Bottom,
    });

    actions
}
