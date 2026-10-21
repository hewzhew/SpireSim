//! A replaceable structured frontend over the durable private workspace.
//! State, edits and certification stay in the existing workspace owner.
use crate::{ManualDraftWorkspaceV1, ManualSessionV1, RewardPlanStepV1};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, Write};
use sts_parity_lockstep::{ActionViewV1, SemanticActionSpaceV1, SemanticActionV1};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionSelectorV1 {
    Reference {
        reference: String,
    },
    Reward {
        reward: RewardPlanStepV1,
    },
    Card {
        card_id: String,
        #[serde(default)]
        binding: Option<String>,
    },
    EndTurn,
    MatchCell {
        cell: u8,
    },
}

impl DecisionSelectorV1 {
    fn resolve(&self, view: &ActionViewV1) -> Result<SemanticActionV1, String> {
        let candidates = view.actions().actions.iter().filter(|candidate| match self {
            Self::Reference { reference } => candidate.handle == *reference,
            Self::Reward { reward } => matches!(&candidate.action,
                SemanticActionV1::Select { surface: sts_parity_lockstep::DecisionSurfaceV1::CombatReward, choice }
                if choice.kind == "run_reward" && choice.id == match reward {
                    RewardPlanStepV1::Potion => "potion", RewardPlanStepV1::Gold => "gold", RewardPlanStepV1::Cards => "card",
                }),
            Self::Card { card_id, binding } => card_selection_matches(&candidate.action, card_id, binding.as_deref()),
            Self::EndTurn => candidate.action == SemanticActionV1::EndTurn,
            Self::MatchCell { cell } => sts_parity_lockstep::natural_match_flip_v1(*cell).is_ok_and(|a| a == candidate.action),
        }).collect::<Vec<_>>();
        match candidates.as_slice() {
            [one] => Ok(one.action.clone()),
            [] => Err("no legal action matches this selection".into()),
            _ => {
                Err("selection is ambiguous; inspect choices and select one exact reference".into())
            }
        }
    }
}

fn card_selection_matches(action: &SemanticActionV1, card_id: &str, binding: Option<&str>) -> bool {
    use sts_parity_lockstep::{DecisionSurfaceV1, SemanticScalarV1};
    match action {
        SemanticActionV1::PlayCard {
            card_id: id,
            card_binding,
            ..
        } => id == card_id && binding.is_none_or(|binding| binding == card_binding),
        SemanticActionV1::SubmitSelection {
            surface: DecisionSurfaceV1::HandSelect | DecisionSurfaceV1::GridSelect,
            choices,
        } if choices.len() == 1 => {
            matches!(choices[0].attributes.get("card_id"), Some(SemanticScalarV1::Text(id)) if id == card_id)
                && binding.is_none_or(|binding| binding == choices[0].id)
        }
        SemanticActionV1::Select {
            surface: DecisionSurfaceV1::GridSelect,
            choice,
        } if choice.kind == "deck_card" => {
            matches!(choice.attributes.get("card_id"), Some(SemanticScalarV1::Text(id)) if id == card_id)
                && binding.is_none_or(|binding| binding == choice.id)
        }
        SemanticActionV1::Select {
            surface:
                DecisionSurfaceV1::CardReward
                | DecisionSurfaceV1::CombatDiscovery
                | DecisionSurfaceV1::CombatCodex,
            choice,
        } => {
            binding.is_none()
                && matches!(
                    choice.kind.as_str(),
                    "reward_card" | "discovery_card" | "codex_card"
                )
                && choice.id == card_id
        }
        _ => false,
    }
}

#[cfg(test)]
mod selector_tests {
    use super::*;
    use sts_parity_lockstep::{DecisionSurfaceV1, SemanticChoiceV1, SemanticScalarV1};

