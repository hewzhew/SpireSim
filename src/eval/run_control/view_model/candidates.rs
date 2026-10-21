use crate::ai::planner_core::PlannerMapRoom;
use crate::content::identity::gameplay_display_label_v1;
use crate::content::potions::{get_potion_definition, PotionId};
use crate::content::relics::RelicId;
use crate::eval::event_boundary_classifier_v1::classify_event_option_boundary_v1;
use crate::eval::run_control::RunDecisionAction;
use crate::runtime::combat::{CardLineageId, CardPileView};
use crate::sim::combat_legal_actions::engine_atomic_actions;
use crate::state::core::{CampfireChoice, ClientInput, EngineState, PendingChoice, PileType};
use crate::state::events::{EventOption, EventOptionTransition};
use crate::state::rewards::{BossRelicChoiceState, RewardItem, RewardState};
use crate::state::selection::{SelectionResolution, SelectionScope};
use std::collections::BTreeMap;

use super::labels::{
    candidate, clean_event_label, combat_card_label, keyed_candidate, monster_name,
    question_mark_outcomes_label, reward_card_label, reward_item_label, room_type_label,
    shop_block_note, unavailable_candidate,
};
use super::{CandidateResolution, DecisionCandidate, DecisionCandidateKey, RunControlSession};

pub(super) fn decision_candidates(session: &RunControlSession) -> Vec<DecisionCandidate> {
    let candidates = match &session.engine_state {
        EngineState::EventRoom => event_candidates(session),
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => map_candidates(session),
        EngineState::RewardScreen(reward) => reward_candidates(session, reward, None),
        EngineState::RewardOverlay {
            reward_state,
            return_state,
        } => reward_candidates(session, reward_state, Some(return_state.as_ref())),
        EngineState::TreasureRoom(_) => {
            vec![
                candidate("open", "Open chest", ClientInput::OpenChest, None::<String>),
                candidate(
                    "leave",
                    "Leave chest unopened",
                    ClientInput::Proceed,
                    None::<String>,
                ),
            ]
        }
        EngineState::Campfire => campfire_candidates(session),
        EngineState::Shop(shop) => shop_candidates(session, shop),
        EngineState::CombatStart(_) => Vec::new(),
        EngineState::CombatPlayerTurn
        | EngineState::CombatProcessing
        | EngineState::PendingChoice(_) => combat_candidates(session),
        EngineState::RunPendingChoice(choice) => run_choice_candidates(session, choice),
        EngineState::BossRelicSelect(choice) => boss_relic_candidates(choice),
        EngineState::Act3VictoryComplete => vec![candidate(
            "proceed",
            "Continue to the Spire Heart",
            ClientInput::Proceed,
            Some("routine"),
        )],
        EngineState::TrueVictoryComplete => vec![candidate(
            "proceed",
            "Continue to the victory screen",
            ClientInput::Proceed,
            Some("routine"),
        )],
        EngineState::DoubleBossComplete => vec![candidate(
            "proceed",
            "Continue to the second boss",
            ClientInput::Proceed,
            Some("routine"),
        )],
        EngineState::GameOver(_) => Vec::new(),
    };
    // The game automatically leaves a campfire with no usable buttons.
    // It is a forced transition even when the player still holds potions.
    if matches!(session.engine_state, EngineState::Campfire) && candidates.is_empty() {
        return candidates;
    }
    if is_run_level_potion_decision(&session.engine_state) {
        with_run_level_potion_candidates(session, candidates)
    } else {
        candidates
    }
}

fn is_run_level_potion_decision(engine_state: &EngineState) -> bool {
    matches!(
        engine_state,
        EngineState::MapNavigation
            | EngineState::MapOverlay { .. }
            | EngineState::EventRoom
            | EngineState::RewardScreen(_)
            | EngineState::RewardOverlay { .. }
            | EngineState::TreasureRoom(_)
            | EngineState::Campfire
            | EngineState::Shop(_)
            | EngineState::RunPendingChoice(_)
            | EngineState::BossRelicSelect(_)
            | EngineState::DoubleBossComplete
            | EngineState::Act3VictoryComplete
            | EngineState::TrueVictoryComplete
    )
}

fn with_run_level_potion_candidates(
    session: &RunControlSession,
    candidates: Vec<DecisionCandidate>,
) -> Vec<DecisionCandidate> {
    let is_we_meet_again_event = session
        .run_state
        .event_state
        .as_ref()
        .is_some_and(|event| event.id == crate::state::events::EventId::WeMeetAgain);
    let mut potion_candidates = Vec::new();
    for (slot, potion) in session.run_state.potions.iter().enumerate() {
        let Some(potion) = potion else {
            continue;
        };
        let use_key = DecisionCandidateKey::RunPotionUse {
            slot,
            potion: potion.id,
            uuid: potion.uuid,
        };
        if potion.can_use
            && crate::content::potions::potion_can_use_out_of_combat(
                potion.id,
                is_we_meet_again_event,
            )
            && !candidates
                .iter()
                .any(|candidate| candidate.key.as_ref() == Some(&use_key))
        {
            potion_candidates.push(keyed_candidate(
                format!("use-run-potion-{slot}"),
                format!("Use {}", get_potion_definition(potion.id).name),
                use_key,
                ClientInput::UsePotion {
                    potion_index: slot,
                    target: None,
                },
                Some("legal out-of-combat potion action"),
            ));
        }

        let discard_key = DecisionCandidateKey::RunPotionDiscard {
            slot,
            potion: potion.id,
            uuid: potion.uuid,
        };
        if potion.can_discard
            && crate::content::potions::potion_can_discard_in_event(is_we_meet_again_event)
            && !candidates
                .iter()
                .any(|candidate| candidate.key.as_ref() == Some(&discard_key))
        {
            potion_candidates.push(keyed_candidate(
                format!("discard-run-potion-{slot}"),
                format!("Discard {}", get_potion_definition(potion.id).name),
                discard_key,
                ClientInput::DiscardPotion(slot),
                Some("legal run-level potion action"),
            ));
        }
    }
    potion_candidates.extend(candidates);
    potion_candidates
}

fn event_candidates(session: &RunControlSession) -> Vec<DecisionCandidate> {
    let options = crate::engine::event_handler::get_event_options(&session.run_state);
    let event_state = session.run_state.event_state.as_ref();
    options
        .iter()
        .enumerate()
        .map(|(idx, option)| {
            let label = clean_event_label(&option.ui.text);
            let resolution = CandidateResolution::from_event_option(option);
            let note = event_option_note(option, options.len());
            if option.ui.disabled {
                unavailable_candidate(
                    idx.to_string(),
                    label,
                    option
                        .ui
                        .disabled_reason
                        .clone()
                        .unwrap_or_else(|| "disabled".to_string()),
                    note,
                )
            } else {
                let mut candidate =
                    candidate(idx.to_string(), label, ClientInput::EventChoice(idx), note);
                candidate.key = event_state.map(|event_state| DecisionCandidateKey::EventOption {
                    event_id: event_state.id,
                    screen: event_state.current_screen,
                    option_index: idx,
                    action: option.semantics.action,
                });
                candidate.resolution = resolution;
                candidate
            }
        })
        .collect()
}

