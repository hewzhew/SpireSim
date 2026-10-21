use std::collections::{BTreeMap, BTreeSet};
use sts_oracle_run_control::{
    ai::planner_core::PlannerMapRoom,
    content::events::neow::{choice_identity, NeowDrawback, NeowRewardType},
    eval::run_control::{
        build_decision_surface, DecisionCandidateKey, RunControlSession, RunDecisionAction,
    },
    runtime::combat::CardLineageId,
    state::{
        core::{ClientInput, EngineState, HandSelectReason, PendingChoice, RunPendingChoiceReason},
        events::{EventActionKind, EventId},
        selection::{DomainEventSource, SelectionResolution, SelectionScope, SelectionTargetRef},
    },
};
use sts_parity_lockstep::{
    canonical_selection_indices_v1, natural_campfire_option_v1, natural_card_reward_choice_v1,
    natural_deck_card_choice_v1, natural_event_option_v1, natural_hand_card_choice_v1,
    natural_map_node_id_v1, natural_reward_choice_v1, DecisionSurfaceV1, NaturalRewardCardV1,
    NaturalRunRewardsV2, SemanticActionV1, SemanticChoiceV1, SemanticScalarV1,
};

#[derive(Clone, Debug)]
pub(crate) struct ResolvedRustActionV1 {
    pub semantic: SemanticActionV1,
    pub execution: RustActionExecutionV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RustActionExecutionV1 {
    Candidate {
        candidate_id: String,
        input: ClientInput,
    },
    DirectInput {
        input: ClientInput,
    },
    BeginCampfireSmith,
}

impl ResolvedRustActionV1 {
    pub(crate) fn candidate(
        semantic: SemanticActionV1,
        candidate_id: String,
        input: ClientInput,
    ) -> Self {
        Self {
            semantic,
            execution: RustActionExecutionV1::Candidate {
                candidate_id,
                input,
            },
        }
    }
}

pub(crate) fn resolved_actions_v1(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, crate::bootstrap::BoundCardSourceV1>,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    match &session.engine_state {
        EngineState::EventRoom
            if session.run_state.event_state.as_ref().map(|event| event.id)
                == Some(EventId::Neow) =>
        {
            neow_actions(session)
        }
        EngineState::EventRoom => event_actions(session),
        EngineState::MapNavigation | EngineState::MapOverlay { .. } => map_actions(session),
        EngineState::CombatPlayerTurn => combat_actions(session, bindings),
        EngineState::PendingChoice(PendingChoice::DiscoverySelect(_)) => discovery_actions(session),
        EngineState::PendingChoice(PendingChoice::CardRewardSelect { .. })
            if is_supported_codex(session) =>
        {
            codex_actions(session)
        }
        EngineState::PendingChoice(PendingChoice::HandSelect { .. }) => {
            hand_select_actions(session, bindings)
        }
        EngineState::PendingChoice(
            PendingChoice::GridSelect { .. } | PendingChoice::ScrySelect { .. },
        ) if is_supported_combat_grid_select(session) => combat_grid_actions(session, bindings),
        EngineState::RewardScreen(reward)
        | EngineState::RewardOverlay {
            reward_state: reward,
            ..
        } if reward.pending_card_choice.is_some() => card_reward_actions(session),
        EngineState::RewardScreen(_) | EngineState::RewardOverlay { .. } => reward_actions(session),
        EngineState::TreasureRoom(_) => treasure_actions(session),
        EngineState::BossRelicSelect(_) => boss_relic_actions(session),
        EngineState::DoubleBossComplete => {
            boss_completion_actions(session, DecisionSurfaceV1::DoubleBoss)
        }
        EngineState::Act3VictoryComplete => {
            boss_completion_actions(session, DecisionSurfaceV1::Act3Victory)
        }
        EngineState::TrueVictoryComplete => {
            boss_completion_actions(session, DecisionSurfaceV1::TrueVictory)
        }
        EngineState::GameOver(sts_oracle_run_control::state::core::RunResult::Victory)
            if is_heart_victory(session) =>
        {
            Ok(Vec::new())
        }
        EngineState::Campfire => campfire_actions(session, bindings),
        EngineState::Shop(_) => crate::shop::actions(session, bindings),
        EngineState::RunPendingChoice(_) => run_pending_choice_actions(session, bindings),
        _ => Err(format!(
            "Rust lockstep v1 does not yet project boundary {:?}",
            session.engine_state
        )),
    }
}

pub(crate) fn is_heart_victory(session: &RunControlSession) -> bool {
    matches!(
        session.engine_state,
        EngineState::GameOver(sts_oracle_run_control::state::core::RunResult::Victory)
    ) && session.run_state.act_num == 4
        && (
            session.run_state.map.current_x,
            session.run_state.map.current_y,
        ) == (3, 4)
        && session.run_state.current_hp > 0
        && session.active_combat.is_none()
}

fn boss_completion_actions(
    session: &RunControlSession,
    surface: DecisionSurfaceV1,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    let mut actions = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(RunDecisionAction::Input(input)) = candidate.action.executable_action() else {
            return Err("Rust boss completion has a non-input candidate".to_owned());
        };
        let semantic =
            if let Some(discard) = run_potion_semantic(session, candidate.key.as_ref(), &input)? {
                discard
            } else if input == ClientInput::Proceed {
                SemanticActionV1::Proceed { surface }
            } else {
                return Err("Rust boss completion exposed an unrelated input".to_owned());
            };
        actions.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            input,
        ));
    }
    Ok(actions)
}

fn boss_relic_actions(session: &RunControlSession) -> Result<Vec<ResolvedRustActionV1>, String> {
    let EngineState::BossRelicSelect(state) = &session.engine_state else {
        unreachable!()
    };
    let mut occurrences = BTreeMap::new();
    let choices = state
        .relics
        .iter()
        .map(|id| {
            let id = sts_java_parity_protocol::canonical_game_id(id.stable_id());
            let occurrence = occurrences.entry(id.clone()).or_insert(0);
            let choice = sts_parity_lockstep::natural_boss_relic_choice_v1(&id, *occurrence);
            *occurrence += 1;
            choice
        })
        .collect::<Vec<_>>();
    build_decision_surface(session)
        .view
        .candidates
        .into_iter()
        .filter_map(|candidate| {
            let input = candidate.action.executable_input()?;
            Some((candidate.id, candidate.key, input))
        })
        .map(|(id, key, input)| {
            if let Some(discard) = run_potion_semantic(session, key.as_ref(), &input)? {
                return Ok(ResolvedRustActionV1::candidate(discard, id, input));
            }
            let semantic = match &input {
                ClientInput::SubmitRelicChoice(index) => SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::BossRelic,
                    choice: choices
                        .get(*index)
                        .ok_or("boss relic index exceeds offers")?
                        .clone(),
                },
                ClientInput::Cancel => SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::BossRelic,
                },
                _ => return Err("boss relic boundary exposed an unrelated action".to_owned()),
            };
            Ok(ResolvedRustActionV1::candidate(semantic, id, input))
        })
        .collect()
}

pub(crate) fn is_supported_discovery(session: &RunControlSession) -> bool {
    matches!(&session.engine_state, EngineState::PendingChoice(PendingChoice::DiscoverySelect(choice))
        if matches!(choice.amount, 1 | 2) && choice.cards.len() == 3)
        && session.active_combat.as_ref().is_some_and(|active| {
            active.engine_state == session.engine_state
                && active.combat_state.turn.counters.discovery_cost_for_turn == Some(0)
        })
}

pub(crate) fn is_supported_codex(session: &RunControlSession) -> bool {
    matches!(&session.engine_state,
        EngineState::PendingChoice(PendingChoice::CardRewardSelect {
            cards,
            destination: sts_oracle_run_control::runtime::action::CardDestination::DrawPileRandom,
            can_skip: true,
        }) if cards.len() == 3)
        && session
            .active_combat
            .as_ref()
            .is_some_and(|active| active.engine_state == session.engine_state)
}

pub(crate) fn codex_state(
    session: &RunControlSession,
) -> Result<sts_parity_lockstep::NaturalCombatCodexV1, String> {
    if !is_supported_codex(session) {
        return Err(
            "Rust Codex requires three offers, random draw-pile insertion and skip".to_owned(),
        );
    }
    let EngineState::PendingChoice(PendingChoice::CardRewardSelect {
        cards, can_skip, ..
    }) = &session.engine_state
    else {
        unreachable!()
    };
    Ok(sts_parity_lockstep::NaturalCombatCodexV1 {
        cards: cards
            .iter()
            .map(|id| NaturalRewardCardV1 {
                card_id: sts_java_parity_protocol::canonical_game_id(id.stable_id()),
                upgrades: 0,
            })
            .collect(),
        can_skip: *can_skip,
    })
}

fn codex_actions(session: &RunControlSession) -> Result<Vec<ResolvedRustActionV1>, String> {
    let state = codex_state(session)?;
    combat_offer_actions(
        session,
        &state.cards,
        state.can_skip,
        DecisionSurfaceV1::CombatCodex,
        sts_parity_lockstep::natural_codex_choice_v1,
    )
}

pub(crate) fn discovery_state(
    session: &RunControlSession,
) -> Result<sts_parity_lockstep::NaturalCombatDiscoveryV1, String> {
    if !is_supported_discovery(session) {
        return Err("Rust Discovery has an unsupported choice or cost contract".to_owned());
    }
    let EngineState::PendingChoice(PendingChoice::DiscoverySelect(choice)) = &session.engine_state
    else {
        unreachable!()
    };
    Ok(sts_parity_lockstep::NaturalCombatDiscoveryV1 {
        cards: choice
            .cards
            .iter()
            .map(|id| NaturalRewardCardV1 {
                card_id: sts_java_parity_protocol::canonical_game_id(id.stable_id()),
                upgrades: 0,
            })
            .collect(),
        amount: choice.amount,
        colorless: choice.colorless,
        card_type: choice
            .card_type
            .map(|kind| format!("{kind:?}").to_lowercase()),
        cost_for_turn: 0,
        can_skip: choice.can_skip,
    })
}

fn discovery_actions(session: &RunControlSession) -> Result<Vec<ResolvedRustActionV1>, String> {
    let state = discovery_state(session)?;
    combat_offer_actions(
        session,
        &state.cards,
        state.can_skip,
        DecisionSurfaceV1::CombatDiscovery,
        sts_parity_lockstep::natural_discovery_choice_v1,
    )
}

fn combat_offer_actions(
    session: &RunControlSession,
    cards: &[NaturalRewardCardV1],
    can_skip: bool,
    surface: DecisionSurfaceV1,
    make_choice: fn(&NaturalRewardCardV1, u32) -> SemanticChoiceV1,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    let combat = &session
        .active_combat
        .as_ref()
        .expect("validated combat offer")
        .combat_state;
    let mut occurrences = BTreeMap::new();
    let choices = cards
        .iter()
        .map(|card| {
            let occurrence = occurrences.entry(card.card_id.clone()).or_insert(0u32);
            let choice = make_choice(card, *occurrence);
            *occurrence += 1;
            choice
        })
        .collect::<Vec<_>>();
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        let semantic = match &input {
            ClientInput::SubmitDiscoverChoice(index) => SemanticActionV1::Select {
                surface,
                choice: choices
                    .get(*index)
                    .ok_or_else(|| "Rust combat offer action exceeds its cards".to_owned())?
                    .clone(),
            },
            ClientInput::Cancel if can_skip => SemanticActionV1::Cancel { surface },
            ClientInput::DiscardPotion(slot) => combat_potion_discard_semantic(combat, *slot)?,
            _ => return Err("Rust combat offer exposes an unrelated action".to_owned()),
        };
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            input,
        ));
    }
    if resolved
        .iter()
        .filter(|action| !matches!(action.semantic, SemanticActionV1::DiscardPotion { .. }))
        .count()
        != choices.len() + usize::from(can_skip)
    {
        return Err("Rust combat offer action space is incomplete".to_owned());
    }
    Ok(resolved)
}

