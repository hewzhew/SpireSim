use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub const MECHANICS: &str =
    "Deal 7 damage to one enemy (10 if upgraded). Costs 0 Energy and does not Exhaust.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::SwiftStrike,
        name: "Swift Strike",
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

pub fn swift_strike_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Swift Strike requires a valid target");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    let def = crate::content::cards::get_card_definition(card.id);
    smallvec::smallvec![ActionInfo {
        action: Action::Damage(DamageInfo {
            source: 0,
            target,
            base: evaluated.base_damage_mut,
            output: evaluated.base_damage_mut,
            damage_type: DamageType::Normal,
            is_modified: evaluated.base_damage_mut != def.base_damage,
        }),
        insertion_mode: AddTo::Bottom,
    }]
}
