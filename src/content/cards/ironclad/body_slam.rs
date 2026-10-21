use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str =
    "Use current Block as base Attack damage, then apply Strength, Weak, the target's Vulnerable and other damage modifiers. Playing Body Slam preserves your Block. Costs 1 Energy (0 if upgraded).";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::BodySlam,
        name: "Body Slam",
        card_type: CardType::Attack,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
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

pub fn body_slam_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<crate::EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Body Slam requires a valid target!");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let mut actions = smallvec::SmallVec::new();
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
