use std::fmt;

use super::*;
use crate::CombatSelectionPreferences;
use sts_core::sim::combat_action_equivalence::equivalent_combat_selection_input_v1;

// Separate from policy lane IDs: an intention is editable scheduling state,
// whereas the policy probabilities on already-produced paths stay frozen.
pub(super) const SELECTION_INTENT_LANE: usize = usize::MAX;

/// In-process binding to one generator and its retained exact path. A source
/// shares an immutable position and prefix; it owns no continuation or budget.
/// No serialized identity, card-name matching or replay is involved.
#[derive(Clone)]
pub struct SelectionSource {
    owner: Arc<()>,
    partial: Arc<PartialTurnOption>,
}

impl PartialEq for SelectionSource {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.matches_partial(&other.partial)
    }
}

impl Eq for SelectionSource {}

impl SelectionSource {
    pub fn position(&self) -> &CombatPosition {
        &self.partial.position
    }

    /// Materializes the already-retained prefix, without simulation.
    pub fn actions(&self) -> Vec<TurnOptionAction> {
        self.partial.materialize_actions()
    }

    pub fn action_count(&self) -> usize {
        self.partial.action_depth()
    }

    pub fn first_input(&self) -> Option<&ClientInput> {
        let mut trace = self.partial.trace.as_deref()?;
        while let Some(parent) = trace.parent.as_deref() {
            trace = parent;
        }
        Some(&trace.input)
    }

    /// Associate a completed trace with this exact retained prefix. Position
    /// equality at one later point alone cannot prove which path was taken.
    pub(crate) fn selected_input_in<'a>(
        &self,
        actions: &'a [TurnOptionAction],
    ) -> Option<&'a ClientInput> {
        let selection = &actions.get(self.action_count())?.input;
        if !matches!(selection, ClientInput::SubmitSelection(_))
            && equivalent_combat_selection_input_v1(&self.position().engine, selection).is_none()
        {
            return None;
        }
        let mut cursor = self.partial.trace.as_deref();
        while let Some(trace) = cursor {
            let action = &actions[trace.depth - 1];
            if action.input != trace.input {
                return None;
            }
            let shared_key = action
                .expected_successor_hash
                .exact_key()
                .is_some_and(|key| Arc::ptr_eq(key, &trace.successor_key));
            if !shared_key
                && action.expected_successor_hash
                    != ReplaySuccessorHash::from_exact_key(trace.successor_key.clone())
            {
                return None;
            }
            cursor = trace.parent.as_deref();
        }
        Some(selection)
    }

    pub(super) fn matches_partial(&self, partial: &PartialTurnOption) -> bool {
        same_trace(self.partial.trace.as_ref(), partial.trace.as_ref())
    }

    pub(crate) fn matches_preference(&self, input: &ClientInput, members: &[ClientInput]) -> bool {
        members.contains(input)
            || equivalent_combat_selection_input_v1(&self.position().engine, input)
                .is_some_and(|equivalent| members.contains(&equivalent.input))
    }

    fn continuation_input<'a>(&self, partial: &'a PartialTurnOption) -> Option<&'a ClientInput> {
        let target_depth = self.action_count() + 1;
        let mut trace = partial.trace.as_deref()?;
        while trace.depth > target_depth {
            trace = trace.parent.as_deref()?;
        }
        (trace.depth == target_depth
            && same_trace(self.partial.trace.as_ref(), trace.parent.as_ref()))
        .then_some(&trace.input)
    }
}

fn same_trace(a: Option<&Arc<PendingActionTrace>>, b: Option<&Arc<PendingActionTrace>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub struct SelectionSourcePage {
    pub sources: Vec<SelectionSource>,
    pub total: usize,
}

impl fmt::Debug for SelectionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SelectionSource")
    }
}

