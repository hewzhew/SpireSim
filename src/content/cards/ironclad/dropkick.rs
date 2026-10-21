use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub const MECHANICS: &str = "Deal 5 damage (8 if upgraded). When the attack resolves, check whether its target has Vulnerable before damage; if so, queue damage, gain 1 Energy, then draw 1 card. Energy and draw are conditional, not guaranteed on play.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Dropkick,
        name: "Dropkick",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 5,
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

pub fn dropkick_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Dropkick requires a valid target!");
    let mut actions = SmallVec::new();
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));

    // The correct Java behavior: DropkickAction checks Vulnerable AT EXECUTION TIME, not at play time.
    // We defer the logic to Action::DropkickDamageAndEffect.
    actions.push(ActionInfo {
        action: Action::DropkickDamageAndEffect {
            target,
            damage_info: DamageInfo {
                source: 0,
                target,
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: DamageType::Normal,
                is_modified: false,
            },
        },
        insertion_mode: AddTo::Bottom,
    });

    actions
}