fn hand_select_actions(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, crate::bootstrap::BoundCardSourceV1>,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    let EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages,
        min_cards,
        max_cards,
        can_cancel,
        reason,
    }) = &session.engine_state
    else {
        return Err("Rust hand selection requires a combat pending choice".to_owned());
    };
    let allowed_counts = supported_hand_select_counts(
        candidate_lineages.len(),
        *min_cards,
        *max_cards,
        *can_cancel,
        *reason,
    )?;
    let active = session
        .active_combat
        .as_ref()
        .filter(|active| active.engine_state == session.engine_state)
        .ok_or_else(|| "Rust hand selection omitted its active combat state".to_owned())?;

    let surface = build_decision_surface(session);
    let mut submit_candidates = surface.view.candidates.iter().filter(|candidate| {
        matches!(
            candidate.key,
            Some(DecisionCandidateKey::SelectionSubmit { .. })
        )
    });
    let submit = submit_candidates
        .next()
        .ok_or_else(|| "Rust hand selection omits its typed submit surface".to_owned())?;
    if submit_candidates.next().is_some() {
        return Err("Rust hand selection repeats its typed submit surface".to_owned());
    }
    if submit.action.executable_input().is_some()
        || !matches!(
            submit.key.as_ref(),
            Some(DecisionCandidateKey::SelectionSubmit {
                scope: SelectionScope::Hand,
                reason: surface_reason,
                min_choices,
                max_choices,
                item_count,
            }) if *surface_reason == (*reason).into()
                && *min_choices == usize::from(*min_cards)
                && *max_choices == usize::from(*max_cards)
                && *item_count == candidate_lineages.len()
        )
    {
        return Err("Rust hand selection has an invalid typed submit contract".to_owned());
    }

    let mut seen_lineages = BTreeSet::new();
    let mut semantics = BTreeSet::new();
    let mut bound_candidates = Vec::with_capacity(candidate_lineages.len());
    for lineage in candidate_lineages {
        if !seen_lineages.insert(*lineage) {
            return Err("Rust hand selection repeats one candidate lineage".to_owned());
        }
        let card = active
            .combat_state
            .zones
            .hand
            .iter()
            .find(|card| card.lineage == *lineage)
            .ok_or_else(|| "Rust hand selection candidate is absent from the hand".to_owned())?;
        bound_candidates.push((
            *lineage,
            natural_hand_card_choice_v1(
                require_card_binding(bindings, *lineage)?,
                &sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
                card.upgrades,
            )?,
        ));
    }
    let selection_indices = canonical_selection_indices_v1(bound_candidates.len(), allowed_counts)?;
    let mut resolved =
        Vec::with_capacity(selection_indices.len() + active.combat_state.entities.potions.len());
    for indices in selection_indices {
        let semantic = SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::HandSelect,
            choices: indices
                .iter()
                .map(|index| bound_candidates[*index].1.clone())
                .collect(),
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Rust hand-selection action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Rust hand selection repeats one semantic submission".to_owned());
        }
        resolved.push(ResolvedRustActionV1 {
            semantic,
            execution: RustActionExecutionV1::DirectInput {
                input: ClientInput::SubmitSelection(SelectionResolution {
                    scope: SelectionScope::Hand,
                    selected: indices
                        .iter()
                        .map(|index| SelectionTargetRef::CardLineage(bound_candidates[*index].0))
                        .collect(),
                }),
            },
        });
    }
    for candidate in surface.view.candidates {
        let Some(ClientInput::DiscardPotion(slot)) = candidate.action.executable_input() else {
            continue;
        };
        let semantic = combat_potion_discard_semantic(&active.combat_state, slot)?;
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Rust hand-selection action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Rust hand selection repeats one potion discard action".to_owned());
        }
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            ClientInput::DiscardPotion(slot),
        ));
    }
    Ok(resolved)
}

pub(crate) fn is_supported_hand_select(session: &RunControlSession) -> bool {
    let EngineState::PendingChoice(PendingChoice::HandSelect {
        candidate_lineages,
        min_cards,
        max_cards,
        can_cancel,
        reason,
    }) = &session.engine_state
    else {
        return false;
    };
    supported_hand_select_counts(
        candidate_lineages.len(),
        *min_cards,
        *max_cards,
        *can_cancel,
        *reason,
    )
    .is_ok()
}

pub(crate) fn is_exhaust_hand_selection(session: &RunControlSession) -> bool {
    matches!(
        session.engine_state,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::Exhaust,
            ..
        })
    ) && is_supported_hand_select(session)
}

pub(crate) fn is_discard_hand_selection(session: &RunControlSession) -> bool {
    matches!(
        session.engine_state,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::Discard,
            ..
        })
    ) && is_supported_hand_select(session)
}

pub(crate) fn is_put_on_draw_pile_selection(session: &RunControlSession) -> bool {
    matches!(
        session.engine_state,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::PutOnDrawPile | HandSelectReason::Setup,
            ..
        })
    ) && is_supported_hand_select(session)
}

pub(crate) fn is_potion_exhaust_selection(session: &RunControlSession) -> bool {
    matches!(
        session.engine_state,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::PotionExhaust { .. },
            ..
        })
    ) && is_supported_hand_select(session)
}

pub(crate) fn is_upgrade_hand_selection(session: &RunControlSession) -> bool {
    matches!(
        session.engine_state,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::Upgrade,
            ..
        })
    ) && is_supported_hand_select(session)
}

pub(crate) fn is_copy_hand_selection(session: &RunControlSession) -> bool {
    matches!(
        session.engine_state,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::Copy { amount: 2 | 3 },
            ..
        })
    ) && is_supported_hand_select(session)
}

pub(crate) fn is_retain_hand_selection(session: &RunControlSession) -> bool {
    matches!(
        session.engine_state,
        EngineState::PendingChoice(PendingChoice::HandSelect {
            reason: HandSelectReason::Retain,
            ..
        })
    ) && is_supported_hand_select(session)
}

pub(crate) fn is_supported_combat_grid_select(session: &RunControlSession) -> bool {
    combat_grid_kind(session).is_some()
}

pub(crate) fn combat_grid_kind(session: &RunControlSession) -> Option<&'static str> {
    use sts_oracle_run_control::state::{GridSelectReason, PileType};
    match &session.engine_state {
        EngineState::PendingChoice(PendingChoice::ScrySelect {
            cards,
            card_lineages,
        }) if !cards.is_empty() && cards.len() == card_lineages.len() && cards.len() <= 10 => {
            Some("scry")
        }
        EngineState::PendingChoice(PendingChoice::GridSelect {
            source_pile,
            candidate_lineages,
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason,
        }) if !candidate_lineages.is_empty() => match (source_pile, reason) {
            (PileType::Exhaust, GridSelectReason::Exhume { upgrade: false }) => Some("exhume"),
            (PileType::Discard, GridSelectReason::DiscardToHand) => Some("discard_to_hand"),
            (PileType::Discard, GridSelectReason::DiscardToHandNoCostChange) => {
                Some("discard_to_hand_no_cost_change")
            }
            (PileType::Discard, GridSelectReason::DiscardToHandRetain) => {
                Some("discard_to_hand_retain")
            }
            (PileType::Discard, GridSelectReason::MoveToDrawPile) => Some("discard_to_draw"),
            (PileType::Draw, GridSelectReason::SkillFromDeckToHand) => {
                Some("skill_from_deck_to_hand")
            }
            (PileType::Draw, GridSelectReason::AttackFromDeckToHand) => {
                Some("attack_from_deck_to_hand")
            }
            (PileType::Draw, GridSelectReason::DrawPileToHand) => Some("draw_to_hand"),
            (PileType::Draw, GridSelectReason::Omniscience { play_amount: 2 }) => {
                Some("omniscience")
            }
            _ => None,
        },
        _ => None,
    }
}