fn event_option_note(option: &EventOption, option_count: usize) -> Option<String> {
    if option.ui.disabled {
        return Some(format!(
            "locked: {}",
            option.ui.disabled_reason.as_deref().unwrap_or("disabled")
        ));
    }
    if option_count == 1 {
        if let Some(note) = classify_event_option_boundary_v1(option).single_candidate_note() {
            return Some(note.to_string());
        }
    }
    match option.semantics.transition {
        EventOptionTransition::OpenSelection(kind) => {
            Some(format!("opens {} selection", selection_kind_label(kind)))
        }
        EventOptionTransition::OpenReward => Some("opens reward".to_string()),
        EventOptionTransition::StartCombat => Some("starts combat".to_string()),
        EventOptionTransition::AdvanceScreen => Some("advances event".to_string()),
        EventOptionTransition::Complete => Some("leaves event".to_string()),
        EventOptionTransition::None => None,
    }
}

fn selection_kind_label(kind: crate::state::events::EventSelectionKind) -> &'static str {
    match kind {
        crate::state::events::EventSelectionKind::None => "unknown",
        crate::state::events::EventSelectionKind::RemoveCard => "remove card",
        crate::state::events::EventSelectionKind::UpgradeCard => "upgrade card",
        crate::state::events::EventSelectionKind::TransformCard => "transform card",
        crate::state::events::EventSelectionKind::DuplicateCard => "duplicate card",
        crate::state::events::EventSelectionKind::OfferCard => "offer card",
    }
}

fn map_candidates(session: &RunControlSession) -> Vec<DecisionCandidate> {
    let target_y = if session.run_state.map.current_y == -1 {
        0
    } else {
        session.run_state.map.current_y + 1
    };
    if target_y == 15 {
        return with_map_overlay_back_candidate(
            session,
            vec![keyed_candidate(
                "0",
                "Boss room",
                DecisionCandidateKey::RouteSelect {
                    x: 0,
                    y: 15,
                    room_type: Some(PlannerMapRoom::Boss),
                    question_mark_outcomes: Vec::new(),
                    uses_wing_boots: false,
                    has_emerald_key: false,
                },
                ClientInput::SelectMapNode(0),
                Some("boss"),
            )],
        );
    }
    let Some(row) = session.run_state.map.graph.get(target_y as usize) else {
        return with_map_overlay_back_candidate(session, Vec::new());
    };
    let has_wing_boots_charge = session
        .run_state
        .relics
        .iter()
        .any(|relic| relic.id == RelicId::WingBoots && !relic.used_up && relic.counter > 0);
    let question_mark_outcomes = session
        .run_state
        .possible_question_mark_room_types(session.run_state.map.get_current_room_type())
        .into_iter()
        .map(Into::into)
        .collect::<Vec<_>>();
    let candidates = row
        .iter()
        .filter_map(|node| {
            let normal = session.run_state.map.can_travel_to(node.x, node.y, false);
            let wing_boots =
                has_wing_boots_charge && session.run_state.map.can_travel_to(node.x, node.y, true);
            let public_room = node.class.map(PlannerMapRoom::from);
            let public_outcomes = if public_room == Some(PlannerMapRoom::QuestionMark) {
                question_mark_outcomes.clone()
            } else {
                Vec::new()
            };
            let resolution_label = question_mark_outcomes_label(&public_outcomes);
            if normal {
                Some(keyed_candidate(
                    node.x.to_string(),
                    format!(
                        "y={} {}{}",
                        node.y,
                        room_type_label(public_room),
                        resolution_label
                    ),
                    DecisionCandidateKey::RouteSelect {
                        x: node.x,
                        y: node.y,
                        room_type: public_room,
                        question_mark_outcomes: public_outcomes,
                        uses_wing_boots: false,
                        has_emerald_key: node.has_emerald_key,
                    },
                    ClientInput::SelectMapNode(node.x as usize),
                    node.has_emerald_key.then_some("emerald elite"),
                ))
            } else if wing_boots {
                Some(keyed_candidate(
                    format!("fly-{}-{}", node.x, node.y),
                    format!(
                        "y={} {}{} (Wing Boots)",
                        node.y,
                        room_type_label(public_room),
                        resolution_label
                    ),
                    DecisionCandidateKey::RouteSelect {
                        x: node.x,
                        y: node.y,
                        room_type: public_room,
                        question_mark_outcomes: public_outcomes,
                        uses_wing_boots: true,
                        has_emerald_key: node.has_emerald_key,
                    },
                    ClientInput::FlyToNode(node.x as usize, node.y as usize),
                    node.has_emerald_key.then_some("emerald elite"),
                ))
            } else {
                None
            }
        })
        .collect();
    with_map_overlay_back_candidate(session, candidates)
}

fn with_map_overlay_back_candidate(
    session: &RunControlSession,
    mut candidates: Vec<DecisionCandidate>,
) -> Vec<DecisionCandidate> {
    if matches!(session.engine_state, EngineState::MapOverlay { .. }) {
        candidates.push(keyed_candidate(
            "back",
            "Back to reward screen",
            DecisionCandidateKey::RouteCancel,
            ClientInput::Cancel,
            Some("unclaimed rewards remain"),
        ));
    }
    candidates
}

