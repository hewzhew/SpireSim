use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

use crate::content::cards::mechanics::card_self_hp_loss_on_play_v1;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Hemokinesis,
        name: "Hemokinesis",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 15,
        base_block: 0,
        base_magic: 2,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 5,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn hemokinesis_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Hemokinesis requires a valid target!");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let self_hp_loss = card_self_hp_loss_on_play_v1(card.id, card.upgrades)
        .expect("Hemokinesis has direct self HP loss");
    smallvec::smallvec![
        ActionInfo {
            action: Action::LoseHp {
                target: 0,
                amount: self_hp_loss.base_hp_loss,
                triggers_rupture: self_hp_loss.triggers_rupture,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::Damage(DamageInfo {
                source: 0,
                target,
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: DamageType::Normal,
                is_modified: false,
            }),
            insertion_mode: AddTo::Bottom,
        }
    ]
}