fn combat_grid_actions(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, crate::bootstrap::BoundCardSourceV1>,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    if let EngineState::PendingChoice(PendingChoice::ScrySelect {
        cards,
        card_lineages,
    }) = &session.engine_state
    {
        return scry_actions(session, bindings, cards, card_lineages);
    }
    let EngineState::PendingChoice(PendingChoice::GridSelect {
        candidate_lineages, ..
    }) = &session.engine_state
    else {
        unreachable!()
    };
    let active = session
        .active_combat
        .as_ref()
        .filter(|active| active.engine_state == session.engine_state)
        .ok_or("Rust combat grid selection omitted its active combat")?;
    let kind = combat_grid_kind(session).ok_or("unsupported combat grid selection")?;
    let cards = if kind == "exhume" {
        active
            .combat_state
            .zones
            .exhaust_pile
            .iter()
            .filter(|card| card.id != sts_oracle_run_control::content::cards::CardId::Exhume)
            .collect::<Vec<_>>()
    } else if matches!(
        kind,
        "discard_to_hand"
            | "discard_to_hand_no_cost_change"
            | "discard_to_hand_retain"
            | "discard_to_draw"
    ) {
        active
            .combat_state
            .zones
            .discard_pile
            .iter()
            .collect::<Vec<_>>()
    } else {
        use sts_oracle_run_control::content::cards::{get_card_definition, CardType};
        let card_type = match kind {
            "skill_from_deck_to_hand" => Some(CardType::Skill),
            "attack_from_deck_to_hand" => Some(CardType::Attack),
            "omniscience" | "draw_to_hand" => None,
            _ => unreachable!(),
        };
        active
            .combat_state
            .zones
            .draw_pile
            .iter()
            .filter(|card| {
                card_type.is_none_or(|kind| get_card_definition(card.id).card_type == kind)
            })
            .collect::<Vec<_>>()
    };
    if cards
        .iter()
        .map(|card| card.lineage)
        .collect::<BTreeSet<_>>()
        != candidate_lineages.iter().copied().collect::<BTreeSet<_>>()
        || candidate_lineages.iter().collect::<BTreeSet<_>>().len() != cards.len()
    {
        return Err(
            "Rust combat grid candidates are not exactly the eligible source cards".to_owned(),
        );
    }
    let mut actions = cards
        .into_iter()
        .map(|card| {
            Ok(ResolvedRustActionV1 {
                semantic: SemanticActionV1::SubmitSelection {
                    surface: DecisionSurfaceV1::GridSelect,
                    choices: vec![(if kind == "exhume" {
                        sts_parity_lockstep::natural_exhaust_card_choice_v1
                    } else if matches!(
                        kind,
                        "discard_to_hand"
                            | "discard_to_hand_no_cost_change"
                            | "discard_to_hand_retain"
                            | "discard_to_draw"
                    ) {
                        sts_parity_lockstep::natural_discard_card_choice_v1
                    } else {
                        sts_parity_lockstep::natural_draw_card_choice_v1
                    })(
                        require_card_binding(bindings, card.lineage)?,
                        &sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
                        card.upgrades,
                    )?],
                },
                execution: RustActionExecutionV1::DirectInput {
                    input: ClientInput::SubmitSelection(SelectionResolution {
                        scope: SelectionScope::Grid,
                        selected: vec![SelectionTargetRef::CardLineage(card.lineage)],
                    }),
                },
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    for candidate in build_decision_surface(session).view.candidates {
        if let Some(input @ ClientInput::DiscardPotion(slot)) = candidate.action.executable_input()
        {
            let semantic = combat_potion_discard_semantic(&active.combat_state, slot)?;
            actions.push(ResolvedRustActionV1::candidate(
                semantic,
                candidate.id,
                input,
            ));
        }
    }
    Ok(actions)
}

fn scry_actions(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, crate::bootstrap::BoundCardSourceV1>,
    ids: &[sts_oracle_run_control::content::cards::CardId],
    lineages: &[CardLineageId],
) -> Result<Vec<ResolvedRustActionV1>, String> {
    if combat_grid_kind(session) != Some("scry") {
        return Err("Rust Scry selection has an unsupported size or identity contract".into());
    }
    let active = session
        .active_combat
        .as_ref()
        .filter(|active| active.engine_state == session.engine_state)
        .ok_or("Rust Scry omitted its active combat")?;
    let cards = active
        .combat_state
        .zones
        .draw_pile
        .iter()
        .take(lineages.len())
        .collect::<Vec<_>>();
    if cards.len() != lineages.len()
        || cards
            .iter()
            .map(|card| card.lineage)
            .ne(lineages.iter().copied())
        || cards.iter().map(|card| card.id).ne(ids.iter().copied())
        || lineages.iter().collect::<BTreeSet<_>>().len() != lineages.len()
    {
        return Err("Rust Scry candidates differ from the exact top draw-pile objects".into());
    }
    let choices = cards
        .iter()
        .map(|card| {
            sts_parity_lockstep::natural_draw_card_choice_v1(
                require_card_binding(bindings, card.lineage)?,
                &sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
                card.upgrades,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut actions = canonical_selection_indices_v1(cards.len(), 0..=cards.len())?
        .into_iter()
        .map(|indices| ResolvedRustActionV1 {
            semantic: SemanticActionV1::SubmitSelection {
                surface: DecisionSurfaceV1::GridSelect,
                choices: indices
                    .iter()
                    .map(|index| choices[*index].clone())
                    .collect(),
            },
            execution: RustActionExecutionV1::DirectInput {
                input: ClientInput::SubmitScryDiscard(indices),
            },
        })
        .collect::<Vec<_>>();
    for candidate in build_decision_surface(session).view.candidates {
        if let Some(input @ ClientInput::DiscardPotion(slot)) = candidate.action.executable_input()
        {
            actions.push(ResolvedRustActionV1::candidate(
                combat_potion_discard_semantic(&active.combat_state, slot)?,
                candidate.id,
                input,
            ));
        }
    }
    Ok(actions)
}

fn supported_hand_select_counts(
    candidate_count: usize,
    min_cards: u8,
    max_cards: u8,
    can_cancel: bool,
    reason: HandSelectReason,
) -> Result<Vec<usize>, String> {
    match reason {
        HandSelectReason::Copy { amount: 2 | 3 }
            if min_cards == 1 && max_cards == 1 && !can_cancel && candidate_count > 1 =>
        {
            Ok(vec![1])
        }
        HandSelectReason::Discard
            if min_cards > 0
                && min_cards == max_cards
                && !can_cancel
                && usize::from(max_cards) <= candidate_count
                && candidate_count <= 10 =>
        {
            Ok(vec![usize::from(max_cards)])
        }
        HandSelectReason::Exhaust
        | HandSelectReason::PutOnDrawPile
        | HandSelectReason::Setup
        | HandSelectReason::Upgrade
            if min_cards == 1 && max_cards == 1 && !can_cancel && candidate_count > 0 =>
        {
            Ok(vec![1])
        }
        HandSelectReason::Exhaust
            if min_cards == 0
                && can_cancel
                && (1..=5).contains(&max_cards)
                && usize::from(max_cards) <= candidate_count
                && candidate_count <= 10 =>
        {
            Ok((0..=usize::from(max_cards)).collect())
        }
        HandSelectReason::Retain
            if min_cards == 0
                && can_cancel
                && max_cards > 0
                && usize::from(max_cards) <= candidate_count
                && candidate_count <= 10 =>
        {
            Ok((0..=usize::from(max_cards)).collect())
        }
        HandSelectReason::GamblingChip | HandSelectReason::PotionExhaust { .. }
            if min_cards == 0
                && usize::from(max_cards) == candidate_count
                && can_cancel
                && candidate_count <= 10 =>
        {
            Ok((0..=candidate_count).collect())
        }
        _ => Err("Rust hand selection is outside the supported selection profiles".to_owned()),
    }
}

#[cfg(test)]
#[test]
fn retain_selection_accepts_zero_through_the_available_stack_limit() {
    for candidates in 1..=10 {
        for max in 1..=candidates {
            assert_eq!(
                supported_hand_select_counts(
                    candidates,
                    0,
                    max as u8,
                    true,
                    HandSelectReason::Retain
                )
                .unwrap(),
                (0..=max).collect::<Vec<_>>()
            );
        }
    }
    for (candidates, min, max, cancel) in [
        (2, 1, 1, true),
        (2, 0, 1, false),
        (2, 0, 0, true),
        (2, 0, 3, true),
        (11, 0, 1, true),
    ] {
        assert!(supported_hand_select_counts(
            candidates,
            min,
            max,
            cancel,
            HandSelectReason::Retain
        )
        .is_err());
    }
}

#[cfg(test)]
#[test]
fn dual_wield_selection_requires_the_original_single_pick_copy_counts() {
    for amount in [2, 3] {
        let reason = HandSelectReason::Copy { amount };
        assert_eq!(
            supported_hand_select_counts(2, 1, 1, false, reason).unwrap(),
            vec![1]
        );
        for (candidates, min, max, cancel) in [
            (0, 1, 1, false),
            (1, 1, 1, false),
            (2, 0, 1, false),
            (2, 1, 2, false),
            (2, 1, 1, true),
        ] {
            assert!(supported_hand_select_counts(candidates, min, max, cancel, reason).is_err());
        }
    }
    for amount in [0, 1, 4] {
        assert!(
            supported_hand_select_counts(2, 1, 1, false, HandSelectReason::Copy { amount })
                .is_err()
        );
    }
}

#[cfg(test)]
#[test]
fn optional_exhaust_enumerates_empty_single_and_bounded_subsets() {
    for candidate_count in 1..=10 {
        for limit in [3, 5] {
            let maximum = candidate_count.min(limit);
            assert_eq!(
                supported_hand_select_counts(
                    candidate_count,
                    0,
                    maximum as u8,
                    true,
                    HandSelectReason::Exhaust
                )
                .unwrap(),
                (0..=maximum).collect::<Vec<_>>()
            );
        }
    }
    for (candidates, min, max, cancel) in [
        (4, 0, 5, true),
        (6, 0, 6, true),
        (4, 1, 3, true),
        (4, 0, 3, false),
    ] {
        assert!(supported_hand_select_counts(
            candidates,
            min,
            max,
            cancel,
            HandSelectReason::Exhaust
        )
        .is_err());
    }
}

fn treasure_actions(session: &RunControlSession) -> Result<Vec<ResolvedRustActionV1>, String> {
    let mut saw_open = false;
    let mut saw_proceed = false;
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        let semantic =
            if let Some(discard) = run_potion_semantic(session, candidate.key.as_ref(), &input)? {
                discard
            } else if input == ClientInput::OpenChest && !saw_open {
                saw_open = true;
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Treasure,
                    choice: SemanticChoiceV1::new("treasure_chest", "open", 0),
                }
            } else if input == ClientInput::Proceed && !saw_proceed {
                saw_proceed = true;
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::Treasure,
                }
            } else {
                return Err("Rust treasure boundary exposes an invalid primary action".to_owned());
            };
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            input,
        ));
    }
    if !saw_open || !saw_proceed {
        return Err("Rust treasure boundary must expose open and proceed actions".to_owned());
    }
    Ok(resolved)
}

fn campfire_actions(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, crate::bootstrap::BoundCardSourceV1>,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    let mut saw_smith = false;
    let mut semantics = BTreeSet::new();
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        if let Some(discard) = run_potion_semantic(session, candidate.key.as_ref(), &input)? {
            let key = serde_json::to_string(&discard)
                .map_err(|error| format!("failed to encode campfire action: {error}"))?;
            if !semantics.insert(key) {
                return Err("Rust campfire repeats one potion discard action".to_owned());
            }
            resolved.push(ResolvedRustActionV1::candidate(
                discard,
                candidate.id,
                input,
            ));
            continue;
        }
        let (semantic, execution) = match (candidate.key.as_ref(), &input) {
            (
                Some(DecisionCandidateKey::CampfireRest),
                ClientInput::CampfireOption(
                    sts_oracle_run_control::state::core::CampfireChoice::Rest,
                ),
            ) => (
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Campfire,
                    choice: natural_campfire_option_v1("rest")?,
                },
                RustActionExecutionV1::Candidate {
                    candidate_id: candidate.id,
                    input,
                },
            ),
            (
                Some(DecisionCandidateKey::CampfireRecall),
                ClientInput::CampfireOption(
                    sts_oracle_run_control::state::core::CampfireChoice::Recall,
                ),
            ) => (
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Campfire,
                    choice: natural_campfire_option_v1("recall")?,
                },
                RustActionExecutionV1::Candidate {
                    candidate_id: candidate.id,
                    input,
                },
            ),
            (
                Some(DecisionCandidateKey::CampfireSmith {
                    deck_index,
                    card_lineage,
                    card,
                    upgrades,
                }),
                ClientInput::CampfireOption(
                    sts_oracle_run_control::state::core::CampfireChoice::Smith(input_index),
                ),
            ) => {
                let actual = session
                    .run_state
                    .master_deck
                    .get(*deck_index)
                    .ok_or_else(|| {
                        "Rust campfire Smith action points outside the master deck".to_owned()
                    })?;
                if input_index != deck_index
                    || actual.lineage != *card_lineage
                    || actual.id != *card
                    || actual.upgrades != *upgrades
                    || require_card_binding(bindings, *card_lineage).is_err()
                {
                    return Err("Rust campfire Smith action has an invalid typed card".to_owned());
                }
                if saw_smith {
                    continue;
                }
                saw_smith = true;
                (
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::Campfire,
                        choice: natural_campfire_option_v1("smith")?,
                    },
                    RustActionExecutionV1::BeginCampfireSmith,
                )
            }
            (
                Some(DecisionCandidateKey::CampfireDig),
                ClientInput::CampfireOption(
                    sts_oracle_run_control::state::core::CampfireChoice::Dig,
                ),
            ) => (
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Campfire,
                    choice: natural_campfire_option_v1("dig")?,
                },
                RustActionExecutionV1::Candidate {
                    candidate_id: candidate.id,
                    input,
                },
            ),
            (Some(DecisionCandidateKey::CampfireLift), _)
            | (Some(DecisionCandidateKey::CampfireToke { .. }), _) => {
                return Err(
                    "Rust campfire exposes an option outside the current original-game protocol"
                        .to_owned(),
                )
            }
            _ => return Err("Rust campfire contains an invalid executable candidate".to_owned()),
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode campfire action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Rust campfire repeats one semantic action".to_owned());
        }
        resolved.push(ResolvedRustActionV1 {
            semantic,
            execution,
        });
    }
    if resolved.is_empty() {
        return Err("Rust campfire exposes no semantic actions".to_owned());
    }
    Ok(resolved)
}

fn event_actions(session: &RunControlSession) -> Result<Vec<ResolvedRustActionV1>, String> {
    let event = session
        .run_state
        .event_state
        .as_ref()
        .filter(|event| event.id != EventId::Neow)
        .ok_or_else(|| "Rust natural event omitted its non-Neow state".to_owned())?;
    let canonical_id = sts_java_parity_protocol::canonical_game_id(event.id.stable_id());
    let canonical_screen = natural_event_screen_v1(session)?;
    let mut option_indices = BTreeSet::new();
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        let semantic = if let Some(discard) =
            run_potion_semantic(session, candidate.key.as_ref(), &input)?
        {
            discard
        } else {
            let Some(DecisionCandidateKey::EventOption {
                event_id,
                screen,
                option_index,
                ..
            }) = candidate.key.as_ref()
            else {
                return Err(
                    "Rust natural event contains a non-event executable candidate".to_owned(),
                );
            };
            if *event_id != event.id
                || *screen != event.current_screen
                || !option_indices.insert(*option_index)
            {
                return Err("Rust natural event action has an invalid typed identity".to_owned());
            }
            let occurrence = u32::try_from(*option_index)
                .map_err(|_| "Rust natural event option index exceeds u32".to_owned())?;
            if event.id == EventId::TheLibrary && event.current_screen == 1 {
                if event.card_payload.len() != 20
                    || event
                        .card_payload
                        .iter()
                        .map(|card| card.card_id.stable_id())
                        .collect::<BTreeSet<_>>()
                        .len()
                        != 20
                {
                    return Err("Rust Library omitted its twenty unique offers".to_owned());
                }
                let card = event
                    .card_payload
                    .get(*option_index)
                    .ok_or("Rust Library action is outside its offers")?;
                SemanticActionV1::SubmitSelection {
                    surface: DecisionSurfaceV1::GridSelect,
                    choices: vec![sts_parity_lockstep::natural_event_card_offer_v1(
                        &canonical_id,
                        canonical_screen,
                        occurrence,
                        &sts_java_parity_protocol::canonical_game_id(card.card_id.stable_id()),
                        card.upgrades,
                    )?],
                }
            } else if event.id == EventId::MatchAndKeep && event.current_screen == 2 {
                let position =
                    sts_oracle_run_control::content::events::match_and_keep::available_positions(
                        event,
                    )
                    .get(*option_index)
                    .copied()
                    .ok_or("missing Match and Keep cell")?;
                sts_parity_lockstep::natural_match_flip_v1(
                    sts_oracle_run_control::content::events::match_and_keep::cell_position(position)
                        as u8,
                )?
            } else {
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Event,
                    choice: natural_event_option_v1(&canonical_id, canonical_screen, occurrence)?,
                }
            }
        };
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            input,
        ));
    }
    if resolved.is_empty() {
        return Err("Rust natural event exposes no semantic actions".to_owned());
    }
    Ok(resolved)
}

