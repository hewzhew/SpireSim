use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Shockwave,
        name: "Shockwave",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 2,
        base_damage: 0,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::AllEnemy,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 2,
    }
}

pub fn shockwave_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let requests = crate::content::cards::mechanics::card_enemy_debuffs_in_monster_order_v1(
        card.id,
        evaluated.base_magic_num_mut,
    )
    .expect("Shockwave debuff sequence");

    for m in &state.entities.monsters {
        for request in &requests {
            actions.push(ActionInfo {
                action: Action::ApplyPower {
                    source: 0,
                    target: m.id,
                    power_id: request.debuff.power_id(),
                    amount: request.amount,
                },
                insertion_mode: AddTo::Bottom,
            });
        }
    }

    actions
}
