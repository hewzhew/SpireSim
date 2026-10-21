use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::CalculatedGamble,
        name: "Calculated Gamble",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
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

pub fn calculated_gamble_play(
    _state: &CombatState,
    _card: &CombatCard,
) -> SmallVec<[ActionInfo; 4]> {
    smallvec::smallvec![ActionInfo {
        // Java CalculatedGamble.use passes `false` even when the card is
        // upgraded. The upgrade changes exhaust only, not draw count.
        action: Action::CalculatedGamble { draw_extra: false },
        insertion_mode: AddTo::Bottom,
    }]
}