fn run_pending_choice_actions(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, crate::bootstrap::BoundCardSourceV1>,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    let EngineState::RunPendingChoice(choice) = &session.engine_state else {
        return Err("Rust grid selection requires a run pending choice".to_owned());
    };
    if is_astrolabe_choice_v1(choice) {
        return astrolabe_actions(session, bindings);
    }
    let surface = match choice.source {
        DomainEventSource::Event(event_id)
            if choice.return_state.as_ref() == &EngineState::EventRoom
                && session
                    .run_state
                    .event_state
                    .as_ref()
                    .is_some_and(|event| event.id == event_id) =>
        {
            natural_event_screen_v1(session)?;
            DecisionSurfaceV1::GridSelect
        }
        DomainEventSource::CampfireSmith
            if choice.reason == RunPendingChoiceReason::Upgrade
                && choice.return_state.as_ref() == &EngineState::MapNavigation
                && session.run_state.event_state.is_none() =>
        {
            DecisionSurfaceV1::Campfire
        }
        DomainEventSource::Relic(relic_id)
            if (bottle_choice_identity_v1(choice).is_some() || is_empty_cage_choice_v1(choice))
                && session
                    .run_state
                    .relics
                    .iter()
                    .filter(|relic| relic.id == relic_id && relic.amount == 0)
                    .count()
                    == 1 =>
        {
            DecisionSurfaceV1::GridSelect
        }
        _ => return Err("Rust deck selection has no supported natural owner".to_owned()),
    };
    let count = if is_empty_cage_choice_v1(choice)
        || (choice.source == DomainEventSource::Event(EventId::DrugDealer)
            && choice.reason == RunPendingChoiceReason::Transform
            && choice.return_state.as_ref() == &EngineState::EventRoom
            && natural_event_screen_v1(session)? == 1)
    {
        2
    } else {
        1
    };
    if choice.min_choices != count || choice.max_choices != count || !choice.selected.is_empty() {
        return Err("Rust deck selection is outside its declared complete-set profile".to_owned());
    }
    let request = choice.selection_request(&session.run_state);
    if request.scope != SelectionScope::Deck
        || request.can_cancel
        || request.targets.is_empty()
        || request.targets.len() > session.run_state.master_deck.len()
        || selection_kind_v1(choice.reason).is_err()
    {
        return Err("Rust grid selection has an invalid deck-selection contract".to_owned());
    }
    let allowed = request
        .targets
        .iter()
        .copied()
        .map(SelectionTargetRef::card_lineage)
        .collect::<BTreeSet<_>>();
    if allowed.len() != request.targets.len() {
        return Err("Rust grid selection repeats an allowed target".to_owned());
    }
    if count == 2 {
        let targets = &request.targets;
        let mut resolved = Vec::new();
        for candidate in build_decision_surface(session).view.candidates {
            let Some(input) = candidate.action.executable_input() else {
                continue;
            };
            if let Some(discard) = run_potion_semantic(session, candidate.key.as_ref(), &input)? {
                resolved.push(ResolvedRustActionV1::candidate(
                    discard,
                    candidate.id,
                    input,
                ));
            }
        }
        for first in targets {
            for second in targets {
                if first == second {
                    continue;
                }
                let selected = vec![*first, *second];
                let choices = selected
                    .iter()
                    .map(|target| {
                        let lineage = target.card_lineage();
                        let card = session
                            .run_state
                            .master_deck
                            .iter()
                            .find(|c| c.lineage == lineage)
                            .ok_or("Deck selection target is absent from the master deck")?;
                        natural_deck_card_choice_v1(
                            require_card_binding(bindings, lineage)?,
                            &sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
                            card.upgrades,
                        )
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                resolved.push(ResolvedRustActionV1 {
                    semantic: SemanticActionV1::SubmitSelection { surface, choices },
                    execution: RustActionExecutionV1::DirectInput {
                        input: ClientInput::SubmitSelection(SelectionResolution {
                            scope: SelectionScope::Deck,
                            selected,
                        }),
                    },
                });
            }
        }
        if resolved.is_empty() {
            return Err("Deck selection requires two distinct eligible cards".into());
        }
        return Ok(resolved);
    }
    let mut selected = BTreeSet::new();
    let mut semantics = BTreeSet::new();
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        if let Some(discard) = run_potion_semantic(session, candidate.key.as_ref(), &input)? {
            let key = serde_json::to_string(&discard)
                .map_err(|error| format!("failed to encode grid-selection action: {error}"))?;
            if !semantics.insert(key) {
                return Err("Rust grid selection repeats one potion discard action".to_owned());
            }
            resolved.push(ResolvedRustActionV1::candidate(
                discard,
                candidate.id,
                input,
            ));
            continue;
        }
        let ClientInput::SubmitSelection(resolution) = &input else {
            return Err("Rust grid selection exposes a non-selection input".to_owned());
        };
        let [SelectionTargetRef::CardLineage(lineage)] = resolution.selected.as_slice() else {
            return Err(
                "Rust grid selection candidate does not select exactly one card".to_owned(),
            );
        };
        if resolution.scope != SelectionScope::Deck
            || !allowed.contains(lineage)
            || !selected.insert(*lineage)
        {
            return Err("Rust grid selection candidate is outside its typed target set".to_owned());
        }
        let card = session
            .run_state
            .master_deck
            .iter()
            .find(|card| card.lineage == *lineage)
            .ok_or_else(|| {
                "Rust grid selection target is absent from the master deck".to_owned()
            })?;
        let binding = require_card_binding(bindings, *lineage)?;
        let card_choice = natural_deck_card_choice_v1(
            binding,
            &sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
            card.upgrades,
        )?;
        let semantic = match surface {
            DecisionSurfaceV1::GridSelect => SemanticActionV1::SubmitSelection {
                surface,
                choices: vec![card_choice],
            },
            DecisionSurfaceV1::Campfire => SemanticActionV1::Select {
                surface,
                choice: card_choice,
            },
            _ => unreachable!("natural deck selection chose an unsupported surface"),
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode grid-selection action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Rust grid selection repeats one semantic action".to_owned());
        }
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            input,
        ));
    }
    if selected.len() != allowed.len() || resolved.is_empty() {
        return Err("Rust grid selection action space is incomplete".to_owned());
    }
    Ok(resolved)
}

pub(crate) fn is_empty_cage_choice_v1(
    choice: &sts_oracle_run_control::state::core::RunPendingChoiceState,
) -> bool {
    choice.source
        == DomainEventSource::Relic(sts_oracle_run_control::content::relics::RelicId::EmptyCage)
        && choice.reason == RunPendingChoiceReason::Purge
        && choice.min_choices == 2
        && choice.max_choices == 2
        && choice.selected.is_empty()
}

pub(crate) fn is_astrolabe_choice_v1(
    choice: &sts_oracle_run_control::state::core::RunPendingChoiceState,
) -> bool {
    choice.source
        == DomainEventSource::Relic(sts_oracle_run_control::content::relics::RelicId::Astrolabe)
        && choice.reason == RunPendingChoiceReason::TransformUpgraded
}

fn astrolabe_actions(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, crate::bootstrap::BoundCardSourceV1>,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    use sts_oracle_run_control::content::relics::RelicId;
    let EngineState::RunPendingChoice(choice) = &session.engine_state else {
        unreachable!()
    };
    if choice.min_choices != 3
        || choice.max_choices != 3
        || choice.selected.len() >= 3
        || choice.selected.iter().collect::<BTreeSet<_>>().len() != choice.selected.len()
        || session
            .run_state
            .relics
            .iter()
            .filter(|r| r.id == RelicId::Astrolabe)
            .count()
            != 1
        || choice.return_state.as_ref() != &EngineState::MapNavigation
        || !session.run_state.pending_boss_act_transition
    {
        return Err("Rust Astrolabe selection has an invalid owner or ordered prefix".into());
    }
    for lineage in &choice.selected {
        let card = session
            .run_state
            .master_deck
            .iter()
            .find(|card| card.lineage == *lineage)
            .ok_or("Rust Astrolabe selected card is absent from the deck")?;
        if !sts_oracle_run_control::state::core::run_pending_choice_allows_card_for_run(
            &choice.reason,
            card,
            &session.run_state,
        ) {
            return Err("Rust Astrolabe selected an ineligible card".into());
        }
        require_card_binding(bindings, *lineage)?;
    }
    let request = choice.selection_request(&session.run_state);
    let mut resolved = Vec::new();
    for target in request.targets {
        let lineage = target.card_lineage();
        let (index, card) = session
            .run_state
            .master_deck
            .iter()
            .enumerate()
            .find(|(_, card)| card.lineage == lineage)
            .ok_or("Astrolabe target missing")?;
        resolved.push(ResolvedRustActionV1 {
            semantic: SemanticActionV1::Select {
                surface: DecisionSurfaceV1::GridSelect,
                choice: natural_deck_card_choice_v1(
                    require_card_binding(bindings, lineage)?,
                    &sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
                    card.upgrades,
                )?,
            },
            execution: RustActionExecutionV1::DirectInput {
                input: ClientInput::SelectCard(index),
            },
        });
    }
    for candidate in build_decision_surface(session).view.candidates {
        if let Some(input) = candidate.action.executable_input() {
            if let Some(semantic) = run_potion_semantic(session, candidate.key.as_ref(), &input)? {
                resolved.push(ResolvedRustActionV1::candidate(
                    semantic,
                    candidate.id,
                    input,
                ));
            }
        }
    }
    Ok(resolved)
}

pub(crate) fn selection_kind_v1(reason: RunPendingChoiceReason) -> Result<&'static str, String> {
    match reason {
        RunPendingChoiceReason::Purge | RunPendingChoiceReason::PurgeNonBottled => {
            Ok("deck_remove")
        }
        RunPendingChoiceReason::Upgrade => Ok("deck_upgrade"),
        RunPendingChoiceReason::BottleFlame => Ok("bottle_flame"),
        RunPendingChoiceReason::BottleLightning => Ok("bottle_lightning"),
        RunPendingChoiceReason::BottleTornado => Ok("bottle_tornado"),
        RunPendingChoiceReason::Transform
        | RunPendingChoiceReason::TransformNonBottled
        | RunPendingChoiceReason::TransformUpgraded => Ok("deck_transform"),
        _ => Err("Rust pending choice is not an original-game event grid mutation".to_owned()),
    }
}

pub(crate) fn bottle_choice_identity_v1(
    choice: &sts_oracle_run_control::state::core::RunPendingChoiceState,
) -> Option<&'static str> {
    use sts_oracle_run_control::content::relics::RelicId;
    match (choice.source, choice.reason) {
        (DomainEventSource::Relic(RelicId::BottledFlame), RunPendingChoiceReason::BottleFlame) => {
            Some("bottledflame")
        }
        (
            DomainEventSource::Relic(RelicId::BottledLightning),
            RunPendingChoiceReason::BottleLightning,
        ) => Some("bottledlightning"),
        (
            DomainEventSource::Relic(RelicId::BottledTornado),
            RunPendingChoiceReason::BottleTornado,
        ) => Some("bottledtornado"),
        _ => None,
    }
}

