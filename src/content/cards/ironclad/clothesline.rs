use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Deal 12 damage (14 if upgraded), then apply 2 Weak (3 if upgraded). The new Weak applies after this hit.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Clothesline,
        name: "Clothesline",
        card_type: CardType::Attack,
        rarity: CardRarity::Common,
        cost: 2,
        base_damage: 12,
        base_block: 0,
        base_magic: 2,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 2,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn clothesline_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<crate::EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Clothesline requires a valid target!");
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

    actions
}
