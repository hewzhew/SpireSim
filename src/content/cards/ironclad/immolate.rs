use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Immolate,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Immolate,
        name: "Immolate",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 21,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::AllEnemy,
        is_multi_damage: true,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 7,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn immolate_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::DamageAllEnemies {
                source: 0,
                damages: evaluated.multi_damage.clone(),
                damage_type: crate::runtime::action::DamageType::Normal,
                is_modified: false,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: crate::content::cards::mechanics::card_status_creation_on_play_v1(card.id)
                .expect("Immolate status creation rule")
                .action(state),
            insertion_mode: AddTo::Bottom,
        }
    ]
}
