use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::ThinkingAhead,
        name: "Thinking Ahead",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn thinking_ahead_play(
    state: &CombatState,
    _card: &CombatCard,
    context: crate::content::cards::CardUseContext,
) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::smallvec![ActionInfo {
        action: Action::DrawCards(2),
        insertion_mode: AddTo::Bottom,
    }];
    if context.played_from_hand || !state.zones.hand.is_empty() {
        actions.push(ActionInfo {
            action: Action::SuspendForHandSelect {
                min: 1,
                max: 1,
                can_cancel: false,
                filter: crate::state::HandSelectFilter::Any,
                reason: crate::state::HandSelectReason::PutOnDrawPile,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions
}
