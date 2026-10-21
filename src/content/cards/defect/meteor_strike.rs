use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTag, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState, OrbId};
use crate::EntityId;
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::MeteorStrike,
        name: "Meteor Strike",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 5,
        base_damage: 24,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[CardTag::Strike],
        upgrade_damage: 6,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn meteor_strike_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Meteor Strike requires a valid target");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let mut actions = SmallVec::new();
    actions.push(ActionInfo {
        action: Action::Damage(DamageInfo {
            source: 0,
            target,
            base: evaluated.base_damage_mut,
            output: evaluated.base_damage_mut,
            damage_type: DamageType::Normal,
            is_modified: true,
        }),
        insertion_mode: AddTo::Bottom,
    });
    for _ in 0..evaluated.base_magic_num_mut.max(0) {
        actions.push(ActionInfo {
            action: Action::ChannelOrb(OrbId::Plasma),
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
