use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::RecklessCharge,
        name: "Reckless Charge",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 7,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 3,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn reckless_charge_play(
    _state: &CombatState,
    card: &CombatCard,
    target: Option<crate::EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Reckless Charge requires a valid target!");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, _state, Some(target));
    smallvec::smallvec![
        ActionInfo {
            action: Action::Damage(crate::runtime::action::DamageInfo {
                source: 0,
                target,
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: crate::runtime::action::DamageType::Normal,
                is_modified: false,
            }),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: crate::content::cards::mechanics::card_status_creation_on_play_v1(card.id)
                .expect("Reckless Charge status creation rule")
                .action(_state),
            insertion_mode: AddTo::Bottom,
        }
    ]
}
