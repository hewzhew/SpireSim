use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTag, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub const MECHANICS: &str = "Deal 10 damage (12 if upgraded) and Exhaust. If this hit kills a non-Minion enemy that is not half-dead, gain 3 Max HP (4 if upgraded) and heal by the same amount. Killing a Minion does not grant Max HP.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Feed,
        name: "Feed",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 1,
        base_damage: 10,
        base_block: 0,
        base_magic: 3,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[CardTag::Healing],
        upgrade_damage: 2,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn feed_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Feed requires a valid target!");
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, Some(target));
    smallvec::smallvec![ActionInfo {
        action: Action::Feed {
            target,
            damage_info: DamageInfo {
                source: 0,
                target,
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: DamageType::Normal,
                is_modified: false,
            },
            max_hp_amount: evaluated.base_magic_num_mut,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
