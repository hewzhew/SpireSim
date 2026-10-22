use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Armaments,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Armaments,
        name: "Armaments",
        card_type: CardType::Skill,
        rarity: CardRarity::Common,
        cost: 1,
        base_damage: 0,
        base_block: 5,
        base_magic: 0,
        target: CardTarget::SelfTarget,
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

pub fn armaments_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    // ArmamentsAction inspects the live hand after GainBlock and preceding
    // hooks resolve. Pain/Runic Cube can draw new upgradeable cards first.
    let upgrade =
        crate::content::cards::mechanics::combat_upgrade_scope_for_card_v1(card.id, card.upgrades)
            .expect("Armaments has a reviewed combat upgrade scope")
            .queued_action();
    smallvec::smallvec![
        ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: evaluated.base_block_mut,
            },
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: upgrade,
            insertion_mode: AddTo::Bottom
        },
    ]
}