    #[test]
    fn card_selector_matches_single_choices_and_preserves_binding_and_multi_select_limits() {
        let card = SemanticChoiceV1 {
            kind: "hand_card".into(),
            id: "card/source/1".into(),
            occurrence: 0,
            attributes: [("card_id".into(), SemanticScalarV1::Text("headbutt".into()))].into(),
        };
        for surface in [DecisionSurfaceV1::HandSelect, DecisionSurfaceV1::GridSelect] {
            let single = SemanticActionV1::SubmitSelection {
                surface,
                choices: vec![card.clone()],
            };
            assert!(card_selection_matches(&single, "headbutt", None));
            assert!(card_selection_matches(
                &single,
                "headbutt",
                Some("card/source/1")
            ));
            assert!(!card_selection_matches(
                &single,
                "headbutt",
                Some("card/source/2")
            ));
            assert!(!card_selection_matches(&single, "strike", None));
            let multiple = SemanticActionV1::SubmitSelection {
                surface,
                choices: vec![card.clone(), card.clone()],
            };
            assert!(!card_selection_matches(&multiple, "headbutt", None));
            assert!(!card_selection_matches(
                &SemanticActionV1::SubmitSelection {
                    surface,
                    choices: vec![]
                },
                "headbutt",
                None
            ));
        }
        for (surface, kind) in [
            (DecisionSurfaceV1::CardReward, "reward_card"),
            (DecisionSurfaceV1::CombatDiscovery, "discovery_card"),
            (DecisionSurfaceV1::CombatCodex, "codex_card"),
        ] {
            let offer = SemanticActionV1::Select {
                surface,
                choice: SemanticChoiceV1 {
                    kind: kind.into(),
                    id: "headbutt".into(),
                    occurrence: 0,
                    attributes: Default::default(),
                },
            };
            assert!(card_selection_matches(&offer, "headbutt", None));
            assert!(!card_selection_matches(
                &offer,
                "headbutt",
                Some("card/source/1")
            ));
        }
        let grid = SemanticActionV1::Select {
            surface: DecisionSurfaceV1::GridSelect,
            choice: SemanticChoiceV1 {
                kind: "deck_card".into(),
                ..card.clone()
            },
        };
        assert!(card_selection_matches(
            &grid,
            "headbutt",
            Some("card/source/1")
        ));
        assert!(!card_selection_matches(
            &grid,
            "headbutt",
            Some("card/source/2")
        ));
        assert!(!card_selection_matches(&grid, "strike", None));
        assert!(!card_selection_matches(
            &SemanticActionV1::EndTurn,
            "headbutt",
            None
        ));
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum DecisionOperationV1 {
    Observe,
    Inspect {
        #[serde(default)]
        section: crate::DecisionSectionV1,
    },
    Choose {
        selector: DecisionSelectorV1,
    },
    ResearchTurns {
        request: crate::TurnResearchRequestV1,
        #[serde(default)]
        after: Option<String>,
        #[serde(default)]
        start: crate::TurnResearchStartV1,
    },
    InspectTurn {
        reference: String,
    },
    ChooseTurn {
        reference: String,
    },
    PreviewRewards {
        steps: Vec<RewardPlanStepV1>,
    },
    FlipMatchCells {
        cells: Vec<u8>,
    },
    Undo,
    Mark {
        name: String,
    },
    Switch {
        reference: String,
    },
    Compare {
        reference: String,
    },
    Certify,
    Close,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionRequestV1 {
    pub id: u64,
    #[serde(default)]
    pub view: Option<String>,
    pub operation: DecisionOperationV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionReplyV1 {
    pub id: u64,
    pub status: String,
    pub workspace: String,
    pub node: crate::WorkspaceNodeV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bookmarks: Option<Vec<crate::WorkspaceNodeV1>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison: Option<DecisionComparisonV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub research: Option<crate::TurnResearchPageV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<crate::TurnResearchDetailV1>,
    pub view: String,
    pub selected_actions: usize,
    pub verified_actions: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_stop: Option<String>,
    pub stop: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<SemanticActionSpaceV1>,
    pub choice_labels: std::collections::BTreeMap<String, String>,
    pub elapsed_micros: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<crate::DecisionStateV1>,
    pub unprojected_domains: Vec<sts_parity_lockstep::ParityDomainV1>,
}

pub struct DecisionWorkspaceServiceV1 {
    session: ManualSessionV1,
    draft: ManualDraftWorkspaceV1,
    last: Option<(DecisionRequestV1, DecisionReplyV1)>,
    next_id: u64,
    closed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DecisionComparisonV1 {
    pub current: crate::WorkspaceNodeV1,
    pub other: crate::WorkspaceNodeV1,
    pub common_actions: usize,
    pub different_domains: Vec<sts_parity_lockstep::ParityDomainV1>,
    pub unprojected_domains: Vec<sts_parity_lockstep::ParityDomainV1>,
    pub summary: String,
}

impl DecisionWorkspaceServiceV1 {
    pub fn new(session: ManualSessionV1, draft: ManualDraftWorkspaceV1) -> Self {
        Self {
            session,
            draft,
            last: None,
            next_id: 1,
            closed: false,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn handle(&mut self, request: DecisionRequestV1) -> Result<DecisionReplyV1, String> {
        // One in-flight request per connection. An identical retry of its last
        // receipt never edits twice. Older requests are not reinterpreted.
        if let Some((previous, reply)) = &self.last {
            if previous == &request {
                return Ok(reply.clone());
            }
        }
        let start = std::time::Instant::now();
        let selected_before = self.draft.selected_actions().len();
        let before = self.draft.decision_state()?;
        let view = self.draft.action_view()?;
        let full = matches!(
            request.operation,
            DecisionOperationV1::Observe
                | DecisionOperationV1::Inspect { .. }
                | DecisionOperationV1::Switch { .. }
        );
        let inspect = matches!(request.operation, DecisionOperationV1::Inspect { .. });
        let section = match request.operation {
            DecisionOperationV1::Inspect { section } => section,
            _ => crate::DecisionSectionV1::Position,
        };
        let mut proposal = None;
        let mut comparison = None;
        let mut research = None;
        let mut turn = None;
        let result = (|| {
            if self.closed {
                return Err("workspace connection is closed".into());
            }
            if request.id != self.next_id {
                return Err("out-of-order request; observe with the next request ID".into());
            }
            if !matches!(
                request.operation,
                DecisionOperationV1::Observe | DecisionOperationV1::Close
            ) && request.view.as_deref() != Some(view.reference_prefix())
            {
                return Err(
                    "stale view; use the returned current view before another decision".into(),
                );
            }
            match &request.operation {
                DecisionOperationV1::Observe | DecisionOperationV1::Inspect { .. } => Ok(()),
                DecisionOperationV1::Choose { selector } => {
                    self.draft.choose(selector.resolve(&view)?)
                }
                DecisionOperationV1::ResearchTurns {
                    request,
                    after,
                    start,
                } => {
                    research = Some(self.draft.research_turns_from(
                        request,
                        after.as_deref(),
                        *start,
                    )?);
                    Ok(())
                }
                DecisionOperationV1::InspectTurn { reference } => {
                    turn = Some(self.draft.inspect_turn(reference)?);
                    Ok(())
                }
                DecisionOperationV1::ChooseTurn { reference } => self.draft.choose_turn(reference),
                DecisionOperationV1::PreviewRewards { steps } => {
                    self.draft.preview_rewards(steps.clone())
                }
                DecisionOperationV1::FlipMatchCells { cells } => self.draft.flip_match_cells(cells),
                DecisionOperationV1::Undo => self.draft.undo(),
                DecisionOperationV1::Mark { name } => self.draft.mark(name),
                DecisionOperationV1::Switch { reference } => self.draft.switch_to(reference),
                DecisionOperationV1::Compare { reference } => {
                    comparison = Some(render_comparison(self.draft.compare_with(reference)?)?);
                    Ok(())
                }
                DecisionOperationV1::Certify => {
                    proposal = Some(self.draft.certify(&self.session)?);
                    Ok(())
                }
                DecisionOperationV1::Close => {
                    self.closed = true;
                    Ok(())
                }
            }
        })();
        let after = self.draft.snapshot()?;
        let after_view = self.draft.action_view()?;
        let mut state = self.draft.decision_state()?;
        if section == crate::DecisionSectionV1::Board {
            let room: serde_json::Value =
                crate::manual::decode_domain(&after, sts_parity_lockstep::ParityDomainV1::Room)?;
            if !room["match_game"].is_null() {
                state.room["match_game"] = room["match_game"].clone();
                state.room["layout_source"] = "rust_preview".into();
            }
        }
        let mut visible_actions = section.choices(after_view.actions());
        let preview_stop = self.draft.strategic_preview_stop();
        if preview_stop.is_some() {
            visible_actions.actions.clear();
        }
        let show_choices = inspect || small_action_surface(&after, &visible_actions);
        let choice_labels = if show_choices {
            crate::describe_manual_actions_v1(&after, &visible_actions, &[])?
        } else {
            Default::default()
        };
        let error = result.err();
        let partial = error.is_some() && selected_before != self.draft.selected_actions().len();
        // Errors return the current facts even when the rejected caller missed
        // earlier edits. No presentation text is used to determine a delta.
        let body = if let Some(body) = section.render(&state)?.filter(|_| error.is_none()) {
            body
        } else {
            state.render(
                if full || error.is_some() {
                    None
                } else {
                    Some(&before)
                },
                inspect,
            )?
        };
        let mut summary = format!(
            "Rust: {}",
            if body.is_empty() {
                match &request.operation {
                    DecisionOperationV1::Mark { name } if error.is_none() => {
                        format!("Marked: {name}")
                    }
                    DecisionOperationV1::Close if error.is_none() => "Workspace closed".into(),
                    DecisionOperationV1::Certify if error.is_none() => "Plan saved".into(),
                    _ => "Summary unchanged".into(),
                }
            } else {
                body
            }
        );
        if section.is_action_section() && error.is_none() {
            let menu = crate::render_decision_choices_v1(&visible_actions, &choice_labels);
            summary.push_str(&format!(
                "\nChoices:{}",
                if menu.is_empty() {
                    " none".into()
                } else {
                    format!("\n{menu}")
                }
            ));
        }
        if let Some(error) = &error {
            summary.push_str(&format!(
                "\n{}: {error}",
                if partial {
                    "Partially completed"
                } else {
                    "Rejected"
                }
            ));
        }
        if let Some(reason) = self.draft.replay_stop() {
            summary.push_str(&format!(
                "\nreplay stopped after {} verified actions: {reason}; undo the unverified suffix before continuing",
                self.draft.verified_actions()
            ));
        }
        if let Some(reason) = preview_stop {
            summary.push_str(&format!("\n{reason}"));
        }
        if let Some(comparison) = &comparison {
            summary.clone_from(&comparison.summary);
        }
        if let Some(research) = &research {
            summary.clone_from(&research.summary);
        }
        if let Some(turn) = &turn {
            summary.clone_from(&turn.summary);
        }
        let reply = DecisionReplyV1 {
            id: request.id,
            status: if partial {
                "partial"
            } else if error.is_some() {
                "rejected"
            } else {
                "ok"
            }
            .into(),
            workspace: self.draft.handle().into(),
            node: self.draft.current_node()?,
            bookmarks: if inspect {
                Some(self.draft.bookmarks()?)
            } else {
                None
            },
            comparison,
            research,
            turn,
            view: after_view.reference_prefix().into(),
            selected_actions: self.draft.selected_actions().len(),
            verified_actions: self.draft.verified_actions(),
            replay_stop: self.draft.replay_stop().map(str::to_owned),
            stop: after.boundary_kind.clone(),
            summary,
            error,
            proposal,
            choice_labels,
            choices: if show_choices {
                Some(visible_actions)
            } else {
                None
            },
            elapsed_micros: start.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
            state: inspect.then_some(state),
            unprojected_domains: if pending_combat_unprojected(&after)? {
                vec![sts_parity_lockstep::ParityDomainV1::Combat]
            } else {
                vec![]
            },
        };
        if request.id == self.next_id {
            self.next_id = self.next_id.checked_add(1).ok_or("request ID exhausted")?;
            self.last = Some((request, reply.clone()));
        }
        Ok(reply)
    }
}

fn render_comparison(
    comparison: crate::WorkspaceComparisonV1,
) -> Result<DecisionComparisonV1, String> {
    let different_domains = comparison
        .current_snapshot
        .domains
        .iter()
        .filter(|(domain, value)| comparison.other_snapshot.domains.get(domain) != Some(*value))
        .map(|(domain, _)| *domain)
        .collect::<Vec<_>>();
    let current = comparison
        .current_state
        .render(Some(&comparison.other_state), true)?;
    let other = comparison
        .other_state
        .render(Some(&comparison.current_state), true)?;
    let summary = if current.is_empty() && other.is_empty() {
        "Rust: Matching branch summaries".into()
    } else {
        format!(
            "Rust: Branch comparison\n{}:\n{}\n\n{}:\n{}",
            comparison
                .current
                .name
                .as_deref()
                .unwrap_or("Current branch"),
            if current.is_empty() {
                "Summary unchanged"
            } else {
                &current
            },
            comparison
                .other
                .name
                .as_deref()
                .unwrap_or("Compared branch"),
            if other.is_empty() {
                "Summary unchanged"
            } else {
                &other
            }
        )
    };
    Ok(DecisionComparisonV1 {
        current: comparison.current,
        other: comparison.other,
        common_actions: comparison.common_actions,
        different_domains,
        unprojected_domains: if pending_combat_unprojected(&comparison.current_snapshot)?
            || pending_combat_unprojected(&comparison.other_snapshot)?
        {
            vec![sts_parity_lockstep::ParityDomainV1::Combat]
        } else {
            vec![]
        },
        summary,
    })
}

fn pending_combat_unprojected(
    snapshot: &sts_parity_lockstep::ParitySnapshotV1,
) -> Result<bool, String> {
    if snapshot.boundary_kind != "natural_hand_select" {
        return Ok(false);
    }
    let combat: sts_parity_lockstep::NaturalRunCombatV4 =
        crate::manual::decode_domain(snapshot, sts_parity_lockstep::ParityDomainV1::Combat)?;
    Ok(matches!(
        combat,
        sts_parity_lockstep::NaturalRunCombatV4::Inactive
    ))
}

fn small_action_surface(
    snapshot: &sts_parity_lockstep::ParitySnapshotV1,
    actions: &SemanticActionSpaceV1,
) -> bool {
    if actions.actions.len() > 12 {
        return false;
    }
    if snapshot.boundary_kind == "natural_combat" {
        return true;
    }
    matches!(
        snapshot.boundary_kind.as_str(),
        "natural_hand_select" | "natural_combat_grid_select"
    ) && actions.actions.iter().all(|candidate| {
        matches!(
            &candidate.action,
            SemanticActionV1::SubmitSelection { .. } | SemanticActionV1::DiscardPotion { .. }
        )
    })
}

pub fn serve_decision_workspace_v1(
    mut service: DecisionWorkspaceServiceV1,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<(), String> {
    const MAX_REQUEST: u64 = 1024 * 1024;
    while !service.is_closed() {
        let mut bytes = Vec::new();
        let read = std::io::Read::take(&mut *input, MAX_REQUEST + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        if read as u64 > MAX_REQUEST {
            return Err("workspace request exceeds one MiB".into());
        }
        let request: DecisionRequestV1 = serde_json::from_slice(&bytes)
            .map_err(|e| format!("invalid workspace request: {e}"))?;
        let reply = service.handle(request)?;
        serde_json::to_writer(&mut *output, &reply).map_err(|e| e.to_string())?;
        output
            .write_all(b"\n")
            .and_then(|_| output.flush())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
