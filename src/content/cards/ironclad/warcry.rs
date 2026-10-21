use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Draw 1 card (2 if upgraded), then put 1 card from your hand on top of the draw pile. The top-deck step still occurs if drawing is prevented. Exhaust.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Warcry,
        name: "Warcry",
        card_type: CardType::Skill,
        rarity: CardRarity::Common,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn warcry_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    smallvec::smallvec![
        ActionInfo {
            action: Action::DrawCards(evaluated.base_magic_num_mut as u32),
            insertion_mode: AddTo::Bottom,
        },
        ActionInfo {
            action: Action::PutOnDeck {
                amount: crate::content::cards::mechanics::card_hand_topdeck_after_draw_v1(card.id)
                    .expect("Warcry follow-up topdeck rule"),
                random: false
            },
            insertion_mode: AddTo::Bottom,
        }
    ] // Exhaust is handled by card.exhaust = true in definitions
}
