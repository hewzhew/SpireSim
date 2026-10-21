//! Factual distance intervals and explicitly approximate one-step targets.
use super::*;
use crate::path_search::routes::RouteLimits;
use crate::CombatBranch;

/// Observed facts at this exact node, not a value label for its continuations.
/// Kept out of numeric features and omitted by the default distance export.
#[derive(Clone, Debug, Serialize)]
pub struct NodeOutcomeEvidence {
    /// Combat-objective terminal status, before applying the resource query.
    /// Some(false) includes Smoke escape; None includes unprepared open states.
    pub combat_terminal: Option<bool>,
    pub combat_smoked: bool,
    pub spent_starting_potion_slots: u64,
    /// An irreversible violation of this query, possibly before combat ends.
    pub required_original_spent: bool,
    /// Only at combat-objective terminals (including escape), never open-state HP.
    pub observed_terminal_outcome: Option<CombatWitnessMaterializedOutcomeV1>,
    /// Some(false) at a defeated or insufficient-resource terminal; None while open.
    pub terminal_satisfies_query: Option<bool>,
    /// The complete root-to-this-terminal input chain has independently replayed.
    /// Does not certify any other descendant or any best-possible outcome.
    pub terminal_replay_verified: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct DistanceRow {
    pub features: Option<EntityPosition>,
    pub children: Vec<(usize, u8)>,
    pub complete: bool,
    pub lower: u32,
    pub upper: u32,
    pub witnessed_remaining_inputs: Option<usize>,
    /// Evidence coordinates, excluded from features.
    pub parent: Option<usize>,
    pub branch: Option<CombatBranch>,
    pub route_decision: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome_evidence: Option<NodeOutcomeEvidence>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DistanceData {
    pub schema: u32,
    pub horizon: u32,
    pub root: CombatPosition,
    pub goal: ResourceRequirement,
    /// Present only for explicit, independently reconstructed reference evidence.
    pub reference_inputs: Option<Vec<ClientInput>>,
    pub rows: Vec<DistanceRow>,
    pub collection_engine_steps: usize,
}

impl DistanceData {
    fn tighten(&mut self) {
        let cap = self.horizon + 1;
        for i in (0..self.rows.len()).rev() {
            if self.rows[i].children.is_empty() {
                continue;
            }
            let upper = self.rows[i]
                .children
                .iter()
                .map(|(j, c)| self.rows[*j].upper.saturating_add(u32::from(*c)).min(cap))
                .min()
                .unwrap();
            self.rows[i].upper = self.rows[i].upper.min(upper);
            if self.rows[i].complete {
                let lower = self.rows[i]
                    .children
                    .iter()
                    .map(|(j, c)| self.rows[*j].lower.saturating_add(u32::from(*c)).min(cap))
                    .min()
                    .unwrap();
                self.rows[i].lower = self.rows[i].lower.max(lower);
            }
        }
    }
}

impl DistanceSearch {
    /// Read-only. Missing work stays as broad intervals, never terminal failure.
    pub fn data(&self) -> DistanceData {
        self.export_data(false)
    }

    /// Add terminal resources and query-cutoff facts to the same graph. Read-only:
    /// does not prepare choices, simulate, replay, evaluate, or change search work.
    /// Export/projection/serialization cost belongs to the caller's measurements.
    pub fn data_with_outcomes(&self) -> DistanceData {
        self.export_data(true)
    }

    fn export_data(&self, include_outcomes: bool) -> DistanceData {
        let cap = self.horizon + 1;
        let rows = self
            .nodes
            .iter()
            .map(|node| {
                let upper = node
                    .witness_upper
                    .map_or(cap, |k| k.min(cap as usize) as u32);
                let lower = node.lower;
                DistanceRow {
                    features: node.features.clone(),
                    children: node
                        .children
                        .iter()
                        .map(|j| (*j, u8::from(self.nodes[*j].input.is_some())))
                        .collect(),
                    complete: node
                        .choices
                        .as_ref()
                        .is_some_and(|c| node.cursor == c.len()),
                    lower,
                    upper,
                    witnessed_remaining_inputs: node.witness_upper,
                    parent: node.parent,
                    branch: node.branch.clone(),
                    route_decision: None,
                    outcome_evidence: include_outcomes.then(|| {
                        let terminal = CombatSearchSpace::default().terminal(&node.state);
                        let outcome = terminal.map(|_| {
                            project_observed_combat_outcome_with_expenditures_v1(
                                &self.root,
                                node.expenditures,
                                node.state.position(),
                            )
                        });
                        NodeOutcomeEvidence {
                            combat_terminal: terminal,
                            combat_smoked: node.state.position().combat.runtime.combat_smoked,
                            spent_starting_potion_slots: node.spent,
                            required_original_spent: node.spent
                                & self.goal.preserve_starting_potion_slots
                                != 0,
                            terminal_satisfies_query: terminal
                                .map(|won| won && self.goal.accepts(outcome.as_ref().unwrap())),
                            observed_terminal_outcome: outcome,
                            terminal_replay_verified: terminal == Some(true)
                                && node.terminal == Some(true)
                                && node.witness_upper == Some(0),
                        }
                    }),
                }
            })
            .collect();
        let mut data = DistanceData {
            schema: 1,
            horizon: self.horizon,
            root: self.root.clone(),
            goal: self.goal.clone(),
            reference_inputs: None,
            rows,
            collection_engine_steps: self.stats.engine_steps,
        };
        data.tighten();
        data
    }
}

fn row(
    root: &CombatPosition,
    goal: &ResourceRequirement,
    state: &CombatSearchState,
    expenditures: CombatInputExpendituresV1,
    horizon: u32,
    witness: Option<usize>,
) -> Result<(DistanceRow, Option<AdmittedChoices>), String> {
    let spent = goal.spent_originals(root, expenditures, state.position());
    let terminal = if spent & goal.preserve_starting_potion_slots != 0 {
        Some(false)
    } else {
        CombatSearchSpace::default().terminal(state)
    };
    let cap = horizon + 1;
    let mut result = DistanceRow {
        features: None,
        children: vec![],
        complete: false,
        lower: 0,
        upper: witness.map_or(cap, |k| k.min(cap as usize) as u32),
        witnessed_remaining_inputs: witness,
        parent: None,
        branch: None,
        route_decision: None,
        outcome_evidence: None,
    };
    if terminal.is_some() {
        let accepted = terminal == Some(true)
            && goal.accepts(&project_observed_combat_outcome_with_expenditures_v1(
                root,
                expenditures,
                state.position(),
            ));
        // A derived terminal alternative is subsequently replayed before export.
        result.lower = if accepted { 0 } else { cap };
        result.upper = result.lower;
        return Ok((result, None));
    }
    let choices = AdmittedChoices::new(root, state, goal)?;
    if choices.len() == 0 {
        return Err("unresolved neighborhood has no admitted choices".into());
    }
    result.features = Some(encode_entities(
        state,
        PathContext {
            root,
            goal,
            spent_originals: spent,
        },
        &choices.branches(),
    )?);
    Ok((result, Some(choices)))
}

/// One-step alternatives at every selected reference decision. This does not
/// search to label every alternative, or import a reference into online search.
/// The checked selected successor is the next source, preserving its known
/// continuation. Other alternatives retain their own unexpanded rows.
pub fn route_neighborhood(
    root: &CombatPosition,
    goal: &ResourceRequirement,
    inputs: &[ClientInput],
    successor: &CombatPosition,
    horizon: u32,
    route_limits: RouteLimits,
) -> Result<DistanceData, String> {
    if horizon == 0 || horizon == u32::MAX {
        return Err("invalid distance range".into());
    }
    let limits = route_limits.step;
    let checked =
        crate::path_search::routes::reconstruct_route(root, goal, inputs, successor, route_limits)?;
    let mut data = DistanceData {
        schema: 1,
        horizon,
        root: checked.root.clone(),
        goal: checked.goal.clone(),
        reference_inputs: Some(checked.inputs.clone()),
        rows: vec![],
        collection_engine_steps: checked.reconstruction_engine_steps + checked.replay_engine_steps,
    };
    let mut continuation: Option<usize> = None;
    for (decision_index, decision) in checked.decisions.iter().enumerate() {
        let remaining = checked.inputs.len() - decision.input_index;
        let (mut source, choices) = row(
            &checked.root,
            &checked.goal,
            &decision.state,
            decision.expenditures_before,
            horizon,
            Some(remaining),
        )?;
        source.route_decision = Some(decision_index);
        let parent = if let Some(index) = continuation.take() {
            source.parent = data.rows[index].parent;
            source.branch = data.rows[index].branch.clone();
            data.rows[index] = source;
            index
        } else {
            let index = data.rows.len();
            data.rows.push(source);
            index
        };
        let choices = choices.ok_or("reference decision is terminal")?;
        for choice in 0..choices.len() {
            let input = choices.input(choice)?;
            let (state, used) = match choices.transition(choice, limits)? {
                Transition::Settled {
                    state,
                    engine_steps,
                } => (state, engine_steps),
                Transition::Interrupted { cause, .. } => {
                    return Err(format!("neighborhood interrupted: {cause:?}"))
                }
            };
            data.collection_engine_steps += used;
            let cost = u8::from(input.is_some());
            let mut expenditures = decision.expenditures_before;
            if let Some(input) = &input {
                expenditures.record(input);
            }
            let witness = (choice == decision.selected).then_some(remaining - usize::from(cost));
            let (mut child, _) = row(
                &checked.root,
                &checked.goal,
                &state,
                expenditures,
                horizon,
                witness,
            )?;
            if child.lower == 0 && child.upper == 0 {
                let mut inputs = checked.inputs[..decision.input_index].to_vec();
                inputs.extend(input.clone());
                let (_, steps) = crate::continuation::validate_reference_trajectory(
                    &checked.root,
                    &inputs,
                    state.position(),
                    limits,
                )?;
                let outcome = materialize_replay_checked_combat_outcome_v1(
                    &checked.root,
                    &inputs,
                    state.position(),
                )?;
                if !checked.goal.accepts(&outcome) {
                    return Err("neighborhood victory misses query".into());
                }
                data.collection_engine_steps += steps;
                child.witnessed_remaining_inputs = Some(0);
            }
            child.parent = Some(parent);
            child.branch = Some(choices.branch(choice).clone());
            let child_index = data.rows.len();
            if choice == decision.selected {
                if let Some(next) = checked.decisions.get(decision_index + 1) {
                    // Source binding uses the full grammar state, input offset
                    // and expenditure ledger, never feature or card-name equality.
                    if state != next.state
                        || expenditures != next.expenditures_before
                        || decision.input_index + usize::from(cost) != next.input_index
                    {
                        return Err("reference neighborhood continuation mismatch".into());
                    }
                    continuation = Some(child_index);
                } else if state.position() != &checked.successor || child.upper != 0 {
                    return Err("reference neighborhood misses checked victory".into());
                }
            }
            data.rows[parent].children.push((child_index, cost));
            data.rows.push(child);
        }
        data.rows[parent].complete = true;
    }
    data.tighten();
    Ok(data)
}
