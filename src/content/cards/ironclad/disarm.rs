use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Reduce one enemy's current Strength by 2 (3 if upgraded). Strength can become negative: a target at 0 becomes -2 (-3 if upgraded). This persistent reduction affects each later attack hit, including across turns; ordinary Strength gains add to the reduced amount. An effect that explicitly clears debuffs can remove negative Strength. Artifact can block the application. Exhaust.";

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
