pub const MECHANICS: &str = "Shuffle your discard pile into your draw pile, then draw 1 card (2 if upgraded). If the discard pile is empty when played, only draw; do not shuffle. The playing Deep Breath is not included in this shuffle or draw, and is normally discarded after its effects. Costs 0 Energy and does not Exhaust.";

use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::DeepBreath,
        name: "Deep Breath",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 0,
        base_damage: 0,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::SelfTarget,
        is_multi_damage: false,
        exhaust: false,
        ethereal: false,
        innate: false,
        tags: &[],
        upgrade_damage: 0,
        upgrade_block: 0,
        upgrade_magic: 1,
    }
}

pub fn deep_breath_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let mut actions = SmallVec::new();
    if !state.zones.discard_pile.is_empty() {
        actions.push(ActionInfo {
            action: Action::EmptyDeckShuffle,
            insertion_mode: AddTo::Bottom,
        });
        actions.push(ActionInfo {
            action: Action::ShuffleDrawPile {
                trigger_relics: false,
            },
            insertion_mode: AddTo::Bottom,
        });
    }
    actions.push(ActionInfo {
        action: Action::DrawCards(evaluated.base_magic_num_mut.max(0) as u32),
        insertion_mode: AddTo::Bottom,
    });
    actions
}