fn reward_candidates(
    session: &RunControlSession,
    reward: &RewardState,
    overlay_return_state: Option<&EngineState>,
) -> Vec<DecisionCandidate> {
    if let Some(cards) = reward.pending_card_choice.as_ref() {
        let reward_item_index = reward.pending_card_reward_index;
        let mut candidates = cards
            .iter()
            .enumerate()
            .map(|(idx, card)| {
                let obtained_upgrades = session
                    .run_state
                    .preview_obtain_card_upgrades(card.id, card.upgrades);
                let mut candidate = candidate(
                    idx.to_string(),
                    reward_card_label(card.id, obtained_upgrades),
                    ClientInput::SelectCard(idx),
                    None::<String>,
                );
                if let Some(reward_item_index) = reward_item_index {
                    candidate.key = Some(DecisionCandidateKey::CardRewardPick {
                        reward_item_index,
                        option_index: idx,
                        card: card.id,
                        upgrades: obtained_upgrades,
                    });
                }
                candidate.resolution = Some(CandidateResolution::from_reward_card(card));
                candidate
            })
            .collect::<Vec<_>>();
        if session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::SingingBowl)
        {
            candidates.push(candidate(
                "bowl",
                "Singing Bowl | gain 2 max HP",
                ClientInput::SelectCard(cards.len()),
                Some("consume this card reward instead of taking a card"),
            ));
            if let Some(candidate) = candidates.last_mut() {
                if let Some(reward_item_index) = reward_item_index {
                    candidate.key = Some(DecisionCandidateKey::CardRewardSingingBowl {
                        reward_item_index,
                        option_index: cards.len(),
                    });
                }
            }
        }
        if let Some(reward_item_index) = reward.pending_card_reward_index {
            let mut skip = candidate(
                "skip-card-reward",
                "Skip card reward",
                RunDecisionAction::SkipCardReward { reward_item_index },
                Some("close this card choice without consuming its reward item"),
            );
            skip.key = Some(DecisionCandidateKey::CardRewardSkip { reward_item_index });
            candidates.push(skip);
        } else {
            candidates.push(candidate(
                "back",
                "Back to reward screen",
                ClientInput::Cancel,
                Some("card reward remains"),
            ));
        }
        return candidates;
    }

    let potion_claim_changes_state = session.run_state.find_empty_potion_slot().is_some()
        || session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::Sozu);
    let mut candidates = reward
        .items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            if matches!(item, RewardItem::Potion { .. }) && !potion_claim_changes_state {
                return None;
            }
            let mut candidate = candidate(
                idx.to_string(),
                reward_item_label(item),
                ClientInput::ClaimReward(idx),
                None::<String>,
            );
            match item {
                RewardItem::Card { .. } => {
                    candidate.key = Some(DecisionCandidateKey::CardRewardOpen {
                        reward_item_index: idx,
                    });
                }
                RewardItem::Potion { potion_id } => {
                    candidate.key = Some(DecisionCandidateKey::RewardPotionClaim {
                        reward_item_index: idx,
                        potion: *potion_id,
                    });
                }
                _ => {}
            }
            candidate.resolution =
                CandidateResolution::from_reward_item(item, reward, &session.run_state);
            Some(candidate)
        })
        .collect::<Vec<_>>();
    if reward.has_card_reward_item()
        && session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::SingingBowl)
    {
        if let Some(reward_item_index) = reward
            .items
            .iter()
            .position(|item| matches!(item, crate::state::rewards::RewardItem::Card { .. }))
        {
            candidates.push(candidate(
                "bowl",
                "Singing Bowl | gain 2 max HP",
                RunDecisionAction::SingingBowlCardReward { reward_item_index },
                Some("consume the first visible card reward instead of taking a card"),
            ));
            if let Some(candidate) = candidates.last_mut() {
                candidate.key = Some(DecisionCandidateKey::CardRewardSingingBowl {
                    reward_item_index,
                    option_index: 0,
                });
            }
        }
    }
    let has_unclaimed_potion = reward
        .items
        .iter()
        .any(|item| matches!(item, crate::state::rewards::RewardItem::Potion { .. }));
    if let Some((potion_index, potion)) =
        session
            .run_state
            .potions
            .iter()
            .enumerate()
            .find_map(|(slot, potion)| {
                let potion = potion.as_ref()?;
                (has_unclaimed_potion
                    && potion.id == PotionId::FruitJuice
                    && potion.can_use
                    && crate::content::potions::potion_can_use_out_of_combat(
                        potion.id,
                        session.run_state.event_state.as_ref().is_some_and(|event| {
                            event.id == crate::state::events::EventId::WeMeetAgain
                        }),
                    ))
                .then_some((slot, potion))
            })
    {
        candidates.push(keyed_candidate(
            format!("use-potion-{potion_index}"),
            "Use Fruit Juice to make room for potion reward",
            DecisionCandidateKey::RunPotionUse {
                slot: potion_index,
                potion: potion.id,
                uuid: potion.uuid,
            },
            ClientInput::UsePotion {
                potion_index,
                target: None,
            },
            Some("the unclaimed potion reward remains available"),
        ));
    }
    let is_we_meet_again = session
        .run_state
        .event_state
        .as_ref()
        .is_some_and(|event| event.id == crate::state::events::EventId::WeMeetAgain);
    if has_unclaimed_potion
        && session.run_state.find_empty_potion_slot().is_none()
        && crate::content::potions::potion_can_discard_in_event(is_we_meet_again)
    {
        candidates.extend(session.run_state.potions.iter().enumerate().filter_map(
            |(slot, potion)| {
                let potion = potion.as_ref().filter(|potion| potion.can_discard)?;
                Some(keyed_candidate(
                    format!("discard-potion-{slot}"),
                    format!(
                        "Discard {} to make room for potion reward",
                        get_potion_definition(potion.id).name
                    ),
                    DecisionCandidateKey::RunPotionDiscard {
                        slot,
                        potion: potion.id,
                        uuid: potion.uuid,
                    },
                    ClientInput::DiscardPotion(slot),
                    Some("the unclaimed potion reward remains available"),
                ))
            },
        ));
    }
    if reward.skippable {
        let (id, label, note, input) = if let Some(return_state) = overlay_return_state {
            (
                "back",
                reward_overlay_return_label(return_state),
                if reward.items.is_empty() {
                    Some("routine")
                } else if matches!(return_state, EngineState::Shop(_)) {
                    Some("unclaimed overlay rewards remain available in the shop")
                } else {
                    Some("returns to previous screen with unclaimed overlay rewards")
                },
                ClientInput::Cancel,
            )
        } else if reward.items.is_empty() {
            (
                "skip",
                "Leave reward screen".to_string(),
                Some("routine"),
                ClientInput::Proceed,
            )
        } else if session.run_state.pending_boss_reward {
            (
                "skip",
                "Proceed to boss relic".to_string(),
                Some("leave any unclaimed boss-combat rewards behind"),
                ClientInput::Proceed,
            )
        } else if session.run_state.pending_boss_act_transition {
            (
                "skip",
                "Proceed to next Act".to_string(),
                Some("leave any unclaimed boss-relic rewards behind"),
                ClientInput::Proceed,
            )
        } else {
            (
                "skip",
                "Open map preview".to_string(),
                Some("unclaimed rewards remain until a path is chosen"),
                ClientInput::Proceed,
            )
        };
        candidates.push(candidate(id, label, input, note));
    }
    candidates
}

fn reward_overlay_return_label(return_state: &EngineState) -> String {
    match return_state {
        EngineState::Shop(_) => "Return to shop".to_string(),
        EngineState::EventRoom => "Return to event".to_string(),
        EngineState::Campfire => "Return to campfire".to_string(),
        EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. } => {
            "Return to reward screen".to_string()
        }
        _ => "Return to previous screen".to_string(),
    }
}

