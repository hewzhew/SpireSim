use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::StormOfSteel,
        name: "Storm of Steel",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
        cost: 1,
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

pub fn storm_of_steel_play(_state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    // Java BladeFuryAction receives this.upgraded to decide whether generated
    // Shivs are upgraded.
    smallvec::smallvec![ActionInfo {
        action: Action::BladeFury {
            upgraded: card.upgrades > 0,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
