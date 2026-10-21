use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub const MECHANICS: &str = "Choose one Skill card currently in your draw pile and move it to your hand, with its existing cost. Costs 0 Energy. Cannot play if the draw pile has no Skill. Exhaust when unupgraded; upgrading removes Exhaust.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::SecretTechnique,
        name: "Secret Technique",
        card_type: CardType::Skill,
        rarity: CardRarity::Rare,
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

pub fn secret_technique_play(_state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let count = crate::content::cards::mechanics::card_draw_pile_skill_to_hand_v1(card.id)
        .expect("Secret Technique selection count");
    // Java queues SkillFromDeckToHandAction here. Pain/Runic Cube can change
    // the draw pile before it resolves; even singleton identity is deferred.
    smallvec::smallvec![ActionInfo {
        action: Action::SuspendForGridSelect {
            source_pile: crate::state::PileType::Draw,
            min: count,
            max: count,
            can_cancel: false,
            filter: crate::state::GridSelectFilter::Skill,
            reason: crate::state::GridSelectReason::SkillFromDeckToHand,
        },
        insertion_mode: AddTo::Bottom,
    }]
}
