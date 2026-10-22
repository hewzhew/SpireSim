use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: super::super::mechanics::CardMechanics =
    super::super::mechanics::CardMechanics {
        text_id: crate::content::cards::CardId::Exhume,
    };

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Exhume,
        name: "Exhume",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::None,
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

pub fn exhume_play(_state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let effect = crate::content::cards::mechanics::card_exhaust_pile_to_hand_v1(card.id)
        .expect("Exhume exhaust access rule");
    // ExhumeAction checks hand capacity and the exhaust pile only when it
    // resolves. Earlier draw/exhaust hooks can change both the gate and choice.
    smallvec::smallvec![ActionInfo {
        action: Action::SuspendForGridSelect {
            source_pile: crate::state::PileType::Exhaust,
            min: effect.count,
            max: effect.count,
            can_cancel: false,
            filter: crate::state::GridSelectFilter::NonExhume,
            reason: crate::state::GridSelectReason::Exhume { upgrade: false },
        },
        insertion_mode: AddTo::Bottom,
    }]
}