fn campfire_candidates(session: &RunControlSession) -> Vec<DecisionCandidate> {
    let mut candidates = Vec::new();
    for choice in crate::engine::campfire_handler::get_available_options(&session.run_state) {
        match choice {
            CampfireChoice::Rest => candidates.push(keyed_candidate(
                "rest",
                "Rest",
                DecisionCandidateKey::CampfireRest,
                ClientInput::CampfireOption(CampfireChoice::Rest),
                None::<String>,
            )),
            CampfireChoice::Smith(_) => {
                candidates.extend(
                    session
                        .run_state
                        .master_deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| crate::state::core::master_deck_card_can_upgrade(card))
                        .map(|(idx, card)| {
                            keyed_candidate(
                                format!("smith-{idx}"),
                                format!("Smith {}", combat_card_label(card)),
                                DecisionCandidateKey::CampfireSmith {
                                    deck_index: idx,
                                    card_lineage: card.lineage,
                                    card: card.id,
                                    upgrades: card.upgrades,
                                },
                                ClientInput::CampfireOption(CampfireChoice::Smith(idx)),
                                None::<String>,
                            )
                        }),
                );
            }
            CampfireChoice::Dig => candidates.push(keyed_candidate(
                "dig",
                "Dig",
                DecisionCandidateKey::CampfireDig,
                ClientInput::CampfireOption(CampfireChoice::Dig),
                None::<String>,
            )),
            CampfireChoice::Lift => candidates.push(keyed_candidate(
                "lift",
                "Lift",
                DecisionCandidateKey::CampfireLift,
                ClientInput::CampfireOption(CampfireChoice::Lift),
                None::<String>,
            )),
            CampfireChoice::Toke(_) => {
                candidates.extend(
                    session
                        .run_state
                        .master_deck
                        .iter()
                        .enumerate()
                        .filter(|(_, card)| {
                            crate::state::core::master_deck_card_is_purgeable(card)
                                && !crate::state::core::master_deck_card_is_bottled(
                                    card,
                                    &session.run_state.relics,
                                )
                        })
                        .map(|(idx, card)| {
                            keyed_candidate(
                                format!("toke-{idx}"),
                                format!("Toke {}", combat_card_label(card)),
                                DecisionCandidateKey::CampfireToke {
                                    deck_index: idx,
                                    card_lineage: card.lineage,
                                    card: card.id,
                                    upgrades: card.upgrades,
                                },
                                ClientInput::CampfireOption(CampfireChoice::Toke(idx)),
                                None::<String>,
                            )
                        }),
                );
            }
            CampfireChoice::Recall => candidates.push(keyed_candidate(
                "recall",
                "Recall ruby key",
                DecisionCandidateKey::CampfireRecall,
                ClientInput::CampfireOption(CampfireChoice::Recall),
                None::<String>,
            )),
        }
    }
    candidates
}

fn shop_candidates(
    session: &RunControlSession,
    shop: &crate::state::shop::ShopState,
) -> Vec<DecisionCandidate> {
    let mut candidates = Vec::new();
    candidates.extend(shop.cards.iter().enumerate().map(|(idx, card)| {
        let label = format!(
            "{} | {} gold",
            reward_card_label(card.card_id, card.upgrades),
            card.price
        );
        let block_reason = super::super::shop_merchandise_purchase_block_reason_v1(
            &session.run_state,
            card.can_buy,
            card.blocked_reason.as_deref(),
            card.price,
        );
        let block_label = block_reason
            .as_ref()
            .map(super::super::render_shop_purchase_block_reason_v1);
        let note = shop_block_note(block_reason.is_none(), block_label.as_deref());
        let key = DecisionCandidateKey::ShopBuyCard {
            shop_slot: idx,
            card: card.card_id,
            upgrades: card.upgrades,
            price: card.price,
        };
        let mut candidate = if block_reason.is_none() {
            candidate(
                format!("card-{idx}"),
                label,
                ClientInput::BuyCard(idx),
                note,
            )
        } else {
            unavailable_candidate(
                format!("card-{idx}"),
                label,
                block_label.unwrap_or_else(|| "cannot buy".to_string()),
                note,
            )
        };
        candidate.key = Some(key);
        candidate
    }));
    candidates.extend(shop.relics.iter().enumerate().map(|(idx, relic)| {
        let label = format!("{:?} | {} gold", relic.relic_id, relic.price);
        let block_reason = super::super::shop_merchandise_purchase_block_reason_v1(
            &session.run_state,
            relic.can_buy,
            relic.blocked_reason.as_deref(),
            relic.price,
        );
        let block_label = block_reason
            .as_ref()
            .map(super::super::render_shop_purchase_block_reason_v1);
        let note = shop_block_note(block_reason.is_none(), block_label.as_deref());
        let key = DecisionCandidateKey::ShopBuyRelic {
            shop_slot: idx,
            relic: relic.relic_id,
            price: relic.price,
        };
        let mut candidate = if block_reason.is_none() {
            candidate(
                format!("relic-{idx}"),
                label,
                ClientInput::BuyRelic(idx),
                note,
            )
        } else {
            unavailable_candidate(
                format!("relic-{idx}"),
                label,
                block_label.unwrap_or_else(|| "cannot buy".to_string()),
                note,
            )
        };
        candidate.key = Some(key);
        candidate
    }));
    candidates.extend(shop.potions.iter().enumerate().map(|(idx, potion)| {
        let label = format!(
            "{} | {} gold",
            get_potion_definition(potion.potion_id).name,
            potion.price
        );
        let block_reason =
            super::super::shop_potion_purchase_block_reason_v1(&session.run_state, potion);
        let block_label = block_reason
            .as_ref()
            .map(super::super::render_shop_purchase_block_reason_v1);
        let note = shop_block_note(block_reason.is_none(), block_label.as_deref());
        let key = DecisionCandidateKey::ShopBuyPotion {
            shop_slot: idx,
            potion: potion.potion_id,
            price: potion.price,
        };
        let mut candidate = if block_reason.is_none() {
            candidate(
                format!("potion-{idx}"),
                label,
                ClientInput::BuyPotion(idx),
                note,
            )
        } else {
            unavailable_candidate(
                format!("potion-{idx}"),
                label,
                block_label.unwrap_or_else(|| "cannot buy".to_string()),
                note,
            )
        };
        candidate.key = Some(key);
        candidate
    }));
    let purge_block = super::super::shop_purge_purchase_block_reason_v1(&session.run_state, shop);
    if purge_block.is_none() {
        candidates.extend(session.run_state.master_deck.iter().enumerate().filter_map(
            |(deck_index, card)| {
                if !crate::state::core::master_deck_card_is_purgeable(card)
                    || crate::state::core::master_deck_card_is_bottled(
                        card,
                        &session.run_state.relics,
                    )
                {
                    return None;
                }
                let mut candidate = candidate(
                    format!("purge-{deck_index}"),
                    format!(
                        "Remove {} | {} gold",
                        combat_card_label(card),
                        shop.purge_cost
                    ),
                    ClientInput::PurgeCard(deck_index),
                    None::<String>,
                );
                candidate.key = Some(DecisionCandidateKey::ShopPurgeCard {
                    deck_index,
                    card: card.id,
                    upgrades: card.upgrades,
                });
                Some(candidate)
            },
        ));
    } else {
        let purge_block_label = purge_block
            .as_ref()
            .map(super::super::render_shop_purchase_block_reason_v1)
            .unwrap_or_else(|| "locked".to_string());
        candidates.push(unavailable_candidate(
            "purge",
            format!("Remove card | {} gold", shop.purge_cost),
            purge_block_label.clone(),
            Some(format!("locked: {purge_block_label}")),
        ));
    }
    if shop.pending_reward_overlay.is_some() {
        let mut rewards = candidate(
            "rewards",
            "Open pending rewards",
            ClientInput::OpenRewardOverlay,
            Some("shop overlay rewards remain until leaving the shop"),
        );
        rewards.key = Some(DecisionCandidateKey::ShopOpenRewards);
        candidates.push(rewards);
    }
    let mut leave = candidate("leave", "Leave shop", ClientInput::Proceed, None::<String>);
    leave.key = Some(DecisionCandidateKey::ShopLeave);
    candidates.push(leave);
    candidates
}

