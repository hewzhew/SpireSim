use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Trip,
        name: "Trip",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 2,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn trip_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, target);
    if card.upgrades > 0 {
        state
            .entities
            .monsters
            .iter()
            .map(|monster| ActionInfo {
                action: Action::ApplyPower {
                    source: 0,
                    target: monster.id,
                    power_id: PowerId::Vulnerable,
                    amount: evaluated.base_magic_num_mut,
                },
                insertion_mode: AddTo::Bottom,
            })
            .collect()
    } else {
        let target = target.expect("Trip requires a valid target");
        smallvec::smallvec![ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target,
                power_id: PowerId::Vulnerable,
                amount: evaluated.base_magic_num_mut,
            },
            insertion_mode: AddTo::Bottom,
        }]
    }
}