#[derive(Clone, Debug)]
pub struct SelectionIntent {
    pub source: SelectionSource,
    pub members: Vec<ClientInput>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectionIntentError {
    StaleSource,
    TooManyMembers { limit: usize },
    InvalidMember { index: usize },
}

impl TurnOptionGeneratorSession {
    fn root_source(&self) -> &SelectionSource {
        // Ordinary generators never retain an extra root snapshot. An analyst
        // read shares live root work when available, otherwise snapshots only
        // this explicitly inspected root. This also avoids a constructor-time
        // Arc::make_mut clone when scheduling installs the initial guides.
        self.root_selection_source.get_or_init(|| {
            let partial = self
                .work
                .iter()
                .flatten()
                .map(work_partial)
                .find(|partial| partial.trace.is_none())
                .cloned()
                .unwrap_or_else(|| {
                    Arc::new(PartialTurnOption {
                        position: self.root.position().clone(),
                        trace: None,
                        atomic_depth: 0,
                        negative_log_policy: 0.0,
                        potion_expenditures: 0,
                        generation_guides: None,
                        trajectory_progress: self.initial_trajectory_progress.clone(),
                    })
                });
            SelectionSource {
                owner: self.selection_source_owner.clone(),
                partial,
            }
        })
    }

    /// Inspection performs neither generation nor replay and grants no budget.
    pub fn root_selection_intent(&self) -> SelectionIntent {
        SelectionIntent {
            source: self.root_source().clone(),
            members: if self.intent_source() == self.root_source() {
                self.selection_intent_members.clone()
            } else {
                Vec::new()
            },
        }
    }

    pub fn selection_intent(&self) -> SelectionIntent {
        SelectionIntent {
            source: self.intent_source().clone(),
            members: self.selection_intent_members.clone(),
        }
    }

    pub(super) fn intent_source(&self) -> &SelectionSource {
        self.selection_intent_source
            .as_ref()
            .unwrap_or_else(|| self.root_source())
    }

    pub fn validate_selection_source(
        &self,
        source: &SelectionSource,
    ) -> Result<(), SelectionIntentError> {
        if Arc::ptr_eq(&source.owner, &self.selection_source_owner) {
            Ok(())
        } else {
            Err(SelectionIntentError::StaleSource)
        }
    }

    /// Lists only currently retained selection positions, plus the root when
    /// selectable. Pages contain at most 16 sources. No historical state bank
    /// is maintained; repeated work sharing a prefix contributes one source.
    pub fn selection_sources(
        &self,
        stepper: &dyn CombatStepper,
        offset: usize,
        limit: usize,
    ) -> SelectionSourcePage {
        let mut seen = HashSet::new();
        let mut retained = Vec::new();
        let mut add = |partial: &Arc<PartialTurnOption>, sequence| {
            if !matches!(partial.position.engine, EngineState::PendingChoice(_)) {
                return;
            }
            let key = partial.trace.as_ref().map_or(std::ptr::null(), Arc::as_ptr);
            if !seen.insert(key) {
                return;
            }
            let surface = crate::automatic_action_surface::automatic_action_surface(
                stepper,
                &partial.position,
            );
            if surface.selection_families.is_empty() {
                return;
            }
            retained.push((
                partial.action_depth(),
                sequence,
                SelectionSource {
                    owner: self.selection_source_owner.clone(),
                    partial: partial.clone(),
                },
            ));
        };
        if matches!(self.root.position().engine, EngineState::PendingChoice(_)) {
            add(&self.root_source().partial, 0);
        }
        for (id, work) in self.work.iter().enumerate() {
            if let Some(work) = work {
                add(work_partial(work), self.work_sequence_ids[id]);
            }
        }
        retained.sort_by_key(|(depth, sequence, _)| (*depth, *sequence));
        SelectionSourcePage {
            total: retained.len(),
            sources: retained
                .into_iter()
                .skip(offset)
                .take(limit.min(16))
                .map(|(_, _, source)| source)
                .collect(),
        }
    }

    pub fn selection_continuation_work(
        &self,
        source: &SelectionSource,
    ) -> Result<usize, SelectionIntentError> {
        self.validate_selection_source(source)?;
        Ok(self
            .work
            .iter()
            .flatten()
            .filter(|work| {
                let partial = work_partial(work);
                source.matches_partial(partial) || source.continuation_input(partial).is_some()
            })
            .count())
    }

    /// Prioritize these root selections and their retained descendants. An
    /// empty slice withdraws the intention. Validation is all-or-nothing.
    /// Completed options, visited states, path probabilities and work budgets
    /// stay intact. Actual exploration occurs only on the next explicit grant.
    pub fn replace_root_selection_intent(
        &mut self,
        source: &SelectionSource,
        members: &[ClientInput],
        stepper: &dyn CombatStepper,
    ) -> Result<(), SelectionIntentError> {
        if source != self.root_source() {
            return Err(SelectionIntentError::StaleSource);
        }
        self.replace_selection_intent(source, members, stepper)
    }