fn combat_candidates(session: &RunControlSession) -> Vec<DecisionCandidate> {
    let Ok(position) = session.current_combat_position_for_actions() else {
        return Vec::new();
    };
    if let EngineState::PendingChoice(choice) = &position.engine {
        return pending_choice_candidates(session, choice, &position.combat);
    }
    let legal_moves = engine_atomic_actions(&position.engine, &position.combat);
    let mut playable: BTreeMap<usize, Vec<Option<usize>>> = BTreeMap::new();
    let mut usable_potions: BTreeMap<usize, Vec<Option<usize>>> = BTreeMap::new();
    let mut discardable_potions = Vec::new();
    let mut end_turn = false;
    for action in &legal_moves {
        match action {
            ClientInput::PlayCard { card_index, target } => {
                playable.entry(*card_index).or_default().push(*target);
            }
            ClientInput::UsePotion {
                potion_index,
                target,
            } => {
                usable_potions
                    .entry(*potion_index)
                    .or_default()
                    .push(*target);
            }
            ClientInput::DiscardPotion(slot) => discardable_potions.push(*slot),
            ClientInput::EndTurn => end_turn = true,
            _ => {}
        }
    }

    let mut candidates = Vec::new();
    for (card_index, targets) in playable {
        let Some(card) = position.combat.zones.hand.get(card_index) else {
            continue;
        };
        if targets.len() == 1 {
            let target = targets[0];
            let label = match target {
                Some(target_id) => format!(
                    "Play {} -> {}",
                    combat_card_label(card),
                    combat_target_label(&position.combat, target_id)
                ),
                None => format!("Play {}", combat_card_label(card)),
            };
            candidates.push(candidate(
                card_index.to_string(),
                label,
                ClientInput::PlayCard { card_index, target },
                None::<String>,
            ));
        } else {
            for target in targets {
                let Some(target_id) = target else {
                    continue;
                };
                let Some(slot) = combat_target_slot(&position.combat, target_id) else {
                    continue;
                };
                candidates.push(candidate(
                    format!("{card_index}.{slot}"),
                    format!(
                        "Play {} -> {}",
                        combat_card_label(card),
                        combat_target_label(&position.combat, target_id)
                    ),
                    ClientInput::PlayCard {
                        card_index,
                        target: Some(target_id),
                    },
                    None::<String>,
                ));
            }
        }
    }
    for (potion_index, targets) in usable_potions {
        let Some(potion) = position
            .combat
            .entities
            .potions
            .get(potion_index)
            .and_then(Option::as_ref)
        else {
            continue;
        };
        let potion_name = get_potion_definition(potion.id).name;
        if targets.len() == 1 {
            let target = targets[0];
            let label = match target {
                Some(target_id) => format!(
                    "Use {} -> {}",
                    potion_name,
                    combat_target_label(&position.combat, target_id)
                ),
                None => format!("Use {potion_name}"),
            };
            candidates.push(candidate(
                format!("potion.{potion_index}"),
                label,
                ClientInput::UsePotion {
                    potion_index,
                    target,
                },
                None::<String>,
            ));
        } else {
            for target in targets {
                let Some(target_id) = target else {
                    continue;
                };
                let Some(slot) = combat_target_slot(&position.combat, target_id) else {
                    continue;
                };
                candidates.push(candidate(
                    format!("potion.{potion_index}.{slot}"),
                    format!(
                        "Use {} -> {}",
                        potion_name,
                        combat_target_label(&position.combat, target_id)
                    ),
                    ClientInput::UsePotion {
                        potion_index,
                        target: Some(target_id),
                    },
                    None::<String>,
                ));
            }
        }
    }
    for potion_index in discardable_potions {
        let Some(potion) = position
            .combat
            .entities
            .potions
            .get(potion_index)
            .and_then(Option::as_ref)
        else {
            continue;
        };
        candidates.push(candidate(
            format!("discard-potion.{potion_index}"),
            format!("Discard {}", get_potion_definition(potion.id).name),
            ClientInput::DiscardPotion(potion_index),
            None::<String>,
        ));
    }
    if end_turn {
        candidates.push(candidate(
            "end",
            "End turn",
            ClientInput::EndTurn,
            None::<String>,
        ));
    }
    candidates
}

fn pending_choice_candidates(
    session: &RunControlSession,
    choice: &PendingChoice,
    combat: &crate::runtime::combat::CombatState,
) -> Vec<DecisionCandidate> {
    if let Some(surface) =
        crate::eval::run_control::selection_surface::active_selection_surface(session)
    {
        let mut select = candidate(
            "select",
            format!("Submit selection with `{}`", surface.submit_hint),
            surface.submit_hint,
            Some(selection_surface_note(&surface)),
        );
        select.key = Some(DecisionCandidateKey::SelectionSubmit {
            scope: surface.scope,
            reason: surface.reason,
            min_choices: surface.min_choices,
            max_choices: surface.max_choices,
            item_count: surface.item_count,
        });
        let mut candidates = vec![select];
        if surface.can_cancel {
            candidates.push(candidate(
                "cancel",
                "Cancel selection",
                ClientInput::Cancel,
                Some("return without selecting cards"),
            ));
        }
        candidates.extend(pending_choice_potion_discard_candidates(combat));
        return candidates;
    }

    let legal_surface = crate::sim::combat_action_surface::combat_legal_action_surface_v2(
        &EngineState::PendingChoice(choice.clone()),
        combat,
    );
    legal_surface
        .atomic_actions
        .into_iter()
        .enumerate()
        .map(|(idx, input)| {
            let label = pending_choice_input_label(choice, combat, &input);
            candidate(idx.to_string(), label, input, None::<String>)
        })
        .collect()
}

fn pending_choice_potion_discard_candidates(
    combat: &crate::runtime::combat::CombatState,
) -> Vec<DecisionCandidate> {
    combat
        .entities
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            let potion = potion.as_ref().filter(|potion| potion.can_discard)?;
            Some(candidate(
                format!("discard-potion.{slot}"),
                format!("Discard {}", get_potion_definition(potion.id).name),
                ClientInput::DiscardPotion(slot),
                Some("keeps the current combat card selection open"),
            ))
        })
        .collect()
}

fn selection_surface_note(
    surface: &crate::eval::run_control::selection_surface::SelectionSurface,
) -> String {
    if surface.min_choices == 0 {
        format!(
            "choose 0-{} from {} visible item(s); `select` chooses nothing",
            surface.max_choices, surface.item_count
        )
    } else {
        format!(
            "choose {}-{} from {} visible item(s)",
            surface.min_choices, surface.max_choices, surface.item_count
        )
    }
}

