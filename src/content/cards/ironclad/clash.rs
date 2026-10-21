use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Clash,
        name: "Clash",
        card_type: CardType::Attack,
        rarity: CardRarity::Common,
        cost: 0,
        base_damage: 14,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 4,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn clash_play(
    _state: &CombatState,
    card: &CombatCard,
    target: Option<crate::EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Clash requires a valid target!");
    let mut actions = smallvec::SmallVec::new();

    let evaluated = crate::content::cards::evaluate_card_for_play(card, _state, Some(target));
    let damage = evaluated.base_damage_mut;

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

    actions
}
