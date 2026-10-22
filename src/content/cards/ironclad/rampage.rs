use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Rampage,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Rampage,
        name: "Rampage",
        card_type: CardType::Attack,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 8,
        base_block: 0,
        base_magic: 5,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 3,
    }
}

pub fn rampage_play(
    _state: &CombatState,
    card: &CombatCard,
    target: Option<crate::EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    let target = target.expect("Rampage requires a valid target!");
    let mut actions = smallvec::SmallVec::new();
    let evaluated = crate::content::cards::evaluate_card_for_play(card, _state, Some(target));
    let damage = evaluated.base_damage_mut;
    let increase_amount = crate::content::cards::mechanics::card_damage_growth_on_play_v1(
        card.id,
        evaluated.base_magic_num_mut,
    )
    .expect("Rampage damage growth rule");

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
        action: Action::ModifyCardDamage {
            card_lineage: evaluated.lineage,
            amount: increase_amount,
        },
        insertion_mode: AddTo::Bottom,
    });

    actions
}
