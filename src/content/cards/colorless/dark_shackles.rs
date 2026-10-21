use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::DarkShackles,
        name: "Dark Shackles",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 9,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 6,
    }
}

pub fn dark_shackles_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Dark Shackles requires a valid target");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let amount = evaluated.base_magic_num_mut;
    let mut actions = smallvec::smallvec![ActionInfo {
        action: Action::ApplyPower {
            source: 0,
            target,
            power_id: PowerId::Strength,
            amount: -amount,
        },
        insertion_mode: AddTo::Bottom,
    }];
    if !crate::content::powers::store::has_power(state, target, PowerId::Artifact) {
        actions.push(ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target,
                power_id: PowerId::Shackled,
                amount,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
