use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTag, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub const MECHANICS: &str = "Deal 7 damage to one enemy, then heal 2 HP (8 damage and 3 HP if upgraded). Healing is a fixed amount per play, subject to healing modifiers and the Max HP cap.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Bite,
        name: "Bite",
        card_type: CardType::Attack,
        rarity: CardRarity::Special,
        cost: 1,
        base_damage: 7,
        base_block: 0,
        base_magic: 2,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[CardTag::Healing],
        upgrade_damage: 1,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn bite_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Bite requires a valid target");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let def = crate::content::cards::get_card_definition(card.id);
    smallvec::smallvec![
        ActionInfo {
            action: Action::Damage(DamageInfo {
                source: 0,
                target,
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: DamageType::Normal,
                is_modified: evaluated.base_damage_mut != def.base_damage,
            }),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::Heal {
                target: 0,
                amount: evaluated.base_magic_num_mut,
            },
            insertion_mode: AddTo::Bottom,
        }
    ]
}