pub(crate) fn natural_event_screen_v1(session: &RunControlSession) -> Result<i64, String> {
    let event = session
        .run_state
        .event_state
        .as_ref()
        .ok_or_else(|| "Rust natural event omitted its state".to_owned())?;
    let screen = match (&session.engine_state, event.id, event.current_screen) {
        (EngineState::RunPendingChoice(choice), EventId::Neow, 2)
            if choice.source == DomainEventSource::Event(EventId::Neow)
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            2
        }
        (EngineState::EventRoom, EventId::BonfireElementals, 0 | 1) => event.current_screen,
        (EngineState::EventRoom, EventId::BonfireElementals, 3) => 2,
        (EngineState::EventRoom, EventId::Cleric, 1) => 99,
        (EngineState::EventRoom, EventId::AccursedBlacksmith, 1) => 2,
        (EngineState::EventRoom, EventId::TheLibrary, 2) => 1,
        (EngineState::EventRoom, EventId::DeadAdventurer, 2)
            if sts_oracle_run_control::content::events::dead_adventurer::num_rewards(
                event.internal_state,
            ) < 3 =>
        {
            // Rust shares a result page for leaving and taking all rewards;
            // original ESCAPE is ordinal3, while SUCCESS remains ordinal2.
            3
        }
        (EngineState::RunPendingChoice(choice), EventId::AccursedBlacksmith, 1)
            if choice.source == DomainEventSource::Event(EventId::AccursedBlacksmith)
                && choice.reason == RunPendingChoiceReason::Upgrade
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            // Forge opens the upgrade grid and sets screenNum to 2 before
            // the selected card is upgraded in AccursedBlacksmith.update.
            2
        }
        (EngineState::RunPendingChoice(choice), EventId::Cleric, 1)
            if choice.source == DomainEventSource::Event(EventId::Cleric)
                && choice.reason == RunPendingChoiceReason::PurgeNonBottled
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            // Cleric opens the grid, then showProceedScreen sets screenNum to 99.
            99
        }
        (EngineState::RunPendingChoice(choice), EventId::LivingWall, 1)
            if choice.source == DomainEventSource::Event(EventId::LivingWall)
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            // LivingWall.buttonEffect switches INTRO -> RESULT before its
            // master-deck selection settles; the grid therefore belongs to 1.
            1
        }
        (EngineState::RunPendingChoice(choice), EventId::UpgradeShrine, 1)
            if choice.source == DomainEventSource::Event(EventId::UpgradeShrine)
                && choice.reason == RunPendingChoiceReason::Upgrade
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            // Pray switches INTRO -> COMPLETE before opening the upgrade grid;
            // UpgradeShrine.update upgrades the selected card on that same page.
            1
        }
        (EngineState::RunPendingChoice(choice), EventId::Purifier, 1)
            if choice.source == DomainEventSource::Event(EventId::Purifier)
                && choice.reason == RunPendingChoiceReason::PurgeNonBottled
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            // PurificationShrine switches INTRO -> COMPLETE before opening
            // the non-bottled purge grid; update removes the selected card.
            1
        }
        (EngineState::RunPendingChoice(choice), EventId::Transmorgrifier, 1)
            if choice.source == DomainEventSource::Event(EventId::Transmorgrifier)
                && choice.reason == RunPendingChoiceReason::TransformNonBottled
                && choice.min_choices == 1
                && choice.max_choices == 1
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            // Transmogrifier enters COMPLETE before opening its one-card
            // non-bottled transform grid; update settles the choice there.
            1
        }
        (EngineState::RunPendingChoice(choice), EventId::Beggar, 1)
            if choice.source == DomainEventSource::Event(EventId::Beggar)
                && choice.reason == RunPendingChoiceReason::PurgeNonBottled
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            // Beggar opens its purge grid and changes GAVE_MONEY -> LEAVE.
            1
        }
        (EngineState::RunPendingChoice(choice), EventId::DrugDealer, 1)
            if choice.source == DomainEventSource::Event(EventId::DrugDealer)
                && choice.reason == RunPendingChoiceReason::Transform
                && choice.min_choices == 2
                && choice.max_choices == 2
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            // DrugDealer opens its two-card grid then advances screenNum to 1.
            1
        }
        (EngineState::RunPendingChoice(choice), EventId::GoldenWing, 2)
            if choice.source == DomainEventSource::Event(EventId::GoldenWing)
                && choice.reason == RunPendingChoiceReason::PurgeNonBottled
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            // GoldenWing opens the purge grid on PURGE, then switches to MAP
            // before purgeLogic removes the selected master-deck card.
            2
        }
        (EngineState::RunPendingChoice(choice), EventId::GremlinWheelGame, 2)
            if choice.source == DomainEventSource::Event(EventId::GremlinWheelGame)
                && choice.reason == RunPendingChoiceReason::PurgeNonBottled
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            2
        }
        (EngineState::RunPendingChoice(choice), EventId::BonfireElementals, 2)
            if choice.source == DomainEventSource::Event(EventId::BonfireElementals)
                && choice.return_state.as_ref() == &EngineState::EventRoom =>
        {
            1
        }
        (EngineState::EventRoom, _, screen) => screen,
        (EngineState::RunPendingChoice(_), _, _) => {
            return Err(format!(
                "Rust event {:?} has no declared grid-screen normalization",
                event.id
            ))
        }
        _ => return Err("Rust natural event is not at a supported decision boundary".to_owned()),
    };
    i64::try_from(screen).map_err(|_| "Rust natural event screen exceeds i64".to_owned())
}

fn reward_actions(session: &RunControlSession) -> Result<Vec<ResolvedRustActionV1>, String> {
    let rewards = crate::projection::rewards(session)?;
    let (items, surface) = match &rewards {
        NaturalRunRewardsV2::Combat { items, .. } => (items, DecisionSurfaceV1::CombatReward),
        NaturalRunRewardsV2::Treasure { items, .. } => (items, DecisionSurfaceV1::TreasureReward),
        NaturalRunRewardsV2::Inactive => {
            return Err("Rust reward boundary omitted its rewards".to_owned())
        }
    };
    let mut occurrences = BTreeMap::<String, u32>::new();
    let mut item_choices = Vec::with_capacity(items.len());
    for item in items {
        let key = serde_json::to_string(item)
            .map_err(|error| format!("failed to encode Rust reward identity: {error}"))?;
        let occurrence = *occurrences.entry(key.clone()).or_default();
        *occurrences
            .get_mut(&key)
            .expect("reward occurrence was inserted") += 1;
        item_choices.push(natural_reward_choice_v1(item, occurrence));
    }

    let mut semantics = BTreeSet::new();
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        let executable_input = input.clone();
        let semantic =
            if let Some(discard) = run_potion_semantic(session, candidate.key.as_ref(), &input)? {
                discard
            } else {
                match input {
                    ClientInput::ClaimReward(index) => SemanticActionV1::Select {
                        surface,
                        choice: item_choices
                            .get(index)
                            .ok_or_else(|| {
                                "Rust combat reward action points outside its item list".to_owned()
                            })?
                            .clone(),
                    },
                    ClientInput::Proceed => SemanticActionV1::Proceed { surface },
                    ClientInput::Cancel
                        if matches!(session.engine_state, EngineState::RewardOverlay { .. }) =>
                    {
                        SemanticActionV1::Cancel { surface }
                    }
                    other => {
                        return Err(format!(
                            "Rust combat reward exposes unsupported input {:?}",
                            other
                        ))
                    }
                }
            };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Rust reward action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Rust combat reward repeats one semantic action".to_owned());
        }
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            executable_input,
        ));
    }
    if resolved.is_empty() {
        return Err("Rust combat reward exposes no semantic actions".to_owned());
    }
    Ok(resolved)
}

fn card_reward_actions(session: &RunControlSession) -> Result<Vec<ResolvedRustActionV1>, String> {
    let reward = crate::projection::reward_state(session)?
        .ok_or("Rust card reward is not on a reward screen")?;
    if reward.screen_context
        == sts_oracle_run_control::state::rewards::RewardScreenContext::NeowCardChoice
        && (session.run_state.act_num != 1
            || (
                session.run_state.map.current_x,
                session.run_state.map.current_y,
            ) != (-1, -1)
            || !session
                .run_state
                .event_state
                .as_ref()
                .is_some_and(|event| event.id == EventId::Neow && event.current_screen == 2))
    {
        return Err("Rust Neow card choice lacks its natural event entrance".to_owned());
    }
    let cards = reward
        .pending_card_choice
        .as_ref()
        .ok_or_else(|| "Rust card reward omitted its offered cards".to_owned())?;
    let reward_item_index = reward
        .pending_card_reward_index
        .ok_or_else(|| "Rust card reward omitted its parent reward index".to_owned())?;
    let parent_cards = match reward.items.get(reward_item_index) {
        Some(sts_oracle_run_control::state::rewards::RewardItem::Card { cards }) => cards,
        _ => return Err("Rust card reward parent is not a card item".to_owned()),
    };
    if parent_cards != cards {
        return Err("Rust card reward offers differ from their parent reward item".to_owned());
    }

    let mut occurrences = BTreeMap::<(String, u8), u32>::new();
    let mut choices = Vec::with_capacity(cards.len());
    for card in cards {
        let card = NaturalRewardCardV1 {
            card_id: sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
            upgrades: card.upgrades,
        };
        let key = (card.card_id.clone(), card.upgrades);
        let occurrence = *occurrences.entry(key.clone()).or_default();
        *occurrences
            .get_mut(&key)
            .expect("card-reward occurrence was inserted") += 1;
        choices.push(natural_card_reward_choice_v1(&card, occurrence));
    }

    let mut semantics = BTreeSet::new();
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let action = candidate.action.executable_action();
        let (semantic, input) = if let Some(RunDecisionAction::Input(input)) = action.as_ref() {
            if let Some(discard) = run_potion_semantic(session, candidate.key.as_ref(), input)? {
                (discard, input.clone())
            } else {
                match (candidate.key.as_ref(), input) {
                    (
                        Some(DecisionCandidateKey::CardRewardPick {
                            reward_item_index: candidate_reward_index,
                            option_index,
                            card,
                            upgrades,
                        }),
                        ClientInput::SelectCard(input_index),
                    ) => {
                        let offered = cards.get(*option_index).ok_or_else(|| {
                            "Rust card reward candidate points outside its offers".to_owned()
                        })?;
                        let expected_upgrades = session
                            .run_state
                            .preview_obtain_card_upgrades(offered.id, offered.upgrades);
                        if *candidate_reward_index != reward_item_index
                            || input_index != option_index
                            || offered.id != *card
                            || expected_upgrades != *upgrades
                        {
                            return Err(
                                "Rust card reward candidate disagrees with its typed offer"
                                    .to_owned(),
                            );
                        }
                        (
                            SemanticActionV1::Select {
                                surface: DecisionSurfaceV1::CardReward,
                                choice: choices[*option_index].clone(),
                            },
                            input.clone(),
                        )
                    }
                    (
                        Some(DecisionCandidateKey::CardRewardSingingBowl {
                            reward_item_index: candidate_reward_index,
                            option_index,
                        }),
                        ClientInput::SelectCard(input_index),
                    ) if *candidate_reward_index == reward_item_index
                        && *option_index == cards.len()
                        && input_index == option_index =>
                    {
                        (
                            SemanticActionV1::Select {
                                surface: DecisionSurfaceV1::CardReward,
                                choice: sts_parity_lockstep::natural_singing_bowl_choice_v1(),
                            },
                            input.clone(),
                        )
                    }
                    _ => {
                        return Err(
                            "Rust card reward exposes an unrelated executable input".to_owned()
                        )
                    }
                }
            }
        } else {
            match (candidate.key.as_ref(), action) {
                (
                    Some(DecisionCandidateKey::CardRewardSkip {
                        reward_item_index: candidate_reward_index,
                    }),
                    Some(RunDecisionAction::SkipCardReward {
                        reward_item_index: action_reward_index,
                    }),
                ) if *candidate_reward_index == reward_item_index
                    && action_reward_index == reward_item_index =>
                {
                    (
                        SemanticActionV1::Cancel {
                            surface: DecisionSurfaceV1::CardReward,
                        },
                        ClientInput::Cancel,
                    )
                }
                (Some(DecisionCandidateKey::CardRewardSingingBowl { .. }), _) => {
                    return Err(
                        "Rust natural card reward does not yet support Singing Bowl".to_owned()
                    )
                }
                _ => continue,
            }
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Rust card reward action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Rust card reward repeats one semantic action".to_owned());
        }
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            input,
        ));
    }
    if resolved
        .iter()
        .filter(|action| {
            matches!(
                action.semantic,
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::CardReward,
                    ref choice,
                } if choice.kind == "reward_card"
            )
        })
        .count()
        != cards.len()
    {
        return Err("Rust card reward does not expose every offered card exactly once".to_owned());
    }
    if !resolved.iter().any(|action| {
        matches!(
            action.semantic,
            SemanticActionV1::Cancel {
                surface: DecisionSurfaceV1::CardReward
            }
        )
    }) {
        return Err("Rust ordinary card reward exposes no skip action".to_owned());
    }
    Ok(resolved)
}

fn combat_actions(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, crate::bootstrap::BoundCardSourceV1>,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    let active = session
        .active_combat
        .as_ref()
        .filter(|active| active.engine_state == EngineState::CombatPlayerTurn)
        .ok_or_else(|| "Rust combat boundary omitted its active player turn".to_owned())?;
    let combat = &active.combat_state;
    let mut semantics = BTreeSet::new();
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        let executable_input = input.clone();
        let semantic = match input {
            ClientInput::PlayCard { card_index, target } => {
                let card = combat
                    .zones
                    .hand
                    .get(card_index)
                    .ok_or_else(|| "Rust combat action points outside the hand".to_owned())?;
                SemanticActionV1::PlayCard {
                    card_binding: require_card_binding(bindings, card.lineage)?.to_owned(),
                    card_id: sts_java_parity_protocol::canonical_game_id(card.id.stable_id()),
                    target: target
                        .map(|target| monster_choice(combat, target))
                        .transpose()?,
                }
            }
            ClientInput::UsePotion {
                potion_index,
                target,
            } => {
                let potion = combat
                    .entities
                    .potions
                    .get(potion_index)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| {
                        "Rust combat action points to an empty potion slot".to_owned()
                    })?;
                SemanticActionV1::UsePotion {
                    slot: u8::try_from(potion_index)
                        .map_err(|_| "Rust potion slot exceeds u8".to_owned())?,
                    potion_id: sts_java_parity_protocol::canonical_game_id(potion.id.stable_id()),
                    target: target
                        .map(|target| monster_choice(combat, target))
                        .transpose()?,
                }
            }
            ClientInput::DiscardPotion(slot) => {
                let potion = combat
                    .entities
                    .potions
                    .get(slot)
                    .and_then(Option::as_ref)
                    .ok_or_else(|| {
                        "Rust discard action points to an empty potion slot".to_owned()
                    })?;
                SemanticActionV1::DiscardPotion {
                    slot: u8::try_from(slot)
                        .map_err(|_| "Rust potion slot exceeds u8".to_owned())?,
                    potion_id: sts_java_parity_protocol::canonical_game_id(potion.id.stable_id()),
                }
            }
            ClientInput::EndTurn => SemanticActionV1::EndTurn,
            other => {
                return Err(format!(
                    "Rust combat player turn exposes unsupported input {:?}",
                    other
                ))
            }
        };
        let key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Rust combat action: {error}"))?;
        if !semantics.insert(key) {
            return Err("Rust combat action space repeats one semantic action".to_owned());
        }
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            executable_input,
        ));
    }
    if resolved.is_empty() {
        return Err("Rust combat player turn exposes no semantic actions".to_owned());
    }
    Ok(resolved)
}