    /// One editable focus within this generator. Changing its source replaces
    /// the prior focus, retaining all ordinary work and completed candidates.
    pub fn replace_selection_intent(
        &mut self,
        source: &SelectionSource,
        members: &[ClientInput],
        stepper: &dyn CombatStepper,
    ) -> Result<(), SelectionIntentError> {
        self.validate_selection_source(source)?;
        if members.len() > CombatSelectionPreferences::LIMIT {
            return Err(SelectionIntentError::TooManyMembers {
                limit: CombatSelectionPreferences::LIMIT,
            });
        }
        if !members.is_empty() {
            let surface = crate::automatic_action_surface::automatic_action_surface(
                stepper,
                source.position(),
            );
            for (index, member) in members.iter().enumerate() {
                let legal = stepper.supports_canonical_pending_choice_actions()
                    && stepper
                        .choice_for_legal_input(source.position(), member)
                        .is_some()
                    && surface.selection_families.iter().any(|family| {
                        SelectionTransactionCursor::new(family)
                            .is_ok_and(|mut cursor| cursor.exclude_input(member))
                    });
                if !legal {
                    return Err(SelectionIntentError::InvalidMember { index });
                }
            }
        }
        let mut unique = Vec::new();
        for member in members {
            if !unique.contains(member) {
                unique.push(member.clone());
            }
        }
        if unique == self.selection_intent_members && source == self.intent_source() {
            return Ok(());
        }
        self.selection_intent_members = unique;
        self.selection_intent_source = if self.selection_intent_members.is_empty() {
            None
        } else {
            Some(source.clone())
        };
        // Keep all frozen ordinary heads. Only the withdrawn intention's head
        // loses its service entitlement; repeatedly editing cannot erase the
        // already-promised fallback service.
        self.scheduled_round
            .retain(|(lane, _)| *lane != SELECTION_INTENT_LANE);
        self.refresh_selection_intent();
        Ok(())
    }

    pub(super) fn refresh_selection_intent(&mut self) {
        self.selection_intent_frontier.clear();
        if self.selection_intent_members.is_empty() {
            self.selection_intent_frontier.shrink_to_fit();
            return;
        }
        // Partition only unseen edges. Previously extracted but withdrawn
        // inputs remain ordinary atomic work; they are never put back behind
        // a cursor's consumed prefix or executed a second time.
        let handles: Vec<_> = (0..self.work.len())
            .filter_map(|id| self.live_work_handle(id))
            .collect();
        let source = self.intent_source().clone();
        for handle in handles {
            let extracted = match self.work[handle.work_id].as_mut().expect("live work") {
                GeneratorWork::AtomicActions(cursor) if source.matches_partial(&cursor.parent) => {
                    cursor.extract_intended(&self.selection_intent_members)
                }
                GeneratorWork::StructuredSelection(selection)
                    if source.matches_partial(&selection.parent) =>
                {
                    selection.extract_intended(&self.selection_intent_members)
                }
                _ => None,
            };
            let Some(extracted) = extracted else { continue };
            let remainder = self.take_live_work(handle);
            if let Some(priority) = work_priority(&remainder) {
                let replacement = self.push_work(remainder, priority);
                // Extraction invalidates an old heap entry, but does not
                // revoke fallback service already frozen in this round.
                for (_, frozen) in &mut self.scheduled_round {
                    if *frozen == handle {
                        *frozen = replacement;
                    }
                }
            }
            let priority = extracted.priority().expect("extracted members retain mass");
            self.push_work(GeneratorWork::AtomicActions(extracted), priority);
        }
        // Reindex existing descendants as well as newly extracted members.
        // No simulation, probability rewrite or visited-state reset occurs.
        self.selection_intent_frontier = self
            .work
            .iter()
            .enumerate()
            .filter_map(|(id, work)| {
                let work = work.as_ref()?;
                self.work_follows_selection_intent(work)
                    .then(|| GeneratorQueueEntry {
                        priority: work_priority(work).expect("live work retains mass"),
                        sequence_id: self.work_sequence_ids[id],
                        work_id: id,
                    })
            })
            .collect();
    }

