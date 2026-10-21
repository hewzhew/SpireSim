use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTag, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType};
use crate::runtime::combat::{CombatCard, CombatState};
use smallvec::SmallVec;

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::LessonLearned,
        name: "Lesson Learned",
        card_type: CardType::Attack,
        rarity: CardRarity::Rare,
        cost: 2,
        base_damage: 10,
        base_block: 0,
        base_magic: 0,
        target: CardTarget::Enemy,
        is_multi_damage: false,
        exhaust: true,
        ethereal: false,
        innate: false,
        tags: &[CardTag::Healing],
        upgrade_damage: 3,
        upgrade_block: 0,
        upgrade_magic: 0,
    }
}

pub fn lesson_learned_play(
    state: &CombatState,
    card: &CombatCard,
    target: Option<usize>,
) -> SmallVec<[ActionInfo; 4]> {
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, target);
    smallvec::smallvec![ActionInfo {
        action: Action::LessonLearned {
            target: target.unwrap_or(0),
            damage_info: DamageInfo {
                source: 0,
                target: target.unwrap_or(0),
                base: evaluated.base_damage_mut,
                output: evaluated.base_damage_mut,
                damage_type: DamageType::Normal,
                is_modified: evaluated.base_damage_mut != card.base_damage_mut,
            },
        },
        insertion_mode: AddTo::Bottom,
    }]
}
