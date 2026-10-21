use crate::content::cards::{CardDefinition, CardId, CardRarity, CardTarget, CardType};
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CombatCard, CombatState};
use crate::state::HandSelectFilter;
use crate::state::HandSelectReason;
use smallvec::SmallVec;

pub const MECHANICS: &str = "Create 1 additional copy (2 if upgraded) of an Attack or Power in your hand. With one eligible card, keep the original and add copies; with multiple eligible cards, select one and replace it with 2 copies (3 if upgraded). Both branches have the same net copy gain before the hand limit. No eligible card produces no copies; hand capacity applies.";

pub fn definition() -> CardDefinition {
    CardDefinition {
        id: CardId::DualWield,
        name: "Dual Wield",
        card_type: CardType::Skill,
        rarity: CardRarity::Uncommon,
        cost: 1,
        base_damage: 0,
        base_block: 0,
        base_magic: 1,
        target: CardTarget::None,
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

pub fn dual_wield_play(state: &CombatState, card: &CombatCard) -> SmallVec<[ActionInfo; 4]> {
    let mut actions = smallvec::SmallVec::new();
    let evaluated = crate::content::cards::evaluate_card_for_play(card, state, None);
    let effect =
        crate::content::cards::mechanics::card_hand_copy_v1(card.id, evaluated.base_magic_num_mut)
            .expect("Dual Wield copy rule");
    let valid_cards: Vec<_> = state
        .zones
        .hand
        .iter()
        .filter(|c| {
            effect
                .eligible_types
                .contains(&crate::content::cards::get_card_definition(c.id).card_type)
        })
        .cloned()
        .collect();

    if valid_cards.is_empty() {
        return actions;
    }

    if valid_cards.len() == 1 {
        let constructed = crate::content::cards::prepare_make_temp_card_in_hand_constructor(
            valid_cards[0].clone(),
            state,
        );
        actions.push(ActionInfo {
            action: Action::MakeConstructedCopyInHand {
                original: Box::new(constructed),
                amount: effect.single_candidate_copies,
            },
            insertion_mode: AddTo::Bottom,
        });
        return actions;
    }

    // Java DualWieldAction's multi-candidate selected branch queues one copy
    // before its `dupeAmount` loop; preserve that source-visible behavior.
    actions.push(ActionInfo {
        action: Action::SuspendForHandSelect {
            min: 1,
            max: 1,
            can_cancel: false,
            filter: HandSelectFilter::AttackOrPower,
            reason: HandSelectReason::Copy {
                amount: effect.selected_candidate_replacement_copies,
            },
        },
        insertion_mode: AddTo::Bottom,
    });

    actions
}