    pub(super) fn work_follows_selection_intent(&self, work: &GeneratorWork) -> bool {
        if self.selection_intent_members.is_empty() {
            return false;
        }
        let partial = work_partial(work);
        let source = self.intent_source();
        if let Some(input) = source.continuation_input(partial) {
            return source.matches_preference(input, &self.selection_intent_members);
        }
        if !source.matches_partial(partial) {
            return false;
        }
        match work {
            GeneratorWork::Expand(_) => true,
            GeneratorWork::AtomicActions(cursor) => cursor.candidates[cursor.next_candidate..]
                .iter()
                .all(|candidate| {
                    source.matches_preference(&candidate.input, &self.selection_intent_members)
                }),
            GeneratorWork::StructuredSelection(_) => false,
        }
    }

    pub(super) fn peek_selection_intent_work_handle(&mut self) -> Option<GeneratorWorkHandle> {
        while let Some(entry) = self.selection_intent_frontier.peek() {
            let handle = GeneratorWorkHandle {
                work_id: entry.work_id,
                sequence_id: entry.sequence_id,
            };
            if self.is_live_work_handle(handle) {
                return Some(handle);
            }
            self.selection_intent_frontier.pop();
        }
        None
    }

    pub(super) fn reclaim_selection_intent_entries(&mut self) {
        // One membership per live work item at most. Bound stale storage even
        // when the analyst keeps the same intention for a long search.
        if self.selection_intent_frontier.len() > self.live_work_items.saturating_mul(2) {
            let mut entries = std::mem::take(&mut self.selection_intent_frontier);
            entries.retain(|entry| {
                self.is_live_work_handle(GeneratorWorkHandle {
                    work_id: entry.work_id,
                    sequence_id: entry.sequence_id,
                })
            });
            self.selection_intent_frontier = entries;
        }
    }
}

fn work_partial(work: &GeneratorWork) -> &Arc<PartialTurnOption> {
    match work {
        GeneratorWork::Expand(partial) => partial,
        GeneratorWork::AtomicActions(cursor) => &cursor.parent,
        GeneratorWork::StructuredSelection(selection) => &selection.parent,
    }
}

fn work_priority(work: &GeneratorWork) -> Option<GeneratorWorkPriority> {
    match work {
        GeneratorWork::Expand(partial) => Some(GeneratorWorkPriority::for_path(
            partial.atomic_depth,
            partial.negative_log_policy,
        )),
        GeneratorWork::AtomicActions(cursor) => cursor.priority(),
        GeneratorWork::StructuredSelection(selection) => selection.priority(),
    }
}

impl AtomicActionCursorWork {
    fn extract_intended(&mut self, members: &[ClientInput]) -> Option<Self> {
        let remaining = &self.candidates[self.next_candidate..];
        let count = remaining
            .iter()
            .filter(|candidate| members.contains(&candidate.input))
            .count();
        if count == 0 || count == remaining.len() {
            return None;
        }
        let (preferred, remainder): (Vec<_>, Vec<_>) = self
            .candidates
            .drain(self.next_candidate..)
            .partition(|candidate| members.contains(&candidate.input));
        self.candidates.extend(remainder);
        Some(Self {
            parent: self.parent.clone(),
            candidates: preferred,
            next_candidate: 0,
            guides: self.guides.clone(),
        })
    }
}

impl StructuredSelectionWork {
    fn extract_intended(&mut self, members: &[ClientInput]) -> Option<AtomicActionCursorWork> {
        let mass =
            self.remaining_conditional_mass / self.cursor.remaining_input_count().max(1) as f64;
        let candidates: Vec<_> = members
            .iter()
            .filter(|member| self.cursor.exclude_input(member))
            .map(|input| AtomicActionCandidate {
                input: input.clone(),
                conditional_probability: (self.parent.negative_log_policy
                    - self.family_negative_log_policy)
                    .exp()
                    * mass,
                negative_log_policy: self.family_negative_log_policy - mass.ln(),
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        self.remaining_conditional_mass = mass * self.cursor.remaining_input_count() as f64;
        Some(AtomicActionCursorWork {
            parent: self.parent.clone(),
            candidates,
            next_candidate: 0,
            guides: self.parent.generation_guides.clone().unwrap_or_default(),
        })
    }
}