fn pending_choice_input_label(
    choice: &PendingChoice,
    combat: &crate::runtime::combat::CombatState,
    input: &ClientInput,
) -> String {
    match (choice, input) {
        (
            PendingChoice::GridSelect {
                source_pile,
                reason,
                ..
            },
            ClientInput::SubmitSelection(resolution),
        ) => format!(
            "{} {}",
            grid_reason_verb(*reason),
            selected_card_labels(
                grid_source_cards(combat, *source_pile),
                &resolution.selected_card_lineages()
            )
            .join(", ")
        ),
        (PendingChoice::HandSelect { reason, .. }, ClientInput::SubmitSelection(resolution)) => {
            format!(
                "{} {}",
                hand_reason_verb(*reason),
                selected_card_labels(
                    CardPileView::Contiguous(&combat.zones.hand),
                    &resolution.selected_card_lineages(),
                )
                .join(", ")
            )
        }
        (PendingChoice::DiscoverySelect(choice), ClientInput::SubmitDiscoverChoice(idx)) => choice
            .cards
            .get(*idx)
            .map(|card| format!("Choose {}", reward_card_label(*card, 0)))
            .unwrap_or_else(|| format!("Choose {idx}")),
        (PendingChoice::ScrySelect { cards, .. }, ClientInput::SubmitScryDiscard(indices)) => {
            if indices.is_empty() {
                "Keep all".to_string()
            } else {
                let selected = indices
                    .iter()
                    .map(|idx| {
                        cards
                            .get(*idx)
                            .map(|card| reward_card_label(*card, 0))
                            .unwrap_or_else(|| format!("card {idx}"))
                    })
                    .collect::<Vec<_>>();
                format!("Discard {}", selected.join(", "))
            }
        }
        (PendingChoice::CardRewardSelect { cards, .. }, ClientInput::SubmitDiscoverChoice(idx)) => {
            cards
                .get(*idx)
                .map(|card| format!("Choose {}", reward_card_label(*card, 0)))
                .unwrap_or_else(|| format!("Choose {idx}"))
        }
        (
            PendingChoice::ForeignInfluenceSelect { cards, upgraded },
            ClientInput::SubmitDiscoverChoice(idx),
        ) => cards
            .get(*idx)
            .map(|card| format!("Choose {}", reward_card_label(*card, u8::from(*upgraded))))
            .unwrap_or_else(|| format!("Choose {idx}")),
        (PendingChoice::ChooseOneSelect { choices }, ClientInput::SubmitDiscoverChoice(idx)) => {
            choices
                .get(*idx)
                .map(|choice| {
                    format!(
                        "Choose {}",
                        reward_card_label(choice.card_id, choice.upgrades)
                    )
                })
                .unwrap_or_else(|| format!("Choose {idx}"))
        }
        (PendingChoice::StanceChoice, ClientInput::SubmitDiscoverChoice(0)) => {
            "Choose Wrath".to_string()
        }
        (PendingChoice::StanceChoice, ClientInput::SubmitDiscoverChoice(1)) => {
            "Choose Calm".to_string()
        }
        (_, ClientInput::Cancel) => "Cancel".to_string(),
        (_, other) => crate::eval::run_control::view_model::client_input_hint(other),
    }
}

fn selected_card_labels(cards: CardPileView<'_>, lineages: &[CardLineageId]) -> Vec<String> {
    if lineages.is_empty() {
        return vec!["nothing".to_string()];
    }
    lineages
        .iter()
        .map(|lineage| {
            cards
                .iter()
                .find(|card| card.lineage == *lineage)
                .map(combat_card_label)
                .unwrap_or_else(|| format!("card lineage {lineage}"))
        })
        .collect()
}

fn grid_source_cards(
    combat: &crate::runtime::combat::CombatState,
    source_pile: PileType,
) -> CardPileView<'_> {
    match source_pile {
        PileType::Draw => CardPileView::Contiguous(&combat.zones.draw_pile),
        PileType::Discard => CardPileView::Discard(&combat.zones.discard_pile),
        PileType::Exhaust => CardPileView::Contiguous(&combat.zones.exhaust_pile),
        PileType::Hand => CardPileView::Contiguous(&combat.zones.hand),
        PileType::Limbo => CardPileView::Contiguous(&combat.zones.limbo),
        PileType::MasterDeck => CardPileView::Contiguous(&combat.meta.master_deck_snapshot),
    }
}

fn grid_reason_verb(reason: crate::state::GridSelectReason) -> &'static str {
    match reason {
        crate::state::GridSelectReason::MoveToDrawPile => "Put on top of draw pile:",
        crate::state::GridSelectReason::Exhume { .. } => "Exhume:",
        crate::state::GridSelectReason::DrawPileToHand => "Add to hand:",
        crate::state::GridSelectReason::SkillFromDeckToHand => "Add skill to hand:",
        crate::state::GridSelectReason::AttackFromDeckToHand => "Add attack to hand:",
        crate::state::GridSelectReason::DiscardToHand => "Return to hand:",
        crate::state::GridSelectReason::DiscardToHandNoCostChange => "Return to hand:",
        crate::state::GridSelectReason::DiscardToHandRetain => "Return to hand and retain:",
        crate::state::GridSelectReason::Omniscience { .. } => "Choose for Omniscience:",
    }
}

fn hand_reason_verb(reason: crate::state::HandSelectReason) -> &'static str {
    match reason {
        crate::state::HandSelectReason::Exhaust
        | crate::state::HandSelectReason::PotionExhaust { .. } => "Exhaust:",
        crate::state::HandSelectReason::Discard => "Discard:",
        crate::state::HandSelectReason::Retain => "Retain:",
        crate::state::HandSelectReason::PutOnDrawPile => "Put on top of draw pile:",
        crate::state::HandSelectReason::PutToBottomOfDraw => "Put on bottom of draw pile:",
        crate::state::HandSelectReason::Setup => "Set up:",
        crate::state::HandSelectReason::Copy { .. } => "Copy:",
        crate::state::HandSelectReason::Nightmare { .. } => "Nightmare:",
        crate::state::HandSelectReason::Upgrade => "Upgrade:",
        crate::state::HandSelectReason::GamblingChip => "Discard for Gambling Chip:",
        crate::state::HandSelectReason::Recycle => "Recycle:",
    }
}

fn combat_target_slot(
    combat: &crate::runtime::combat::CombatState,
    target_id: usize,
) -> Option<u8> {
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target_id)
        .map(|monster| monster.slot)
}

fn combat_target_label(combat: &crate::runtime::combat::CombatState, target_id: usize) -> String {
    combat
        .entities
        .monsters
        .iter()
        .find(|monster| monster.id == target_id)
        .map(|monster| format!("{} slot {}", monster_name(monster.enemy_id), monster.slot))
        .unwrap_or_else(|| format!("entity {target_id}"))
}

fn run_choice_candidates(
    session: &RunControlSession,
    choice: &crate::state::core::RunPendingChoiceState,
) -> Vec<DecisionCandidate> {
    let Some(surface) =
        crate::eval::run_control::selection_surface::active_selection_surface(session)
    else {
        return Vec::new();
    };
    let single_choice = choice.min_choices == 1 && choice.max_choices == 1;
    if single_choice {
        return surface
            .items
            .iter()
            .map(|item| {
                candidate(
                    item.visible_index.to_string(),
                    reward_card_label(item.card, item.upgrades),
                    ClientInput::SubmitSelection(SelectionResolution {
                        scope: SelectionScope::Deck,
                        selected: vec![item.target],
                    }),
                    None::<String>,
                )
            })
            .collect();
    }

    let mut select = candidate(
        "select",
        format!("Submit selection with `{}`", surface.submit_hint),
        surface.submit_hint,
        Some(selection_surface_note(&surface)),
    );
    select.key = Some(DecisionCandidateKey::SelectionSubmit {
        scope: surface.scope,
        reason: surface.reason,
        min_choices: surface.min_choices,
        max_choices: surface.max_choices,
        item_count: surface.item_count,
    });
    vec![select]
}