fn require_card_binding(
    bindings: &BTreeMap<CardLineageId, crate::bootstrap::BoundCardSourceV1>,
    lineage: CardLineageId,
) -> Result<&str, String> {
    bindings
        .get(&lineage)
        .map(|bound| bound.binding.as_str())
        .ok_or_else(|| format!("combat card lineage {lineage} has no source-proven binding"))
}

fn monster_choice(
    combat: &sts_oracle_run_control::runtime::combat::CombatState,
    entity_id: usize,
) -> Result<SemanticChoiceV1, String> {
    let index = combat
        .entities
        .monsters
        .iter()
        .position(|monster| monster.id == entity_id)
        .ok_or_else(|| "Rust combat action targets an unknown monster entity".to_owned())?;
    let enemy_id = sts_java_parity_protocol::canonical_game_id(
        combat.entities.monsters[index].enemy_id.stable_id(),
    );
    let occurrence = combat.entities.monsters[..index]
        .iter()
        .filter(|monster| {
            sts_java_parity_protocol::canonical_game_id(monster.enemy_id.stable_id()) == enemy_id
        })
        .count();
    Ok(SemanticChoiceV1::new(
        "monster",
        enemy_id,
        u32::try_from(occurrence).map_err(|_| "monster occurrence exceeds u32".to_owned())?,
    ))
}

fn neow_actions(session: &RunControlSession) -> Result<Vec<ResolvedRustActionV1>, String> {
    let event = session
        .run_state
        .event_state
        .as_ref()
        .filter(|event| event.id == EventId::Neow)
        .ok_or_else(|| "Rust EventRoom is not the natural Neow boundary".to_owned())?;
    let mut seen = BTreeSet::new();
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        let key = candidate
            .key
            .as_ref()
            .ok_or_else(|| "executable Neow candidate has no typed key".to_owned())?;
        if let Some(semantic) = run_potion_semantic(session, Some(key), &input)? {
            let semantic_key = serde_json::to_string(&semantic)
                .map_err(|error| format!("failed to encode Neow action identity: {error}"))?;
            if !seen.insert(semantic_key) {
                return Err("Neow surface repeats one semantic action".to_owned());
            }
            resolved.push(ResolvedRustActionV1::candidate(
                semantic,
                candidate.id,
                input,
            ));
            continue;
        }
        let DecisionCandidateKey::EventOption {
            event_id: EventId::Neow,
            screen,
            option_index,
            action,
        } = key
        else {
            return Err("Neow surface contains a non-Neow executable candidate".to_owned());
        };
        if *screen != event.current_screen {
            return Err("Neow candidate screen disagrees with event state".to_owned());
        }
        let semantic = match event.current_screen {
            0 => SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::Neow,
            },
            1 => {
                let (reward, drawback) =
                    choice_identity(event, *option_index).ok_or_else(|| {
                        "Neow candidate omitted a simulator-owned choice identity".to_owned()
                    })?;
                let mut attributes = BTreeMap::new();
                attributes.insert(
                    "drawback".to_owned(),
                    SemanticScalarV1::Text(neow_drawback_id(drawback).to_owned()),
                );
                SemanticActionV1::Select {
                    surface: DecisionSurfaceV1::Neow,
                    choice: SemanticChoiceV1 {
                        kind: "neow_reward".to_owned(),
                        id: neow_reward_id(reward).to_owned(),
                        occurrence: 0,
                        attributes,
                    },
                }
            }
            2 if *action == EventActionKind::Leave => SemanticActionV1::Proceed {
                surface: DecisionSurfaceV1::Neow,
            },
            screen => {
                return Err(format!(
                    "Neow screen {screen} is outside the natural lockstep profile"
                ))
            }
        };
        let semantic_key = serde_json::to_string(&semantic)
            .map_err(|error| format!("failed to encode Neow action identity: {error}"))?;
        if !seen.insert(semantic_key) {
            return Err("Neow surface repeats one semantic action".to_owned());
        }
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            input,
        ));
    }
    if resolved.is_empty() {
        return Err("Neow boundary exposes no executable semantic actions".to_owned());
    }
    Ok(resolved)
}

fn map_actions(session: &RunControlSession) -> Result<Vec<ResolvedRustActionV1>, String> {
    let mut coordinates = BTreeSet::new();
    let mut resolved = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        let semantic = if let Some(discard) =
            run_potion_semantic(session, candidate.key.as_ref(), &input)?
        {
            discard
        } else {
            match candidate.key.as_ref() {
                Some(DecisionCandidateKey::RouteSelect {
                    x,
                    y,
                    room_type: Some(room),
                    ..
                }) => {
                    if !coordinates.insert((*x, *y)) {
                        return Err(format!("map action space repeats node ({x},{y})"));
                    }
                    let mut attributes = BTreeMap::new();
                    attributes.insert("x".to_owned(), SemanticScalarV1::Integer(i64::from(*x)));
                    attributes.insert("y".to_owned(), SemanticScalarV1::Integer(i64::from(*y)));
                    attributes.insert(
                        "room".to_owned(),
                        SemanticScalarV1::Text(map_room_id(*room).to_owned()),
                    );
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::Map,
                        choice: SemanticChoiceV1 {
                            kind: "map_node".to_owned(),
                            id: map_node_id(*x, *y)?,
                            occurrence: 0,
                            attributes,
                        },
                    }
                }
                Some(DecisionCandidateKey::RouteCancel) => SemanticActionV1::Cancel {
                    surface: DecisionSurfaceV1::Map,
                },
                Some(DecisionCandidateKey::RouteSelect {
                    room_type: None, ..
                }) => return Err("map route candidate has no public room identity".to_owned()),
                _ => return Err("map surface contains a non-route executable candidate".to_owned()),
            }
        };
        resolved.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            input,
        ));
    }
    if resolved.is_empty() {
        return Err("map boundary exposes no executable semantic actions".to_owned());
    }
    Ok(resolved)
}

fn combat_potion_discard_semantic(
    combat: &sts_oracle_run_control::runtime::combat::CombatState,
    slot: usize,
) -> Result<SemanticActionV1, String> {
    let potion = combat
        .entities
        .potions
        .get(slot)
        .and_then(Option::as_ref)
        .filter(|potion| potion.can_discard)
        .ok_or("Rust combat selection exposes an invalid potion discard")?;
    Ok(SemanticActionV1::DiscardPotion {
        slot: u8::try_from(slot).map_err(|_| "Rust potion slot exceeds u8".to_owned())?,
        potion_id: sts_java_parity_protocol::canonical_game_id(potion.id.stable_id()),
    })
}

pub(crate) fn run_potion_semantic(
    session: &RunControlSession,
    key: Option<&DecisionCandidateKey>,
    input: &ClientInput,
) -> Result<Option<SemanticActionV1>, String> {
    let (slot, potion, uuid, use_potion) = match key {
        Some(DecisionCandidateKey::RunPotionUse { slot, potion, uuid }) => {
            (slot, potion, uuid, true)
        }
        Some(DecisionCandidateKey::RunPotionDiscard { slot, potion, uuid }) => {
            (slot, potion, uuid, false)
        }
        _ => {
            if matches!(
                input,
                ClientInput::UsePotion { .. } | ClientInput::DiscardPotion(_)
            ) {
                return Err(
                    "Rust run potion action omitted its typed candidate identity".to_owned(),
                );
            }
            return Ok(None);
        }
    };
    let expected = if use_potion {
        ClientInput::UsePotion {
            potion_index: *slot,
            target: None,
        }
    } else {
        ClientInput::DiscardPotion(*slot)
    };
    if input != &expected {
        return Err("Rust run potion input disagrees with its typed action or slot".to_owned());
    }
    let observed = session
        .run_state
        .potions
        .get(*slot)
        .and_then(Option::as_ref)
        .ok_or("Rust run potion action points to an empty potion slot")?;
    if observed.id != *potion
        || observed.uuid != *uuid
        || (use_potion && !observed.can_use)
        || (!use_potion && !observed.can_discard)
    {
        return Err("Rust run potion action disagrees with its observed slot".to_owned());
    }
    let slot = u8::try_from(*slot).map_err(|_| "Rust potion slot exceeds u8")?;
    let potion_id = sts_java_parity_protocol::canonical_game_id(potion.stable_id());
    Ok(Some(if use_potion {
        SemanticActionV1::UsePotion {
            slot,
            potion_id,
            target: None,
        }
    } else {
        SemanticActionV1::DiscardPotion { slot, potion_id }
    }))
}

pub(crate) fn map_node_id(x: i32, y: i32) -> Result<String, String> {
    natural_map_node_id_v1(i64::from(x), i64::from(y))
}

pub(crate) const fn map_room_id(room: PlannerMapRoom) -> &'static str {
    match room {
        PlannerMapRoom::QuestionMark => "question_mark",
        PlannerMapRoom::Monster => "monster",
        PlannerMapRoom::Elite => "elite",
        PlannerMapRoom::Boss => "boss",
        PlannerMapRoom::Rest => "rest",
        PlannerMapRoom::Shop => "shop",
        PlannerMapRoom::Treasure => "treasure",
        PlannerMapRoom::Victory => "victory",
    }
}

fn neow_reward_id(reward: NeowRewardType) -> &'static str {
    match reward {
        NeowRewardType::ThreeEnemyKill => "three_enemy_kill",
        NeowRewardType::TenPercentHpBonus => "ten_percent_hp_bonus",
        NeowRewardType::ThreeCards => "three_cards",
        NeowRewardType::OneRandomRareCard => "one_random_rare_card",
        NeowRewardType::RemoveCard => "remove_card",
        NeowRewardType::UpgradeCard => "upgrade_card",
        NeowRewardType::TransformCard => "transform_card",
        NeowRewardType::RandomColorless => "random_colorless",
        NeowRewardType::ThreeSmallPotions => "three_small_potions",
        NeowRewardType::RandomCommonRelic => "random_common_relic",
        NeowRewardType::HundredGold => "hundred_gold",
        NeowRewardType::RandomColorless2 => "random_colorless_2",
        NeowRewardType::RemoveTwo => "remove_two",
        NeowRewardType::OneRareRelic => "one_rare_relic",
        NeowRewardType::ThreeRareCards => "three_rare_cards",
        NeowRewardType::TwoFiftyGold => "two_fifty_gold",
        NeowRewardType::TransformTwoCards => "transform_two_cards",
        NeowRewardType::TwentyPercentHpBonus => "twenty_percent_hp_bonus",
        NeowRewardType::BossRelic => "boss_relic",
    }
}

