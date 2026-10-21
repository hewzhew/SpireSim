use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Choose 1 other card in Hand (any number if upgraded, including none) and put the chosen cards on the bottom of the draw pile. A chosen card with positive combat cost becomes free for its next play; this uses its cost before a temporary turn-only override. Zero-cost, X-cost and unplayable cards can be moved but receive no free-play benefit. This card does not draw the moved cards. Costs 0 Energy and does not Exhaust.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::Forethought,
        name: "Forethought",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::None,
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

pub fn forethought_play(_state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        action: Action::Forethought {
            upgraded: card.upgrades > 0,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