fn boss_relic_candidates(choice: &BossRelicChoiceState) -> Vec<DecisionCandidate> {
    let mut candidates = choice
        .relics
        .iter()
        .enumerate()
        .map(|(idx, relic)| {
            let mut candidate = candidate(
                idx.to_string(),
                gameplay_display_label_v1(*relic).to_string(),
                ClientInput::SubmitRelicChoice(idx),
                None::<String>,
            );
            candidate.key = Some(DecisionCandidateKey::BossRelicPick {
                option_index: idx,
                relic: *relic,
            });
            candidate.resolution = Some(CandidateResolution::from_boss_relic(*relic));
            candidate
        })
        .collect::<Vec<_>>();
    let mut skip = candidate(
        "skip",
        "Skip boss relic",
        ClientInput::Cancel,
        Some("leaves boss chest without taking a boss relic"),
    );
    skip.key = Some(DecisionCandidateKey::BossRelicSkip);
    candidates.push(skip);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{
        ActiveCombat, CombatContext, GridSelectReason, PendingChoice, RoomCombatContext,
    };
    use crate::state::map::node::RoomType;
    use crate::state::rewards::RewardItem;

    #[test]
    fn combat_candidate_surface_exposes_untargeted_potion_action() {
        let mut session = RunControlSession::new(Default::default());
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster =
            crate::test_support::planned_monster(crate::content::monsters::EnemyId::JawWorm, 1);
        monster
            .set_planned_visible_spec(Some(crate::runtime::monster_move::MonsterMoveSpec::Unknown));
        combat.entities.monsters = vec![monster];
        combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 41))];
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));

        let potion = decision_candidates(&session)
            .into_iter()
            .find(|candidate| candidate.id == "potion.0")
            .expect("combat decision surface should expose a usable Block Potion");

        assert_eq!(potion.label, "Use Block Potion");
        assert_eq!(
            potion.action.executable_input(),
            Some(ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            })
        );
    }

    #[test]
    fn combat_candidate_surface_exposes_discard_potion_action() {
        let mut session = RunControlSession::new(Default::default());
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster =
            crate::test_support::planned_monster(crate::content::monsters::EnemyId::JawWorm, 1);
        monster
            .set_planned_visible_spec(Some(crate::runtime::monster_move::MonsterMoveSpec::Unknown));
        combat.entities.monsters = vec![monster];
        combat.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 41))];
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));

        let discard = decision_candidates(&session)
            .into_iter()
            .find(|candidate| candidate.id == "discard-potion.0")
            .expect("combat decision surface should expose a potion discard action");

        assert_eq!(discard.label, "Discard Block Potion");
        assert_eq!(
            discard.action.executable_input(),
            Some(ClientInput::DiscardPotion(0))
        );
    }

    #[test]
    fn pending_hand_selection_exposes_and_executes_potion_discard_without_closing() {
        let mut session = RunControlSession::new(Default::default());
        let selected = CombatCard::for_scenario(CardId::Defend, 41);
        let selected_lineage = selected.lineage;
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![selected];
        combat.entities.potions = vec![Some(Potion::new(PotionId::EnergyPotion, 52))];
        let choice = PendingChoice::HandSelect {
            candidate_lineages: vec![selected_lineage],
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: crate::state::core::HandSelectReason::Exhaust,
        };
        session.engine_state = EngineState::PendingChoice(choice.clone());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::PendingChoice(choice.clone()),
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomElite,
            }),
        ));

        let candidates = decision_candidates(&session);
        assert!(candidates.iter().any(|candidate| matches!(
            candidate.key,
            Some(DecisionCandidateKey::SelectionSubmit { .. })
        )));
        let discard = candidates
            .iter()
            .find(|candidate| candidate.id == "discard-potion.0")
            .expect("pending HandSelect should retain the top-panel potion discard");
        assert_eq!(
            discard.action.executable_input(),
            Some(ClientInput::DiscardPotion(0))
        );

        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::DiscardPotion(0)))
            .expect("pending HandSelect potion discard should execute");
        assert_eq!(session.engine_state, EngineState::PendingChoice(choice));
        let active = session
            .active_combat
            .as_ref()
            .expect("combat remains active");
        assert!(active.combat_state.entities.potions[0].is_none());
        assert_eq!(active.combat_state.zones.hand[0].lineage, selected_lineage);
    }

    #[test]
    fn pending_grid_select_uses_compact_selection_command() {
        let mut session = RunControlSession::new(Default::default());
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.discard_pile = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 20),
        ]
        .into();
        let choice = PendingChoice::GridSelect {
            source_pile: PileType::Discard,
            candidate_lineages: crate::test_support::scenario_lineages([10, 20]),
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: GridSelectReason::MoveToDrawPile,
        };
        session.engine_state = EngineState::PendingChoice(choice.clone());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::PendingChoice(choice),
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));

        let candidates = decision_candidates(&session);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].id, "select");
        assert!(candidates[0]
            .label
            .contains("Submit selection with `select <idx...>`"));
        assert!(candidates[0].action.executable_input().is_none());
    }

    #[test]
    fn singing_bowl_card_reward_candidate_is_visible_and_executable() {
        let mut session = RunControlSession::new(Default::default());
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::SingingBowl));
        let mut reward = RewardState::new();
        reward.pending_card_choice = Some(vec![crate::state::rewards::RewardCard::new(
            CardId::PommelStrike,
            0,
        )]);
        reward.pending_card_reward_index = Some(0);
        session.engine_state = EngineState::RewardScreen(reward);

        let candidates = decision_candidates(&session);

        let bowl = candidates
            .iter()
            .find(|candidate| candidate.id == "bowl")
            .expect("Singing Bowl should appear as a visible card reward option");
        assert_eq!(
            bowl.action.executable_input(),
            Some(ClientInput::SelectCard(1))
        );
        assert!(bowl.label.contains("gain 2 max HP"));
    }

    #[test]
    fn singing_bowl_unopened_card_reward_candidate_is_visible_as_command() {
        let mut session = RunControlSession::new(Default::default());
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::SingingBowl));
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Card {
            cards: vec![crate::state::rewards::RewardCard::new(
                CardId::PommelStrike,
                0,
            )],
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let candidates = decision_candidates(&session);

        let bowl = candidates
            .iter()
            .find(|candidate| candidate.id == "bowl")
            .expect("Singing Bowl should be visible next to unopened card reward item");
        assert!(bowl.action.executable_input().is_none());
        assert!(bowl.label.contains("gain 2 max HP"));
    }

    #[test]
    fn reward_overlay_uses_return_candidate_instead_of_map_preview_skip() {
        let mut session = RunControlSession::new(Default::default());
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Gold { amount: 25 }];
        session.engine_state = EngineState::reward_overlay(
            reward,
            EngineState::Shop(crate::state::shop::ShopState::new()),
        );

        let candidates = decision_candidates(&session);

        assert!(candidates.iter().any(|candidate| {
            candidate.id == "back"
                && candidate.label == "Return to shop"
                && candidate.action.executable_input() == Some(ClientInput::Cancel)
        }));
        assert!(!candidates.iter().any(|candidate| candidate.id == "skip"));
    }

    #[test]
    fn full_potion_belt_can_discard_before_claiming_potion_reward() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::Elixir, 10)),
            Some(Potion::new(PotionId::FairyPotion, 20)),
            Some(Potion::new(PotionId::GamblersBrew, 30)),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::CultistPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let candidates = decision_candidates(&session);
        assert!(
            !candidates.iter().any(|candidate| {
                candidate.action.executable_input() == Some(ClientInput::ClaimReward(0))
            }),
            "a full belt must not expose a potion claim that leaves the state unchanged"
        );
        let discard = candidates
            .iter()
            .find(|candidate| candidate.id == "discard-potion-0")
            .expect("a full belt should expose discard actions before a potion reward");
        assert_eq!(
            discard.action.executable_input(),
            Some(ClientInput::DiscardPotion(0))
        );
        assert_eq!(
            discard.key,
            Some(DecisionCandidateKey::RunPotionDiscard {
                slot: 0,
                potion: PotionId::Elixir,
                uuid: 10,
            })
        );

        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::DiscardPotion(0)))
            .expect("discarding from the reward screen should be executable");
        assert!(session.run_state.potions[0].is_none());
        let EngineState::RewardScreen(reward) = &session.engine_state else {
            panic!("discarding a potion must preserve the reward screen");
        };
        assert!(matches!(
            reward.items.as_slice(),
            [RewardItem::Potion {
                potion_id: PotionId::CultistPotion
            }]
        ));
    }

    #[test]
    fn claimable_potion_reward_carries_typed_reward_identity() {
        let mut session = RunControlSession::new(Default::default());
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::FruitJuice,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let claim = decision_candidates(&session)
            .into_iter()
            .find(|candidate| {
                candidate.action.executable_input() == Some(ClientInput::ClaimReward(0))
            })
            .expect("an empty potion slot should expose the claim");

        assert_eq!(
            claim.key,
            Some(DecisionCandidateKey::RewardPotionClaim {
                reward_item_index: 0,
                potion: PotionId::FruitJuice,
            })
        );
    }

    #[test]
    fn sozu_potion_claim_remains_executable_because_it_consumes_the_reward() {
        let mut session = RunControlSession::new(Default::default());
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::Sozu));
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::Elixir, 10)),
            Some(Potion::new(PotionId::FairyPotion, 20)),
            Some(Potion::new(PotionId::GamblersBrew, 30)),
        ];
        let original_potions = session.run_state.potions.clone();
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::CultistPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        decision_candidates(&session)
            .into_iter()
            .find(|candidate| {
                candidate.action.executable_input() == Some(ClientInput::ClaimReward(0))
            })
            .expect("Sozu claim consumes the blocked potion reward and must remain executable");

        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::ClaimReward(0)))
            .expect("the Sozu potion claim should change the reward surface");
        assert_eq!(session.run_state.potions, original_potions);
        assert!(matches!(
            &session.engine_state,
            EngineState::RewardScreen(reward) if reward.items.is_empty()
        ));
        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::Proceed))
            .expect("the empty reward page should expose a separate Proceed");
        assert!(matches!(&session.engine_state,
            EngineState::MapOverlay { return_state }
                if matches!(return_state.as_ref(), EngineState::RewardScreen(reward) if reward.items.is_empty())));
        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::Cancel))
            .expect("the original empty reward page remains reachable");
        assert!(matches!(&session.engine_state,
            EngineState::RewardScreen(reward) if reward.items.is_empty()));
    }

    #[test]
    fn full_potion_belt_exposes_fruit_juice_use_before_potion_reward() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::FruitJuice, 10)),
            Some(Potion::new(PotionId::FairyPotion, 20)),
            Some(Potion::new(PotionId::GamblersBrew, 30)),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::CultistPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let candidates = decision_candidates(&session);
        let use_fruit_juice = candidates
            .iter()
            .find(|candidate| candidate.id == "use-potion-0")
            .expect("a full belt should expose immediate Fruit Juice realization");
        assert_eq!(
            use_fruit_juice.action.executable_input(),
            Some(ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            })
        );
        assert_eq!(
            use_fruit_juice.key,
            Some(DecisionCandidateKey::RunPotionUse {
                slot: 0,
                potion: PotionId::FruitJuice,
                uuid: 10,
            })
        );
    }

    #[test]
    fn map_exposes_and_executes_immediate_fruit_juice_use() {
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::MapNavigation;
        session.run_state.potions = vec![Some(Potion::new(PotionId::FruitJuice, 10))];

        let candidates = decision_candidates(&session);
        let use_fruit_juice = candidates
            .iter()
            .find(|candidate| candidate.id == "use-run-potion-0")
            .expect("map should expose immediate Fruit Juice realization");
        assert_eq!(
            use_fruit_juice.key,
            Some(DecisionCandidateKey::RunPotionUse {
                slot: 0,
                potion: PotionId::FruitJuice,
                uuid: 10,
            })
        );

        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::UsePotion {
                potion_index: 0,
                target: None,
            }))
            .expect("map Fruit Juice use should be executable");
        assert_eq!(session.run_state.current_hp, 85);
        assert_eq!(session.run_state.max_hp, 85);
        assert!(session.run_state.potions[0].is_none());
    }

    #[test]
    fn boss_reward_with_full_potion_belt_can_proceed_without_discarding() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.pending_boss_reward = true;
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::Elixir, 10)),
            Some(Potion::new(PotionId::FairyPotion, 20)),
            Some(Potion::new(PotionId::GamblersBrew, 30)),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::RegenPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let candidates = decision_candidates(&session);
        let proceed = candidates
            .iter()
            .find(|candidate| candidate.id == "skip")
            .expect("boss rewards must expose a proceed action even when a potion is unclaimed");

        assert_eq!(proceed.label, "Proceed to boss relic");
        assert_eq!(
            proceed.action.executable_input(),
            Some(ClientInput::Proceed)
        );
    }

    #[test]
    fn boss_relic_reward_with_full_potion_belt_proceeds_to_next_act() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.pending_boss_act_transition = true;
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::Elixir, 10)),
            Some(Potion::new(PotionId::FairyPotion, 20)),
            Some(Potion::new(PotionId::GamblersBrew, 30)),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::RegenPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let candidates = decision_candidates(&session);
        let proceed = candidates
            .iter()
            .find(|candidate| candidate.id == "skip")
            .expect("boss relic rewards must permit leaving an unclaimable potion behind");

        assert_eq!(proceed.label, "Proceed to next Act");
        assert_eq!(
            proceed.action.executable_input(),
            Some(ClientInput::Proceed)
        );
    }
}
