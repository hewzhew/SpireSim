use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTag, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::EntityId;
use smallvec::SmallVec;

pub const MECHANICS: &str =
    "Deal 9 damage (10 if upgraded), then draw 1 card (2 if upgraded). It has the Strike tag.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::PommelStrike,
        name: "Pommel Strike",
        card_type: CardType::Attack,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 9,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[CardTag::Strike],
        upgrade_damage: 1,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn pommel_strike_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Pommel Strike requires a valid target!");
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

    actions.push(ActionInfo {
        action: Action::DrawCards(evaluated.base_magic_num_mut as u32),
        insertion_mode: AddTo::Bottom,
    });

    actions
}