fn neow_drawback_id(drawback: NeowDrawback) -> &'static str {
    match drawback {
        NeowDrawback::None => "none",
        NeowDrawback::TenPercentHpLoss => "ten_percent_hp_loss",
        NeowDrawback::NoGold => "no_gold",
        NeowDrawback::Curse => "curse",
        NeowDrawback::PercentDamage => "percent_damage",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dead_adventurer_escape_and_success_keep_distinct_original_screens() {
        use super::*;
        use sts_oracle_run_control::state::events::EventState;
        let mut session = RunControlSession::new(Default::default());
        session.engine_state = EngineState::EventRoom;
        let mut event = EventState::new(EventId::DeadAdventurer);
        event.current_screen = 2;
        for rewards in 0..=3 {
            event.internal_state = rewards;
            session.run_state.event_state = Some(event.clone());
            assert_eq!(
                natural_event_screen_v1(&session).unwrap(),
                if rewards < 3 { 3 } else { 2 }
            );
        }
        event.current_screen = 0;
        session.run_state.event_state = Some(event.clone());
        assert_eq!(natural_event_screen_v1(&session).unwrap(), 0);
        event.current_screen = 1;
        session.run_state.event_state = Some(event);
        assert_eq!(natural_event_screen_v1(&session).unwrap(), 1);
    }

    #[test]
    fn transmogrifier_grid_requires_its_source_reason_count_and_complete_screen() {
        use super::*;
        use sts_oracle_run_control::state::{core::RunPendingChoiceState, events::EventState};
        let mut session = RunControlSession::new(Default::default());
        let mut event = EventState::new(EventId::Transmorgrifier);
        event.current_screen = 1;
        session.run_state.event_state = Some(event);
        let choice = RunPendingChoiceState {
            selected: vec![],
            min_choices: 1,
            max_choices: 1,
            reason: RunPendingChoiceReason::TransformNonBottled,
            source: DomainEventSource::Event(EventId::Transmorgrifier),
            return_state: Box::new(EngineState::EventRoom),
        };
        session.engine_state = EngineState::RunPendingChoice(choice.clone());
        assert_eq!(natural_event_screen_v1(&session).unwrap(), 1);
        for invalid in [
            RunPendingChoiceState {
                min_choices: 0,
                ..choice.clone()
            },
            RunPendingChoiceState {
                max_choices: 2,
                ..choice.clone()
            },
            RunPendingChoiceState {
                source: DomainEventSource::Event(EventId::LivingWall),
                ..choice.clone()
            },
            RunPendingChoiceState {
                reason: RunPendingChoiceReason::Transform,
                ..choice.clone()
            },
            RunPendingChoiceState {
                return_state: Box::new(EngineState::MapNavigation),
                ..choice.clone()
            },
        ] {
            session.engine_state = EngineState::RunPendingChoice(invalid);
            assert!(natural_event_screen_v1(&session).is_err());
        }
        session.engine_state = EngineState::RunPendingChoice(choice);
        session
            .run_state
            .event_state
            .as_mut()
            .unwrap()
            .current_screen = 0;
        assert!(natural_event_screen_v1(&session).is_err());
    }

    #[test]
    fn drug_dealer_transform_grid_requires_its_source_count_and_result_screen() {
        use super::*;
        use sts_oracle_run_control::state::{core::RunPendingChoiceState, events::EventState};
        let mut session = RunControlSession::new(Default::default());
        let mut event = EventState::new(EventId::DrugDealer);
        event.current_screen = 1;
        session.run_state.event_state = Some(event);
        let choice = RunPendingChoiceState {
            selected: vec![],
            min_choices: 2,
            max_choices: 2,
            reason: RunPendingChoiceReason::Transform,
            source: DomainEventSource::Event(EventId::DrugDealer),
            return_state: Box::new(EngineState::EventRoom),
        };
        session.engine_state = EngineState::RunPendingChoice(choice.clone());
        assert_eq!(natural_event_screen_v1(&session).unwrap(), 1);
        for invalid in [
            RunPendingChoiceState {
                min_choices: 1,
                ..choice.clone()
            },
            RunPendingChoiceState {
                max_choices: 3,
                ..choice.clone()
            },
            RunPendingChoiceState {
                source: DomainEventSource::Event(EventId::LivingWall),
                ..choice.clone()
            },
            RunPendingChoiceState {
                reason: RunPendingChoiceReason::Upgrade,
                ..choice
            },
        ] {
            session.engine_state = EngineState::RunPendingChoice(invalid);
            assert!(natural_event_screen_v1(&session).is_err());
        }
    }

    #[test]
    fn beggar_grid_requires_its_original_leave_screen_and_purge_source() {
        use super::*;
        use sts_oracle_run_control::eval::run_control::RunControlConfig;
        use sts_oracle_run_control::state::core::RunPendingChoiceState;
        use sts_oracle_run_control::state::events::EventState;

        let mut session = RunControlSession::new(RunControlConfig::default());
        let mut event = EventState::new(EventId::Beggar);
        event.current_screen = 1;
        session.run_state.event_state = Some(event);
        let mut choice = RunPendingChoiceState {
            selected: Vec::new(),
            min_choices: 1,
            max_choices: 1,
            reason: RunPendingChoiceReason::PurgeNonBottled,
            source: DomainEventSource::Event(EventId::Beggar),
            return_state: Box::new(EngineState::EventRoom),
        };
        session.engine_state = EngineState::RunPendingChoice(choice.clone());
        assert_eq!(natural_event_screen_v1(&session).unwrap(), 1);
        choice.source = DomainEventSource::Event(EventId::Cleric);
        session.engine_state = EngineState::RunPendingChoice(choice);
        assert!(natural_event_screen_v1(&session).is_err());
    }

    use super::*;
    use crate::bootstrap::BoundCardSourceV1;
    use sts_oracle_run_control::{
        content::cards::CardId,
        eval::run_control::RunControlConfig,
        runtime::combat::CombatCard,
        state::core::{ActiveCombat, CombatContext, RoomCombatContext},
        state::map::RoomType,
        test_support::blank_test_combat,
    };
    use sts_parity_lockstep::NaturalRunCardSourceV1;

    #[test]
    fn scry_preserves_top_order_lineage_and_all_subsets_for_same_named_cards() {
        let cards = vec![
            CombatCard::for_scenario(CardId::StrikeP, 31),
            CombatCard::for_scenario(CardId::StrikeP, 42),
            CombatCard::for_scenario(CardId::DefendP, 53),
            CombatCard::for_scenario(CardId::Vigilance, 64),
        ];
        let ids = cards
            .iter()
            .take(3)
            .map(|card| card.lineage)
            .collect::<Vec<_>>();
        let pending = EngineState::PendingChoice(PendingChoice::ScrySelect {
            cards: cards.iter().take(3).map(|card| card.id).collect(),
            card_lineages: ids.clone(),
        });
        let mut combat = blank_test_combat();
        combat.zones.draw_pile = cards.into();
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = pending.clone();
        session.active_combat = Some(ActiveCombat::new(
            pending,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let bindings = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                (
                    *id,
                    BoundCardSourceV1 {
                        binding: format!("natural_run/starter/{index}"),
                        source: NaturalRunCardSourceV1::StarterDeck {
                            player_class: "WATCHER".into(),
                            source_slot: index as u32,
                        },
                        expected_card_id: if index == 2 { "defendp" } else { "strikep" }.into(),
                        expected_upgrades: 0,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let actions = resolved_actions_v1(&session, &bindings).unwrap();
        assert_eq!(actions.len(), 8);
        assert!(
            matches!(&actions[0].execution, RustActionExecutionV1::DirectInput { input: ClientInput::SubmitScryDiscard(indices) } if indices.is_empty())
        );
        assert!(
            matches!(&actions[7].execution, RustActionExecutionV1::DirectInput { input: ClientInput::SubmitScryDiscard(indices) } if indices == &[0,1,2])
        );
        assert!(
            matches!(&actions[7].semantic, SemanticActionV1::SubmitSelection { surface:DecisionSurfaceV1::GridSelect, choices }
            if choices.iter().map(|choice| &choice.id).eq(ids.iter().map(|id| &bindings[id].binding)))
        );
        assert!(resolved_actions_v1(&session, &BTreeMap::new()).is_err());
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .zones
            .draw_pile
            .swap(0, 1);
        assert!(resolved_actions_v1(&session, &bindings).is_err());
    }

    #[test]
    fn bottle_selection_requires_owned_matching_relic_and_bound_candidates() {
        use sts_oracle_run_control::{
            content::relics::{RelicId, RelicState},
            state::core::RunPendingChoiceState,
        };
        for (relic, reason, card) in [
            (
                RelicId::BottledFlame,
                RunPendingChoiceReason::BottleFlame,
                CardId::Strike,
            ),
            (
                RelicId::BottledLightning,
                RunPendingChoiceReason::BottleLightning,
                CardId::Defend,
            ),
            (
                RelicId::BottledTornado,
                RunPendingChoiceReason::BottleTornado,
                CardId::Inflame,
            ),
        ] {
            let mut session = RunControlSession::new(Default::default());
            session.run_state.master_deck = vec![
                CombatCard::for_scenario(card, 11),
                CombatCard::for_scenario(card, 12),
                CombatCard::for_scenario(CardId::AscendersBane, 13),
            ];
            session.run_state.relics.push(RelicState::new(relic));
            session.engine_state = EngineState::RunPendingChoice(RunPendingChoiceState {
                selected: Vec::new(),
                min_choices: 1,
                max_choices: 1,
                reason,
                source: DomainEventSource::Relic(relic),
                return_state: Box::new(EngineState::MapNavigation),
            });
            let bindings = session
                .run_state
                .master_deck
                .iter()
                .enumerate()
                .map(|(index, card)| {
                    (
                        card.lineage,
                        BoundCardSourceV1 {
                            binding: format!("natural_run/starter/{index}"),
                            source: NaturalRunCardSourceV1::StarterDeck {
                                player_class: "IRONCLAD".into(),
                                source_slot: index as u32,
                            },
                            expected_card_id: sts_java_parity_protocol::canonical_game_id(
                                card.id.stable_id(),
                            ),
                            expected_upgrades: 0,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();
            let actions = resolved_actions_v1(&session, &bindings).unwrap();
            assert_eq!(actions.len(), 2);
            assert_ne!(actions[0].semantic, actions[1].semantic);
            assert!(actions.iter().all(|a| matches!(&a.semantic, SemanticActionV1::SubmitSelection {surface:DecisionSurfaceV1::GridSelect,choices} if choices.len()==1 && choices[0].kind=="deck_card")));
            assert!(resolved_actions_v1(&session, &BTreeMap::new()).is_err());
            if let EngineState::RunPendingChoice(choice) = &mut session.engine_state {
                choice.max_choices = 2;
            }
            assert!(resolved_actions_v1(&session, &bindings).is_err());
            if let EngineState::RunPendingChoice(choice) = &mut session.engine_state {
                choice.max_choices = 1;
            }
            session.run_state.relics.clear();
            assert!(resolved_actions_v1(&session, &bindings).is_err());
        }
    }

    #[test]
    fn draw_selection_preserves_every_exact_lineage_including_skills_and_duplicates() {
        use sts_oracle_run_control::state::{GridSelectReason, PileType};
        let cards = vec![
            CombatCard::for_scenario(CardId::Strike, 31),
            CombatCard::for_scenario(CardId::Strike, 42),
            CombatCard::for_scenario(CardId::Defend, 53),
            CombatCard::for_scenario(CardId::Wound, 64),
        ];
        let ids = cards.iter().map(|card| card.lineage).collect::<Vec<_>>();
        let pending = EngineState::PendingChoice(PendingChoice::GridSelect {
            source_pile: PileType::Draw,
            candidate_lineages: ids.clone(),
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: GridSelectReason::Omniscience { play_amount: 2 },
        });
        let mut combat = blank_test_combat();
        combat.zones.draw_pile = cards.clone().into();
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = pending.clone();
        session.active_combat = Some(ActiveCombat::new(
            pending,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let bindings = cards
            .iter()
            .enumerate()
            .map(|(i, card)| {
                (
                    card.lineage,
                    BoundCardSourceV1 {
                        binding: format!("natural_run/starter/{i}"),
                        source: NaturalRunCardSourceV1::StarterDeck {
                            player_class: "IRONCLAD".into(),
                            source_slot: i as u32,
                        },
                        expected_card_id: sts_java_parity_protocol::canonical_game_id(
                            card.id.stable_id(),
                        ),
                        expected_upgrades: 0,
                    },
                )
            })
            .collect();
        for (reason, kind) in [
            (
                GridSelectReason::Omniscience { play_amount: 2 },
                "omniscience",
            ),
            (GridSelectReason::DrawPileToHand, "draw_to_hand"),
        ] {
            let mut session = session.clone();
            let EngineState::PendingChoice(PendingChoice::GridSelect {
                reason: current, ..
            }) = &mut session.engine_state
            else {
                unreachable!()
            };
            *current = reason;
            session.active_combat.as_mut().unwrap().engine_state = session.engine_state.clone();
            assert_eq!(combat_grid_kind(&session), Some(kind));
            let actions = resolved_actions_v1(&session, &bindings).unwrap();
            assert_eq!(actions.len(), cards.len());
            assert!(actions.iter().all(|a| matches!(&a.semantic,
            SemanticActionV1::SubmitSelection {surface:DecisionSurfaceV1::GridSelect,choices}
            if choices.len()==1 && choices[0].kind=="draw_card")));
            if let EngineState::PendingChoice(PendingChoice::GridSelect {
                candidate_lineages,
                ..
            }) = &mut session.engine_state
            {
                candidate_lineages.pop();
            }
            session.active_combat.as_mut().unwrap().engine_state = session.engine_state.clone();
            assert!(resolved_actions_v1(&session, &bindings).is_err());
        }
    }

    #[test]
    fn headbutt_selection_keeps_duplicate_cards_bound_to_their_discard_lineages() {
        use sts_oracle_run_control::state::{GridSelectReason, PileType};
        let cards = [
            CombatCard::for_scenario(CardId::Strike, 31),
            CombatCard::for_scenario(CardId::Strike, 42),
        ];
        let ids = cards.iter().map(|card| card.lineage).collect::<Vec<_>>();
        let pending = EngineState::PendingChoice(PendingChoice::GridSelect {
            source_pile: PileType::Discard,
            candidate_lineages: ids.clone(),
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: GridSelectReason::MoveToDrawPile,
        });
        let mut combat = blank_test_combat();
        combat.zones.discard_pile = cards.to_vec().into();
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = pending.clone();
        session.active_combat = Some(ActiveCombat::new(
            pending,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let bindings = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                (
                    *id,
                    BoundCardSourceV1 {
                        binding: format!("natural_run/starter/{index}"),
                        source: NaturalRunCardSourceV1::StarterDeck {
                            player_class: "IRONCLAD".to_owned(),
                            source_slot: index as u32,
                        },
                        expected_card_id: "strike".to_owned(),
                        expected_upgrades: 0,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(combat_grid_kind(&session), Some("discard_to_draw"));
        let actions = resolved_actions_v1(&session, &bindings).unwrap();
        assert_eq!(actions.len(), 2);
        for (index, action) in actions.iter().enumerate() {
            assert!(
                matches!(&action.semantic, SemanticActionV1::SubmitSelection { surface: DecisionSurfaceV1::GridSelect, choices }
                if choices.len() == 1 && choices[0].kind == "discard_card" && choices[0].id == bindings[&ids[index]].binding)
            );
            assert!(
                matches!(&action.execution, RustActionExecutionV1::DirectInput { input: ClientInput::SubmitSelection(resolution) }
                if resolution.selected == vec![SelectionTargetRef::CardLineage(ids[index])])
            );
        }
        assert!(resolved_actions_v1(&session, &BTreeMap::new()).is_err());
        for (reason, kind) in [
            (
                GridSelectReason::DiscardToHandRetain,
                "discard_to_hand_retain",
            ),
            (
                GridSelectReason::DiscardToHandNoCostChange,
                "discard_to_hand_no_cost_change",
            ),
        ] {
            if let EngineState::PendingChoice(PendingChoice::GridSelect {
                reason: current, ..
            }) = &mut session.engine_state
            {
                *current = reason;
            } else {
                unreachable!();
            }
            session.active_combat.as_mut().unwrap().engine_state = session.engine_state.clone();
            assert_eq!(combat_grid_kind(&session), Some(kind));
            assert_eq!(
                resolved_actions_v1(&session, &bindings)
                    .unwrap()
                    .into_iter()
                    .map(|action| action.semantic)
                    .collect::<Vec<_>>(),
                actions
                    .iter()
                    .map(|action| action.semantic.clone())
                    .collect::<Vec<_>>()
            );
        }
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .zones
            .discard_pile
            .remove(0);
        assert!(resolved_actions_v1(&session, &bindings).is_err());
    }

    #[test]
    fn exhume_selects_exact_exhaust_lineages_and_excludes_other_exhumes() {
        let first = CombatCard::for_scenario(CardId::Impervious, 31);
        let second = CombatCard::for_scenario(CardId::Impervious, 42);
        let ids = [first.lineage, second.lineage];
        let pending = EngineState::PendingChoice(PendingChoice::GridSelect {
            source_pile: sts_oracle_run_control::state::PileType::Exhaust,
            candidate_lineages: ids.to_vec(),
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: sts_oracle_run_control::state::GridSelectReason::Exhume { upgrade: false },
        });
        let mut combat = blank_test_combat();
        combat.zones.exhaust_pile =
            vec![first, CombatCard::for_scenario(CardId::Exhume, 99), second].into();
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = pending.clone();
        session.active_combat = Some(ActiveCombat::new(
            pending,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomElite,
            }),
        ));
        let bindings = ids
            .iter()
            .enumerate()
            .map(|(index, id)| {
                (
                    *id,
                    BoundCardSourceV1 {
                        binding: format!("natural_run/starter/{index}"),
                        source: NaturalRunCardSourceV1::StarterDeck {
                            player_class: "IRONCLAD".to_owned(),
                            source_slot: index as u32,
                        },
                        expected_card_id: "impervious".to_owned(),
                        expected_upgrades: 0,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let actions = resolved_actions_v1(&session, &bindings).unwrap();
        assert_eq!(actions.len(), 2);
        for (index, action) in actions.iter().enumerate() {
            assert!(
                matches!(&action.semantic, SemanticActionV1::SubmitSelection {
                surface: DecisionSurfaceV1::GridSelect, choices,
            } if choices.len() == 1 && choices[0].kind == "exhaust_card"
                && choices[0].id == bindings[&ids[index]].binding)
            );
            assert!(
                matches!(&action.execution, RustActionExecutionV1::DirectInput {
                input: ClientInput::SubmitSelection(resolution),
            } if resolution.scope == SelectionScope::Grid
                && resolution.selected == vec![SelectionTargetRef::CardLineage(ids[index])])
            );
        }
        assert!(resolved_actions_v1(&session, &BTreeMap::new()).is_err());
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .entities
            .potions = vec![Some(sts_oracle_run_control::content::potions::Potion::new(
            sts_oracle_run_control::content::potions::PotionId::LiquidBronze,
            81,
        ))];
        let with_potion = resolved_actions_v1(&session, &bindings).unwrap();
        assert_eq!(with_potion.len(), 3);
        assert!(with_potion.iter().any(|action|
            matches!(&action.semantic, SemanticActionV1::DiscardPotion { slot: 0, potion_id }
                if potion_id == "liquidbronze")
            && matches!(&action.execution, RustActionExecutionV1::Candidate {
                input: ClientInput::DiscardPotion(0), ..
            })
        ));
        session
            .active_combat
            .as_mut()
            .unwrap()
            .combat_state
            .zones
            .exhaust_pile
            .pop();
        assert!(resolved_actions_v1(&session, &bindings).is_err());
    }

    #[test]
    fn single_card_hand_surfaces_compose_bound_cards_and_potion_discards() {
        let first = CombatCard::for_scenario(CardId::Defend, 41);
        let second = CombatCard::for_scenario(CardId::Defend, 73);
        let first_lineage = first.lineage;
        let second_lineage = second.lineage;
        let pending = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: vec![first_lineage, second_lineage],
            min_cards: 1,
            max_cards: 1,
            can_cancel: false,
            reason: HandSelectReason::Exhaust,
        });
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![second, first];
        combat.entities.potions = vec![
            Some(sts_oracle_run_control::content::potions::Potion::new(
                sts_oracle_run_control::content::potions::PotionId::GamblersBrew,
                81,
            )),
            Some(sts_oracle_run_control::content::potions::Potion::new(
                sts_oracle_run_control::content::potions::PotionId::EnergyPotion,
                82,
            )),
        ];
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = pending.clone();
        session.active_combat = Some(ActiveCombat::new(
            pending,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomElite,
            }),
        ));
        let bindings = BTreeMap::from([
            (
                first_lineage,
                BoundCardSourceV1 {
                    binding: "natural_run/starter/5".to_owned(),
                    source: NaturalRunCardSourceV1::StarterDeck {
                        player_class: "IRONCLAD".to_owned(),
                        source_slot: 5,
                    },
                    expected_card_id: "defend_r".to_owned(),
                    expected_upgrades: 0,
                },
            ),
            (
                second_lineage,
                BoundCardSourceV1 {
                    binding: "natural_run/starter/6".to_owned(),
                    source: NaturalRunCardSourceV1::StarterDeck {
                        player_class: "IRONCLAD".to_owned(),
                        source_slot: 6,
                    },
                    expected_card_id: "defend_r".to_owned(),
                    expected_upgrades: 0,
                },
            ),
        ]);

        let resolved = resolved_actions_v1(&session, &bindings).unwrap();

        assert_eq!(resolved.len(), 4);
        let ids = resolved
            .iter()
            .filter_map(|action| match &action.semantic {
                SemanticActionV1::SubmitSelection {
                    surface: DecisionSurfaceV1::HandSelect,
                    choices,
                } => Some(choices[0].id.as_str()),
                SemanticActionV1::DiscardPotion { .. } => None,
                other => panic!("unexpected hand action {other:?}"),
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            ids,
            BTreeSet::from(["natural_run/starter/5", "natural_run/starter/6"])
        );
        assert_eq!(
            resolved
                .iter()
                .filter_map(|action| match &action.semantic {
                    SemanticActionV1::DiscardPotion { slot, potion_id } => {
                        Some((*slot, potion_id.as_str()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![(0, "gamblersbrew"), (1, "energypotion")]
        );
        assert_eq!(
            resolved
                .iter()
                .filter(|action| matches!(
                    action.execution,
                    RustActionExecutionV1::DirectInput {
                        input: ClientInput::SubmitSelection(_)
                    }
                ))
                .count(),
            2
        );
        let mut put_on_deck = session.clone();
        let EngineState::PendingChoice(PendingChoice::HandSelect { reason, .. }) =
            &mut put_on_deck.engine_state
        else {
            unreachable!()
        };
        *reason = HandSelectReason::PutOnDrawPile;
        put_on_deck.active_combat.as_mut().unwrap().engine_state = put_on_deck.engine_state.clone();
        let put_actions = resolved_actions_v1(&put_on_deck, &bindings).unwrap();
        assert_eq!(
            put_actions.iter().map(|a| &a.semantic).collect::<Vec<_>>(),
            resolved.iter().map(|a| &a.semantic).collect::<Vec<_>>()
        );
        for (min, max, cancel, candidates) in [
            (0, 1, false, 2),
            (1, 2, false, 2),
            (1, 1, true, 2),
            (1, 1, false, 0),
        ] {
            assert!(supported_hand_select_counts(
                candidates,
                min,
                max,
                cancel,
                HandSelectReason::PutOnDrawPile
            )
            .is_err());
        }
        assert_eq!(
            resolved
                .iter()
                .filter(|action| matches!(
                    action.execution,
                    RustActionExecutionV1::Candidate {
                        input: ClientInput::DiscardPotion(_),
                        ..
                    }
                ))
                .count(),
            2
        );
    }

    #[test]
    fn optional_potion_hand_surface_exposes_every_canonical_card_set() {
        let first = CombatCard::for_scenario(CardId::TrueGrit, 41);
        let second = CombatCard::for_scenario(CardId::Defend, 73);
        let first_lineage = first.lineage;
        let second_lineage = second.lineage;
        let pending = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: vec![first_lineage, second_lineage],
            min_cards: 0,
            max_cards: 2,
            can_cancel: true,
            reason: HandSelectReason::GamblingChip,
        });
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![first, second];
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.engine_state = pending.clone();
        session.active_combat = Some(ActiveCombat::new(
            pending,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoomElite,
            }),
        ));
        let bindings = BTreeMap::from([
            (
                first_lineage,
                BoundCardSourceV1 {
                    binding: "natural_run/starter/0".to_owned(),
                    source: NaturalRunCardSourceV1::StarterDeck {
                        player_class: "IRONCLAD".to_owned(),
                        source_slot: 0,
                    },
                    expected_card_id: "truegrit".to_owned(),
                    expected_upgrades: 0,
                },
            ),
            (
                second_lineage,
                BoundCardSourceV1 {
                    binding: "natural_run/starter/5".to_owned(),
                    source: NaturalRunCardSourceV1::StarterDeck {
                        player_class: "IRONCLAD".to_owned(),
                        source_slot: 5,
                    },
                    expected_card_id: "defend".to_owned(),
                    expected_upgrades: 0,
                },
            ),
        ]);

        for reason in [
            HandSelectReason::GamblingChip,
            HandSelectReason::PotionExhaust {
                has_pending_use_effect: false,
            },
            HandSelectReason::PotionExhaust {
                has_pending_use_effect: true,
            },
        ] {
            let EngineState::PendingChoice(PendingChoice::HandSelect {
                reason: selected_reason,
                ..
            }) = &mut session.engine_state
            else {
                unreachable!()
            };
            *selected_reason = reason;
            session.active_combat.as_mut().unwrap().engine_state = session.engine_state.clone();
            let resolved = resolved_actions_v1(&session, &bindings).unwrap();
            let selected_bindings = resolved
                .iter()
                .map(|action| match &action.semantic {
                    SemanticActionV1::SubmitSelection {
                        surface: DecisionSurfaceV1::HandSelect,
                        choices,
                    } => choices
                        .iter()
                        .map(|choice| choice.id.as_str())
                        .collect::<Vec<_>>(),
                    other => panic!("unexpected Gambler action {other:?}"),
                })
                .collect::<Vec<_>>();

            assert_eq!(
                selected_bindings,
                vec![
                    vec![],
                    vec!["natural_run/starter/0"],
                    vec!["natural_run/starter/5"],
                    vec!["natural_run/starter/0", "natural_run/starter/5"],
                ]
            );
            for (min, max, cancel, candidates) in [
                (1, 2, true, 2),
                (0, 1, true, 2),
                (0, 2, false, 2),
                (0, 11, true, 11),
            ] {
                assert!(
                    supported_hand_select_counts(candidates, min, max, cancel, reason).is_err()
                );
            }
        }
    }
}
