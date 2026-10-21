//! Full-information, whole-combat search. Optional frozen retention evaluation.
//! Width passes share work while preserving their unfinished frontiers.
//! A stopped search never proves that combat is lost.

mod actions;
mod audit;
mod families;
mod graph;
mod inspection;
mod prefixes;
mod refinement;
mod retention;
mod sampling;
mod shuffle;
mod tactics;
#[cfg(test)]
mod tests;

pub use audit::{
    AuditCount, PassAuditReport, QuotaAudit, QuotaTurnAudit, ReferenceStep, RetentionAudit,
    RetentionPeer, SelectionAudit, SolverAudit, StepAudit, WorkAudit,
};
pub use families::OrderFamilyOrigin;
pub use inspection::RetentionInspection;
pub use prefixes::{SolverPrefix, SolverPrefixPool};
pub use refinement::{OrderEditOrigin, PotionEditOrigin};
pub use retention::draw_triggers::{
    DrawTriggerPayoff, DrawTriggerProfile, DrawTriggerSource, DrawTriggerZone,
};
pub use retention::recovery::{
    RecoveryCardSource, RecoveryCredit, RecoveryDependency, RecoveryGrowthSource, RecoveryProfile,
    RecoverySource, RecoveryZone,
};
pub use sampling::{CandidateSample, CandidateSampleGroup, CandidateSamples};
pub use shuffle::{ShufflePlanOrigin, ShuffleProgress};

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use sts_combat_outcome::combat_witness_materialization::{
    materialize_replay_checked_combat_outcome_v1,
    project_observed_combat_outcome_with_expenditures_v1, CombatInputExpendituresV1,
    CombatWitnessMaterializedOutcomeV1,
};
use sts_core::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper};
use sts_core::state::core::{ClientInput, EngineState};

use crate::{CombatBranch, CombatSearchSpace, CombatSearchState, SearchSpace, Transition};

#[derive(Clone, Debug, Serialize)]
pub struct SolverConfig {
    pub initial_width: usize,
    pub max_width: usize,
    /// Each turn receives width * this many settled branches before passing
    /// work to the next turn. Pending selections count too; no eager products.
    pub transitions_per_width: usize,
    pub max_engine_steps_per_input: usize,
    /// Caps materialized expenditures, including discarded starting potions
    /// and passive consumption; generation/legality remains simulator-owned.
    pub max_potion_uses: Option<usize>,
    /// Coordinate requested widths across potion/card-only families. False is
    /// fixed-work round-robin between families. Live widths rotate within each
    /// family in either mode; neither mode changes the retained state scores.
    pub balance_beam_widths: bool,
    /// Allocate representatives across hands, energy, effects, enemy phases
    /// and relic counters before retaining additional order/RNG variants.
    /// Disabling this restores the coarse inventory/enemy-progress allocation.
    pub diversify_decision_contexts: bool,
    /// Experimental next-turn diversity for remaining/exhausted card kinds,
    /// upgrades and persistent values. Does not merge exact states or change scores.
    pub preserve_card_compositions: bool,
    /// Experimental scheduling: round-robin action completion jobs alongside
    /// ordinary plays. Within-turn unfinished effects bypass score retention;
    /// start-of-turn choices cross the turn layer before using the new quota.
    pub complete_actions: bool,
    /// Experimental: continue through a newly introduced hand option or player
    /// effect until a completed followup opens no further opportunity, or the
    /// turn ends. Includes action completion and shares the original budget.
    pub follow_preparations: bool,
    /// Propose omitted short kills, reactive root effect orders and current-turn
    /// copy/refill/protection responses. Inputs execute inside the normal work
    /// budget without scoring between recipe steps; ordinary choices remain.
    pub tactical_attacks: bool,
    /// Use the simulator's conservative action-equivalence contracts: adjacent
    /// equal starter instances and unordered Retain member permutations. Other
    /// instances and ordered selections remain separate. This is not state merging.
    pub collapse_equivalent_actions: bool,
    /// Try one potion insertion/removal along verified ordinary-search wins.
    /// Source tracing, changed suffixes and validation share the work budget.
    /// All retained wins also receive bounded checks of their existing discards.
    pub refine_potion_plans: bool,
    /// Swap adjacent complete card actions in verified ordinary-search wins.
    /// Shares source replay, scheduling and validation with potion edits.
    pub refine_card_order: bool,
    /// Frozen, explicitly scoped resource prediction for the fourth retention lane.
    /// Does not change legality, exact deduplication or terminal resource order.
    pub resource_model: Option<crate::resource_value::ValueModel>,
    /// Continuation work per mapped order after an independently verified win.
    /// Zero disables this Oracle improvement pass. All its work shares advance's
    /// budget; child continuations never recursively start another repair pass.
    pub shuffle_continuation_work: usize,
    /// Experimental work per tied pre-shuffle candidate. Zero disables these
    /// card-only continuations; child work and original-root replay are charged.
    pub order_family_work: usize,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            initial_width: 4,
            max_width: 128,
            transitions_per_width: 64,
            max_engine_steps_per_input: 512,
            max_potion_uses: None,
            balance_beam_widths: true,
            diversify_decision_contexts: true,
            preserve_card_compositions: false,
            complete_actions: false,
            follow_preparations: false,
            tactical_attacks: true,
            collapse_equivalent_actions: true,
            refine_potion_plans: true,
            refine_card_order: true,
            resource_model: None,
            shuffle_continuation_work: 0,
            order_family_work: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SolverStop {
    WorkBudget,
    TimeBudget,
    /// All configured beam passes ended, not exhaustive legal-tree coverage.
    ScheduleComplete,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SolverStats {
    pub search_transitions: usize,
    /// Ordinary search only, including cache-served branches. Child searches
    /// and fixed-suffix edits have their own work owners.
    pub potion_search: PotionSearchStats,
    /// Subset of search transitions allocated to the independent card-only beam.
    pub card_only_transitions: usize,
    pub replay_transitions: usize,
    /// Settled search transitions served without executing the engine again.
    /// Hits still debit one search work unit and the producing pass's quota.
    pub transition_cache_hits: usize,
    pub graph_edges_recorded: usize,
    pub graph_edges_evicted: usize,
    pub graph_peak_edges: usize,
    pub result_backup_work: usize,
    /// Subset of result_backup_work: bounded nonterminal bookkeeping quanta.
    pub boundary_backup_work: usize,
    /// Actual seed/envelope operations, separately from charged quanta.
    pub boundary_backup_operations: usize,
    pub observed_turn_boundaries: usize,
    pub boundary_backup_updates: usize,
    pub boundary_updates_coalesced: usize,
    pub boundary_retention_comparisons: usize,
    pub boundary_retention_reorders: usize,
    pub backed_return_updates: usize,
    pub backed_return_limit_dropped: usize,
    pub backed_queue_limit_dropped: usize,
    pub backed_peak_records: usize,
    pub backed_branch_reorders: usize,
    pub backed_retention_batches: usize,
    pub backed_retention_candidates: usize,
    pub backed_retention_nominations: usize,
    /// Repeated or covered feasible fronts omitted only from the guidance lane.
    pub backed_retention_suppressed_nominations: usize,
    /// Actual slots awarded through guidance, not rescued candidates.
    pub backed_retention_selected: usize,
    pub engine_steps: usize,
    pub passes_started: usize,
    pub turn_layers: usize,
    pub deepest_turn: u32,
    pub beam_dropped: usize,
    pub turn_budget_dropped: usize,
    pub quota_cursors_saved: usize,
    pub quota_cursors_resumed: usize,
    pub quota_cursors_dropped: usize,
    pub exact_duplicates: usize,
    pub equivalent_actions_skipped: usize,
    pub attack_plans_started: usize,
    /// Subset of search_transitions, not additional uncharged simulation.
    pub attack_plan_transitions: usize,
    pub attack_plans_achieved: usize,
    pub attack_plans_aborted: usize,
    /// Recipe rejected when the bounded quota-cursor reserve is full.
    pub attack_plans_quota_stopped: usize,
    /// Root effect-order hypotheses; their charged work is a subset of search_transitions.
    pub effect_plans_started: usize,
    pub effect_plan_transitions: usize,
    pub effect_plans_completed: usize,
    pub effect_plans_aborted: usize,
    pub effect_plans_quota_stopped: usize,
    /// Current-turn copy/refill/protection hypotheses, charged as ordinary search work.
    pub response_plans_started: usize,
    pub response_plan_transitions: usize,
    pub response_plans_completed: usize,
    pub response_plans_aborted: usize,
    pub response_plans_quota_stopped: usize,
    pub resource_evaluations: usize,
    pub observed_wins: usize,
    pub replayed_wins: usize,
    pub inferior_wins_skipped: usize,
    pub resource_limit_wins_skipped: usize,
    /// Materialized states rejected immediately after an irreversible potion
    /// expenditure, before retention or any continuation work.
    pub resource_limit_states_skipped: usize,
    /// Explicit use/discard actions excluded by the monotone expenditure bound.
    pub resource_limit_actions_skipped: usize,
    pub pending_actions_started: usize,
    /// Subset of search_transitions spent in action/continuation tasks.
    pub pending_action_work: usize,
    /// Single-card hands admitted to the existing action cursor because their
    /// only permitted choices are EndTurn and one concrete card play.
    pub narrow_card_choices: usize,
    /// Subset of pending_action_work starting at a completed game action:
    /// actual followups, rather than unfinished selection assembly.
    pub followup_transitions: usize,
    /// Generated nonterminal hand/effect opportunities offered a continuation.
    pub preparation_opportunities: usize,
    pub completed_action_alternatives: usize,
    pub pending_action_limit_dropped: usize,
    pub observed_losses: usize,
    pub observed_escapes: usize,
    /// Subset of search transitions after their producing pass's primary work.
    pub recovery_transitions: usize,
    /// Nodes admitted to the bounded reserve, including later replacements.
    pub deferred_nodes: usize,
    /// Exact saved prefixes whose continuation has been reopened.
    pub resumed_nodes: usize,
    /// Incoming reserve candidates rejected because the per-pass cap was full.
    pub deferred_limit_dropped: usize,
    /// Peak node count, not a bound on bytes or trace depth.
    pub max_deferred_nodes_per_pass: usize,
    pub shuffle_work: usize,
    /// Disjoint subsets whose sum is shuffle_work.
    pub shuffle_source_work: usize,
    pub shuffle_prefix_work: usize,
    pub shuffle_suffix_work: usize,
    pub shuffle_continuation_work: usize,
    pub shuffle_continuations_queued: usize,
    pub shuffle_duplicate_continuations: usize,
    pub shuffle_jobs: usize,
    pub shuffle_candidates: usize,
    pub shuffle_rejected: usize,
    pub shuffle_unmapped_boundaries: usize,
    pub shuffle_observed_wins: usize,
    pub shuffle_retained_plans: usize,
    pub plan_edit_work: usize,
    /// Disjoint subsets summing to plan_edit_work. Source replay is shared by
    /// potion and card-order edits and cannot be attributed to either alone.
    pub plan_edit_source_work: usize,
    pub plan_edit_potion_work: usize,
    pub plan_edit_order_work: usize,
    pub plan_edit_jobs: usize,
    pub plan_edit_candidates: usize,
    pub plan_edit_rejected: usize,
    /// Fixed-suffix replays stopped at a fully equal, already completed turn
    /// boundary. No approximate state or discard-order equivalence is used.
    pub plan_edit_suffix_cache_hits: usize,
    pub potion_edit_retained_plans: usize,
    pub order_edit_retained_plans: usize,
    /// Child search and child replay; original-root replay remains replay_transitions.
    pub order_family_work: usize,
    /// Selected families with at least one newly queued member (not unique globally).
    pub order_families_admitted: usize,
    pub order_family_candidates: usize,
    pub order_family_duplicates: usize,
    /// Child roots already completed at this allowance through an equal/shorter
    /// prefix, with the same full search state and original-root expenditures.
    pub order_family_cached: usize,
    pub order_family_limit_dropped: usize,
    pub order_family_jobs: usize,
    pub order_family_completed: usize,
    pub order_family_observed_wins: usize,
    pub order_family_retained_plans: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PotionSearchStats {
    pub use_transitions: usize,
    pub discard_transitions: usize,
    /// Subsets served without executing the engine again.
    pub cached_use_transitions: usize,
    pub cached_discard_transitions: usize,
    /// Work from prefixes with an explicit potion expenditure, not a claim
    /// that this work would disappear if a potion module were introduced.
    pub after_expenditure_transitions: usize,
    pub after_expenditure_engine_steps: usize,
}

impl SolverStats {
    pub fn work(&self) -> usize {
        self.search_transitions
            + self.replay_transitions
            + self.shuffle_work
            + self.plan_edit_work
            + self.order_family_work
            + self.result_backup_work
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SolverPlanOrigin {
    pub width: usize,
    pub card_only: bool,
    /// The producing pass was continuing a saved prefix after primary search.
    pub recovery: bool,
    /// Global work when the endpoint was observed, before independent replay.
    pub observed_at_work: usize,
    pub pass_search_work: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shuffle: Option<ShufflePlanOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub potion_edit: Option<PotionEditOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_edit: Option<OrderEditOrigin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_family: Option<OrderFamilyOrigin>,
}

/// Read-only allocation facts for one retained pass. Queue sizes count stored
/// prefixes, not distinct states or a bound on unsearched possibilities.
#[derive(Clone, Debug, Serialize)]
pub struct SolverPassProgress {
    pub width: usize,
    pub card_only: bool,
    pub search_work: usize,
    pub primary_complete: bool,
    pub complete: bool,
    pub active_nodes: usize,
    pub pending_nodes: usize,
    pub pending_actions: usize,
    /// Queued and quota-suspended recipes plus a currently prepared recipe.
    pub attack_plans: usize,
    pub effect_plans: usize,
    pub response_plans: usize,
    /// Pending pre-shuffle continuations plus this pass's active child, if any.
    pub order_family_candidates: usize,
    pub next_action_nodes: usize,
    pub next_turn_nodes: usize,
    pub deferred_nodes: usize,
    pub deferred_cursors: usize,
    pub remaining_branches: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct SolverPlan {
    pub inputs: Vec<ClientInput>,
    pub outcome: CombatWitnessMaterializedOutcomeV1,
    pub successor: CombatPosition,
    pub found_at_work: usize,
    /// Minimum at replayed stable boundaries; not the within-action minimum.
    pub min_stable_hp: i32,
    /// Diagnostic provenance, never an outcome ranking or replay credential.
    pub origin: SolverPlanOrigin,
}

fn sort_plans_for_display(plans: &mut [SolverPlan]) {
    use sts_combat_outcome::combat_witness_materialization::materialized_claimable_gold_v1;
    // Presentation only; never remove incomparable resource alternatives.
    // At equal HP and expenditure, resource facts precede action count.
    plans.sort_by_key(|p| {
        (
            std::cmp::Reverse(p.outcome.final_hp),
            p.outcome.total_potion_expenditures,
            p.outcome.combat_mugged,
            p.outcome.persistent.unrecovered_stolen_gold,
            std::cmp::Reverse(materialized_claimable_gold_v1(&p.outcome)),
            p.inputs.len(),
        )
    });
}

/// One observed losing branch for diagnosis, not a claim that the root loses.
#[derive(Clone, Debug, Serialize)]
pub struct SolverLossSample {
    pub inputs: Vec<ClientInput>,
    pub successor: CombatPosition,
}

#[derive(Clone)]
struct Trace {
    previous: Option<Arc<Trace>>,
    input: ClientInput,
    expenditures: CombatInputExpendituresV1,
}

#[derive(Clone)]
struct Node {
    state: CombatSearchState,
    card_baseline: Arc<retention::CardBaseline>,
    potion_budget: Arc<PotionBudget>,
    trace: Option<Arc<Trace>>,
    input_count: usize,
}

#[derive(Clone)]
struct PotionBudget {
    starting: Vec<Option<sts_core::content::potions::Potion>>,
    limit: Option<usize>,
}

impl Node {
    fn root(state: CombatSearchState) -> Self {
        Self {
            card_baseline: Arc::new(retention::CardBaseline::new(&state.position().combat)),
            potion_budget: Arc::new(PotionBudget {
                starting: state.position().combat.entities.potions.clone(),
                limit: None,
            }),
            state,
            trace: None,
            input_count: 0,
        }
    }

    fn with_potion_limit(mut self, limit: Option<usize>) -> Self {
        Arc::make_mut(&mut self.potion_budget).limit = limit;
        self
    }

    fn remaining_potion_budget(&self) -> Option<usize> {
        self.potion_budget
            .limit
            .map(|limit| limit.saturating_sub(self.potion_expenditures()))
    }

    fn potion_expenditures(&self) -> usize {
        self.expenditures()
            .potion_expenditures(
                &self.potion_budget.starting,
                &self.state.position().combat.entities.potions,
            )
            .1 as usize
    }

    fn exceeds_potion_budget(&self) -> bool {
        self.potion_budget
            .limit
            .is_some_and(|limit| self.potion_expenditures() > limit)
    }

    fn expenditures(&self) -> CombatInputExpendituresV1 {
        self.trace
            .as_ref()
            .map(|trace| trace.expenditures)
            .unwrap_or_default()
    }

    /// A partial index key, never state identity. Every user of this key must
    /// compare complete states inside the bucket, including RNG and selections.
    fn index_key(&self) -> u64 {
        state_index_key(&self.state)
    }

    fn inputs(&self) -> Vec<ClientInput> {
        let mut inputs = Vec::with_capacity(self.input_count);
        let mut trace = self.trace.as_deref();
        while let Some(t) = trace {
            inputs.push(t.input.clone());
            trace = t.previous.as_deref();
        }
        inputs.reverse();
        inputs
    }
}

fn state_index_key(state: &CombatSearchState) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    let c = &state.position().combat;
    (
        c.turn.turn_count,
        c.turn.energy,
        c.entities.player.current_hp,
        c.entities.player.block,
        c.zones.hand.len(),
        c.zones.draw_pile.len(),
        c.zones.discard_pile.len(),
        c.zones.exhaust_pile.len(),
    )
        .hash(&mut hash);
    for card in c
        .zones
        .hand
        .iter()
        .chain(c.zones.draw_pile.iter())
        .chain(c.zones.discard_pile.iter())
        .chain(c.zones.exhaust_pile.iter())
    {
        card.hash(&mut hash);
    }
    state.selection_family().hash(&mut hash);
    state.selected_members().hash(&mut hash);
    state.potion_uses().hash(&mut hash);
    hash.finish()
}

struct Expansion {
    node: Node,
    graph_key: graph::Key,
    choices: crate::combat::PreparedCombatChoices,
    branches: Vec<usize>,
    index: usize,
    tactical_plan: Option<tactics::TacticalPlan>,
    backed_revision: u64,
}

struct Replay {
    origin: SolverPlanOrigin,
    expected: CombatPosition,
    inputs: Vec<ClientInput>,
    current: CombatSearchState,
    boundaries: Vec<graph::Key>,
    expenditures: CombatInputExpendituresV1,
    index: usize,
    min_stable_hp: i32,
}

/// One width's search, including partially expanded actions and turn quotas.
struct Pass {
    audit: Option<audit::PassAudit>,
    samples: Option<std::sync::Arc<std::sync::Mutex<sampling::Sampler>>>,
    card_only: bool,
    width: usize,
    active: VecDeque<Node>,
    tactics_enabled: bool,
    families_enabled: bool,
    order_families: families::Queue,
    tactical: VecDeque<tactics::TacticalJob>,
    effect_root_pending: bool,
    effect_proposals: Vec<crate::observation::consequences::PlanProposal>,
    service_tactical: bool,
    // Only a family child's first width is armed. Wait for its charged, real
    // next-turn draw; do not reroot, inspect future piles, or repeat per width.
    first_draw_attack_turn: Option<u32>,
    expansion: Option<Expansion>,
    next: Vec<Node>,
    pending: VecDeque<actions::PendingAction>,
    resolving: Option<actions::PendingAction>,
    ordinary: Option<Expansion>,
    service_pending: bool,
    yield_expansion: bool,
    ended: Vec<Node>,
    // Nodes and exact unfinished cursors share the existing slot bound.
    // Prepared work can replace unexpanded omissions, never another cursor.
    deferred: VecDeque<Node>,
    deferred_cursors: VecDeque<actions::DeferredCursor>,
    // This index narrows equality checks; hash equality never merges states.
    deferred_index: HashMap<u64, Vec<CombatSearchState>>,
    primary_done: bool,
    turn_work: usize,
    search_work: usize,
    last_serviced_at: usize,
    done: bool,
}

impl Pass {
    /// A tactical recipe may arrive beyond the ordinary frontier. Retain
    /// those exact nodes, but score only one actual player turn at a time.
    fn take_layer(pool: &mut Vec<Node>, turn: u32) -> Vec<Node> {
        let (current, other) = std::mem::take(pool)
            .into_iter()
            .partition(|node: &Node| node.state.position().combat.turn.turn_count == turn);
        *pool = other;
        current
    }

    fn take_earliest_layer(pool: &mut Vec<Node>) -> Vec<Node> {
        let turn = pool
            .iter()
            .map(|node| node.state.position().combat.turn.turn_count)
            .min()
            .unwrap();
        Self::take_layer(pool, turn)
    }

    fn new(root: Node, width: usize, card_only: bool) -> Self {
        Self {
            audit: None,
            samples: None,
            card_only,
            width,
            active: VecDeque::from([root]),
            tactics_enabled: false,
            families_enabled: false,
            order_families: families::Queue::default(),
            tactical: VecDeque::new(),
            effect_root_pending: true,
            effect_proposals: vec![],
            service_tactical: true,
            first_draw_attack_turn: None,
            expansion: None,
            next: vec![],
            pending: VecDeque::new(),
            resolving: None,
            ordinary: None,
            service_pending: true,
            yield_expansion: false,
            ended: vec![],
            deferred: VecDeque::new(),
            deferred_cursors: VecDeque::new(),
            deferred_index: HashMap::new(),
            primary_done: false,
            turn_work: 0,
            search_work: 0,
            last_serviced_at: 0,
            done: false,
        }
    }

    fn pop_deferred(&mut self) -> Option<Node> {
        let node = self.deferred.pop_front()?;
        self.remove_deferred_index(&node);
        Some(node)
    }

    fn remove_deferred_index(&mut self, node: &Node) {
        let key = node.index_key();
        let bucket = self.deferred_index.get_mut(&key).unwrap();
        let index = bucket
            .iter()
            .position(|state| *state == node.state)
            .unwrap();
        bucket.swap_remove(index);
        if bucket.is_empty() {
            self.deferred_index.remove(&key);
        }
    }

    fn defer(&mut self, nodes: impl IntoIterator<Item = Node>, stats: &mut SolverStats) {
        for node in nodes {
            if self.deferred.len() + self.deferred_cursors.len() == self.width * 8 {
                if let Some(audit) = &mut self.audit {
                    audit.note(&node, "reserve_full", stats.work());
                }
                stats.deferred_limit_dropped += 1;
                continue;
            }
            let key = node.index_key();
            let bucket = self.deferred_index.entry(key).or_default();
            if bucket.iter().any(|state| *state == node.state) {
                if let Some(audit) = &mut self.audit {
                    audit.note(&node, "reserve_duplicate", stats.work());
                }
                continue;
            }
            if let Some(audit) = &mut self.audit {
                audit.note(&node, "deferred", stats.work());
            }
            bucket.push(node.state.clone());
            self.deferred.push_back(node);
            stats.deferred_nodes += 1;
            stats.max_deferred_nodes_per_pass =
                stats.max_deferred_nodes_per_pass.max(self.deferred.len());
        }
    }

    fn prepare(
        &mut self,
        space: &CombatSearchSpace,
        config: &SolverConfig,
        stats: &mut SolverStats,
        guide: Option<graph::RetentionGuide<'_>>,
    ) -> Result<bool, String> {
        if self.done {
            return Ok(false);
        }
        self.tactics_enabled = config.tactical_attacks;
        self.families_enabled = config.order_family_work > 0;
        self.offer_root_effect_plans(stats);
        if config.complete_actions
            || config.follow_preparations
            || self.resolving.is_some()
            || !self.pending.is_empty()
            || self.ordinary.is_some()
        {
            if self.yield_expansion {
                self.park_expansion(config.follow_preparations);
                self.yield_expansion = false;
            } else if self.expansion.is_some() {
                return Ok(true);
            }
        } else if self
            .expansion
            .as_ref()
            .is_some_and(|e| e.index < e.branches.len())
        {
            return Ok(true);
        }
        self.expansion = None;
        if !self.ended.is_empty() && self.turn_work >= self.width * config.transitions_per_width {
            self.defer_turn(stats);
        }
        if self.service_tactical
            && self.resolving.is_none()
            && self.prepare_tactical_plan(space, stats)?
        {
            self.service_tactical = false;
            return Ok(true);
        }
        self.service_tactical = true;
        if self.active.is_empty() && self.ordinary.is_none() {
            if !self.next.is_empty() {
                let pool = Self::take_earliest_layer(&mut self.next);
                let (kept, omitted) = self.partition(
                    pool,
                    self.width,
                    stats,
                    config.diversify_decision_contexts,
                    false,
                    config.resource_model.as_ref(),
                    "next_action",
                    guide,
                );
                self.active = kept.into();
                self.defer(omitted, stats);
            } else if !self.pending.is_empty() {
                // Unresolved jobs still belong to this turn. A completed
                // alternative above can continue without draining all jobs.
            } else if !self.tactical.is_empty() {
                return self.prepare_tactical_plan(space, stats);
            } else if !self.ended.is_empty() {
                let pool = Self::take_earliest_layer(&mut self.ended);
                let (kept, omitted) = self.partition(
                    pool,
                    self.width,
                    stats,
                    config.diversify_decision_contexts,
                    config.preserve_card_compositions,
                    config.resource_model.as_ref(),
                    "next_turn",
                    guide,
                );
                self.active = kept.into();
                self.defer(omitted, stats);
                self.turn_work = 0;
                stats.turn_layers += 1;
            } else {
                self.primary_done = true;
                if let Some(cursor) = self.deferred_cursors.pop_front() {
                    if let Some(audit) = &mut self.audit {
                        for node in cursor.nodes() {
                            audit.note(node, "cursor_resumed", stats.work());
                        }
                    }
                    stats.quota_cursors_resumed += 1;
                    self.turn_work = 0;
                    match cursor {
                        actions::DeferredCursor::Ordinary(e) => self.ordinary = Some(e),
                        actions::DeferredCursor::Pending(task) => self.pending.push_back(task),
                        actions::DeferredCursor::Tactical(job) => self.tactical.push_back(job),
                    }
                    return self.prepare(space, config, stats, guide);
                }
                if let Some(node) = self.pop_deferred() {
                    if let Some(audit) = &mut self.audit {
                        audit.note(&node, "resumed", stats.work());
                    }
                    self.active.push_back(node);
                    self.turn_work = 0;
                    stats.resumed_nodes += 1;
                } else {
                    self.done = true;
                    return Ok(false);
                }
            }
        }
        if self.prepare_resolution(space, config, stats)? {
            return Ok(true);
        }
        let node = self.active.pop_front().unwrap();
        if self
            .first_draw_attack_turn
            .is_some_and(|turn| node.state.position().combat.turn.turn_count >= turn)
        {
            let turn = self.first_draw_attack_turn.take().unwrap();
            if node.state.position().combat.turn.turn_count == turn {
                self.offer_attack_plans(std::slice::from_ref(&node), &[], stats);
                if self.prepare_tactical_plan(space, stats)? {
                    // The recipe is a hypothesis; keep the ordinary root too.
                    self.active.push_front(node);
                    return Ok(true);
                }
            }
        }
        if ((config.complete_actions || config.follow_preparations) && actions::unfinished(&node))
            || node.state.selected_members().is_some()
        {
            let priority = config.follow_preparations || node.state.selected_members().is_some();
            self.enqueue_resolution(node, stats, priority);
            self.service_pending = true;
            return self.prepare_resolution(space, config, stats);
        }
        self.expansion = Some(self.prepare_node(node, space, config, stats)?);
        Ok(true)
    }

    fn prepare_node(
        &mut self,
        node: Node,
        space: &CombatSearchSpace,
        config: &SolverConfig,
        stats: &mut SolverStats,
    ) -> Result<Expansion, String> {
        let choices = space.prepare_choices(&node.state)?;
        if let Some(audit) = &mut self.audit {
            audit.note(&node, "expanded", stats.work());
        }
        let representatives = config.collapse_equivalent_actions.then(|| {
            let actions = choices
                .branches()
                .iter()
                .filter_map(|b| match b {
                    CombatBranch::Input(input) => Some(input.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            sts_core::sim::combat_action_equivalence::canonical_combat_action_representatives_v1(
                &node.state.position().engine,
                &node.state.position().combat,
                &actions,
            )
        });
        let unordered_members = config.collapse_equivalent_actions
            && sts_core::sim::combat_action_equivalence::combat_selection_members_are_unordered_v1(
                &node.state.position().engine,
            );
        // Increasing source-domain indices visit each subset once. Keep the
        // shared legal grammar ordered; only this solver's equivalent branches
        // are reduced. A saved noncanonical prefix may still finish as recorded.
        let last_member = node.state.selected_members().and_then(|s| s.last());
        let selected_count = node.state.selected_members().map_or(0, <[_]>::len);
        let member_count = node.state.selection_members().map_or(0, <[_]>::len);
        let minimum_members = match &node.state.position().engine {
            EngineState::PendingChoice(sts_core::state::core::PendingChoice::HandSelect {
                min_cards,
                ..
            }) => usize::from(*min_cards),
            _ => 0,
        };
        let mut ordinal = 0;
        let mut branches = choices
            .branches()
            .iter()
            .enumerate()
            .filter(|(_, branch)| {
                if matches!(branch, CombatBranch::Input(_)) {
                    let keep = representatives
                        .as_ref()
                        .is_none_or(|reps| reps[ordinal] == ordinal);
                    stats.equivalent_actions_skipped += usize::from(!keep);
                    ordinal += 1;
                    keep
                } else {
                    let redundant = match branch {
                        CombatBranch::SelectMember(i) if unordered_members => {
                            last_member.is_some_and(|last| i <= last)
                                // Do not admit a prefix with too few higher
                                // members left to satisfy an exact minimum.
                                || selected_count + 1 + member_count.saturating_sub(i + 1)
                                    < minimum_members
                        }
                        _ => false,
                    };
                    stats.equivalent_actions_skipped += usize::from(redundant);
                    !redundant
                }
            })
            .filter(|(_, b)| {
                let permitted =
                    actions::permits_branch(&node, b, self.card_only, config.max_potion_uses);
                stats.resource_limit_actions_skipped += usize::from(!permitted && !self.card_only);
                permitted
            })
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        // Expose a real next-turn route before longer combinations.
        branches.sort_by_key(|&i| {
            !matches!(
                choices.branches()[i],
                CombatBranch::Input(ClientInput::EndTurn)
            )
        });
        if branches.is_empty() {
            return Err("unresolved combat has no legal continuation".into());
        }
        Ok(Expansion {
            graph_key: graph::Key::new(node.state.clone(), node.expenditures()),
            node,
            choices,
            branches,
            index: 0,
            tactical_plan: None,
            backed_revision: 0,
        })
    }
}

/// Independent potion/card-only families keep their original outer scheduling.
/// Within a family, live widths rotate rather than waiting for every narrow
/// continuation to terminate. No frontier is discarded merely for widening.
struct Beam {
    card_only: bool,
    passes: Vec<Pass>,
    current: usize,
    last_serviced_at: usize,
    done: bool,
}

impl Beam {
    fn new(root: Node, width: usize, card_only: bool) -> Self {
        Self {
            card_only,
            passes: vec![Pass::new(root, width, card_only)],
            current: 0,
            last_serviced_at: 0,
            done: false,
        }
    }

    fn next_pass(&self) -> Option<usize> {
        self.passes
            .iter()
            .enumerate()
            .filter(|(_, pass)| !pass.done)
            .min_by_key(|(_, pass)| pass.last_serviced_at)
            .map(|(i, _)| i)
    }

    fn width(&self) -> usize {
        self.passes[self.next_pass().unwrap_or(self.passes.len() - 1)].width
    }

    fn prepare(
        &mut self,
        root: &CombatPosition,
        space: &CombatSearchSpace,
        config: &SolverConfig,
        stats: &mut SolverStats,
        graph: &graph::Graph,
    ) -> Result<bool, String> {
        loop {
            let latest = self.passes.last().unwrap();
            if latest.width < config.max_width
                && (latest.primary_done
                    || latest.search_work >= latest.width * config.transitions_per_width)
            {
                let mut pass = Pass::new(
                    Node::root(CombatSearchState::new(root.clone())?)
                        .with_potion_limit(config.max_potion_uses),
                    (latest.width * 2).min(config.max_width),
                    self.card_only,
                );
                // A new width joins service now, without catching up on work
                // done before its creation or replacing an unfinished pass.
                pass.last_serviced_at = stats.search_transitions;
                // Root hypotheses are shared graph work, not repeated for
                // every wider pass. Each beam's first pass owns their recipes.
                pass.effect_root_pending = false;
                pass.audit = self.passes[0].audit.as_ref().map(audit::PassAudit::fork);
                pass.samples = self.passes[0].samples.clone();
                self.passes.push(pass);
                stats.passes_started += 1;
            }
            let Some(i) = self.next_pass() else {
                self.done = true;
                return Ok(false);
            };
            self.current = i;
            if self.passes[i].prepare(
                space,
                config,
                stats,
                Some(graph.retention_guide(self.card_only, config.max_potion_uses)),
            )? {
                return Ok(true);
            }
        }
    }
}

/// Owns only Rust search work. Chunk boundaries preserve unfinished expansion
/// and replay; a read never advances work, and the supplied root is immutable.
pub struct CombatSolver {
    root: CombatPosition,
    root_state: CombatSearchState,
    graph: graph::Graph,
    service_backup: bool,
    space: CombatSearchSpace,
    config: SolverConfig,
    beams: Vec<Beam>,
    beam_index: usize,
    replay: Option<Replay>,
    plans: Vec<SolverPlan>,
    loss_sample: Option<SolverLossSample>,
    stats: SolverStats,
    done: bool,
    shuffle_job: Option<shuffle::ShuffleJob>,
    shuffle_pending: VecDeque<SolverPlan>,
    service_shuffle: bool,
    edit_job: Option<refinement::PlanEditJob>,
    edit_pending: VecDeque<SolverPlan>,
    discard_cleanup: VecDeque<refinement::PlanEditJob>,
    service_edit: bool,
    family_job: Option<families::Job>,
    family_completed: VecDeque<families::CompletedRoot>,
    family_cursor: usize,
    service_family: bool,
}

impl CombatSolver {
    pub fn new(root: CombatPosition, config: SolverConfig) -> Result<Self, String> {
        if config.initial_width == 0
            || config.max_width < config.initial_width
            || config.max_width > 4096
            || config.transitions_per_width == 0
            || config.transitions_per_width > 65_536
            || config.max_engine_steps_per_input == 0
        {
            return Err("invalid combat solver budget".into());
        }
        let state = CombatSearchState::new(root.clone())?;
        if let Some(model) = &config.resource_model {
            model.validate(&state)?;
            if config.max_potion_uses != Some(0) {
                return Err("resource model pilot requires max_potion_uses=0".into());
            }
        }
        let space = config.max_potion_uses.map_or_else(
            CombatSearchSpace::default,
            CombatSearchSpace::with_potion_limit,
        );
        if space.terminal(&state).is_some() {
            return Err("solver root must be an unresolved combat decision".into());
        }
        let root_node = Node::root(state.clone()).with_potion_limit(config.max_potion_uses);
        let mut beams = vec![Beam::new(root_node.clone(), config.initial_width, false)];
        if config.max_potion_uses != Some(0)
            && root.combat.entities.potions.iter().any(Option::is_some)
        {
            beams.push(Beam::new(root_node, config.initial_width, true));
        }
        let passes_started = beams.len();
        Ok(Self {
            root,
            root_state: state,
            graph: graph::Graph::new(config.max_width),
            service_backup: false,
            space,
            config,
            beams,
            beam_index: 0,
            replay: None,
            plans: vec![],
            loss_sample: None,
            stats: SolverStats {
                passes_started,
                ..Default::default()
            },
            done: false,
            shuffle_job: None,
            shuffle_pending: VecDeque::new(),
            service_shuffle: true,
            edit_job: None,
            edit_pending: VecDeque::new(),
            discard_cleanup: VecDeque::new(),
            service_edit: false,
            family_job: None,
            family_completed: VecDeque::new(),
            family_cursor: 0,
            service_family: false,
        })
    }

    pub fn plans(&self) -> &[SolverPlan] {
        &self.plans
    }
    pub fn loss_sample(&self) -> Option<&SolverLossSample> {
        self.loss_sample.as_ref()
    }
    pub fn stats(&self) -> &SolverStats {
        &self.stats
    }

    /// Inspect current allocation without preparing or resuming any work.
    pub fn progress(&self) -> Vec<SolverPassProgress> {
        self.beams
            .iter()
            .flat_map(|beam| &beam.passes)
            .map(|pass| SolverPassProgress {
                width: pass.width,
                card_only: pass.card_only,
                search_work: pass.search_work,
                primary_complete: pass.primary_done,
                complete: pass.done,
                active_nodes: pass.active.len(),
                pending_nodes: pass.pending_nodes().count(),
                pending_actions: pass.pending.len() + usize::from(pass.resolving.is_some()),
                attack_plans: pass
                    .tactical_jobs()
                    .filter(|job| job.plan.is_attack())
                    .count()
                    + usize::from(
                        pass.expansion.as_ref().is_some_and(|e| {
                            e.tactical_plan.as_ref().is_some_and(|p| p.is_attack())
                        }),
                    ),
                effect_plans: pass
                    .tactical_jobs()
                    .filter(|job| !job.plan.is_attack() && !job.plan.is_response())
                    .count()
                    + usize::from(pass.expansion.as_ref().is_some_and(|e| {
                        e.tactical_plan
                            .as_ref()
                            .is_some_and(|p| !p.is_attack() && !p.is_response())
                    })),
                response_plans: pass
                    .tactical_jobs()
                    .filter(|job| job.plan.is_response())
                    .count()
                    + usize::from(pass.expansion.as_ref().is_some_and(|e| {
                        e.tactical_plan.as_ref().is_some_and(|p| p.is_response())
                    })),
                order_family_candidates: pass.order_families.pending.len()
                    + usize::from(self.family_job.as_ref().is_some_and(|j| {
                        j.candidate.origin.width == pass.width
                            && j.candidate.origin.card_only == pass.card_only
                    })),
                next_action_nodes: pass.next.len(),
                next_turn_nodes: pass.ended.len(),
                deferred_nodes: pass.deferred.len(),
                deferred_cursors: pass.deferred_cursors.len(),
                remaining_branches: pass
                    .expansion
                    .as_ref()
                    .map_or(0, |e| e.branches.len() - e.index)
                    + pass
                        .ordinary
                        .as_ref()
                        .map_or(0, |e| e.branches.len() - e.index)
                    + pass
                        .pending
                        .iter()
                        .chain(pass.resolving.iter())
                        .map(actions::PendingAction::remaining_branches)
                        .sum::<usize>()
                    + pass
                        .deferred_cursors
                        .iter()
                        .map(actions::DeferredCursor::remaining_branches)
                        .sum::<usize>(),
            })
            .collect()
    }
    pub fn root(&self) -> &CombatPosition {
        &self.root
    }
    pub fn is_complete(&self) -> bool {
        self.done
            && self.family_job.is_none()
            && self
                .beams
                .iter()
                .flat_map(|b| &b.passes)
                .all(|p| p.order_families.pending.is_empty())
            && self.shuffle_job.is_none()
            && self.shuffle_pending.is_empty()
            && self.edit_job.is_none()
            && self.edit_pending.is_empty()
            && self.discard_cleanup.is_empty()
            && self.replay.is_none()
            && !self.graph.has_pending()
    }

    pub fn shuffle_progress(&self) -> Option<ShuffleProgress> {
        self.shuffle_job
            .as_ref()
            .map(|job| job.progress(self.shuffle_pending.len()))
    }

    /// Both search and independent replay debit the work and wall-clock budget.
    /// A timed-out transition retains its input for the next call. Engine-step
    /// exhaustion is a diagnostic error, never an invented settled successor.
    pub fn advance(&mut self, work: usize, time: Duration) -> Result<SolverStop, String> {
        let stop_at = self.stats.work().saturating_add(work);
        let deadline = Instant::now()
            .checked_add(time)
            .ok_or("deadline overflow")?;
        loop {
            if self.is_complete() {
                return Ok(SolverStop::ScheduleComplete);
            }
            if self.stats.work() >= stop_at {
                return Ok(SolverStop::WorkBudget);
            }
            if Instant::now() >= deadline {
                return Ok(SolverStop::TimeBudget);
            }
            let limits = CombatStepLimits {
                max_engine_steps: self.config.max_engine_steps_per_input,
                deadline: Some(deadline),
            };
            if self.replay.is_some() {
                if !self.replay_step(limits)? {
                    return Ok(SolverStop::TimeBudget);
                }
                continue;
            }
            // Check resource-wasting discards before the broad edit portfolio.
            // These are ordinary charged engine/replay transitions, resumable
            // under the caller's original budget, also for repaired winners.
            if let Some(job) = self.discard_cleanup.front_mut() {
                match job.advance(limits, &mut self.stats)? {
                    refinement::Event::TimedOut => return Ok(SolverStop::TimeBudget),
                    refinement::Event::Done => {
                        self.discard_cleanup.pop_front();
                    }
                    refinement::Event::Progress => {}
                    refinement::Event::Win {
                        inputs,
                        expected,
                        origin,
                    } => {
                        self.replay_refinement_candidate(inputs, expected, origin);
                    }
                }
                continue;
            }
            if self.graph.has_pending() && (self.service_backup || self.done) {
                self.graph.backup_step(&mut self.stats);
                self.service_backup = false;
                continue;
            }
            if self.service_edit || self.done {
                if self.edit_job.is_none() {
                    if let Some(plan) = self.edit_pending.pop_front() {
                        self.stats.plan_edit_jobs += 1;
                        self.edit_job = Some(refinement::PlanEditJob::new(
                            self.root.clone(),
                            plan,
                            self.config.max_potion_uses,
                            self.config.refine_potion_plans,
                            self.config.refine_card_order,
                        ));
                    }
                }
                if let Some(job) = &mut self.edit_job {
                    match job.advance(limits, &mut self.stats)? {
                        refinement::Event::TimedOut => return Ok(SolverStop::TimeBudget),
                        refinement::Event::Done => self.edit_job = None,
                        refinement::Event::Progress => {}
                        refinement::Event::Win {
                            inputs,
                            expected,
                            origin,
                        } => {
                            self.replay_refinement_candidate(inputs, expected, origin);
                        }
                    }
                    self.service_edit = false;
                    continue;
                }
            }
            if self.service_shuffle || self.done {
                if self.shuffle_job.is_none() {
                    if let Some(plan) = self.shuffle_pending.pop_front() {
                        self.stats.shuffle_jobs += 1;
                        self.shuffle_job = Some(shuffle::ShuffleJob::new(
                            self.root.clone(),
                            plan,
                            self.config.clone(),
                        ));
                    }
                }
                if let Some(job) = &mut self.shuffle_job {
                    match job.advance(limits, &mut self.stats)? {
                        shuffle::ShuffleEvent::TimedOut => return Ok(SolverStop::TimeBudget),
                        shuffle::ShuffleEvent::Done => self.shuffle_job = None,
                        shuffle::ShuffleEvent::Progress => {}
                        shuffle::ShuffleEvent::Win {
                            inputs,
                            expected,
                            origin,
                        } => {
                            self.stats.shuffle_observed_wins += 1;
                            self.replay_refinement_candidate(inputs, expected, origin);
                        }
                    }
                    self.service_shuffle = false;
                    continue;
                }
            }
            if self.config.order_family_work > 0 && (self.service_family || self.done) {
                self.prepare_order_family()?;
                if let Some(job) = &mut self.family_job {
                    match job.advance(limits, &mut self.stats)? {
                        families::Event::TimedOut => return Ok(SolverStop::TimeBudget),
                        families::Event::Done => self.finish_order_family(),
                        families::Event::Progress => {}
                        families::Event::Win {
                            inputs,
                            expected,
                            origin,
                        } => {
                            self.replay_refinement_candidate(inputs, expected, origin);
                        }
                    }
                    self.service_family = false;
                    continue;
                }
            }
            self.service_shuffle = true;
            if !self.prepare_expansion()? {
                continue;
            }
            let next_beam_index = (self.beam_index + 1) % self.beams.len();
            let beam = &mut self.beams[self.beam_index];
            let pass = &mut beam.passes[beam.current];
            let e = pass.expansion.as_mut().unwrap();
            self.graph.reorder(
                e,
                pass.card_only,
                self.config.max_potion_uses,
                &mut self.stats,
            );
            let branch = e.branches[e.index];
            let input = e.choices.input(branch)?;
            match self.graph.transition(
                &e.graph_key,
                &e.choices,
                branch,
                input.as_ref(),
                limits,
                &mut self.stats,
            )? {
                Transition::Interrupted {
                    engine_steps,
                    cause,
                } => {
                    self.stats.engine_steps += engine_steps;
                    return match cause {
                        crate::TransitionInterruption::Deadline => Ok(SolverStop::TimeBudget),
                        crate::TransitionInterruption::EngineStepLimit => {
                            Err("search input exceeded engine-step limit".into())
                        }
                    };
                }
                Transition::Settled {
                    state,
                    engine_steps,
                } => {
                    if let Some(audit) = &mut pass.audit {
                        audit.transition(&e.node, input.as_ref(), self.config.max_potion_uses);
                    }
                    self.stats.search_transitions += 1;
                    self.service_backup = true;
                    self.stats.attack_plan_transitions +=
                        usize::from(e.tactical_plan.as_ref().is_some_and(|p| p.is_attack()));
                    self.stats.effect_plan_transitions += usize::from(
                        e.tactical_plan
                            .as_ref()
                            .is_some_and(|p| !p.is_attack() && !p.is_response()),
                    );
                    self.stats.response_plan_transitions +=
                        usize::from(e.tactical_plan.as_ref().is_some_and(|p| p.is_response()));
                    pass.yield_expansion = true;
                    self.stats.pending_action_work += usize::from(pass.resolving.is_some());
                    self.stats.followup_transitions += usize::from(
                        pass.resolving.is_some()
                            && !actions::unfinished(&e.node)
                            && !matches!(input, Some(ClientInput::EndTurn)),
                    );
                    beam.last_serviced_at = self.stats.search_transitions;
                    pass.last_serviced_at = self.stats.search_transitions;
                    pass.search_work += 1;
                    self.stats.recovery_transitions += usize::from(pass.primary_done);
                    self.stats.card_only_transitions += usize::from(beam.card_only);
                    self.stats.engine_steps += engine_steps;
                    pass.turn_work += 1;
                    self.beam_index = next_beam_index;
                    self.service_edit |= next_beam_index == 0;
                    self.service_family = true;
                    let e = pass.expansion.as_mut().unwrap();
                    e.index += 1;
                    let tactical_plan = e.tactical_plan.take();
                    // Exact identity, including RNG and selection assembly. No
                    // card-name, HP-only or fingerprint-only state merging.
                    if state == e.node.state {
                        self.stats.exact_duplicates += 1;
                        if let Some(plan) = tactical_plan {
                            plan.abort(&mut self.stats);
                        }
                        continue;
                    }
                    let next_turn = state.position().combat.turn.turn_count
                        > e.node.state.position().combat.turn.turn_count;
                    let followup_opportunity = self.config.follow_preparations
                        && !next_turn
                        && actions::offers_followup(&e.node.state, &state);
                    self.stats.deepest_turn = self
                        .stats
                        .deepest_turn
                        .max(state.position().combat.turn.turn_count);
                    let mut node = Node {
                        state,
                        card_baseline: e.node.card_baseline.clone(),
                        potion_budget: e.node.potion_budget.clone(),
                        trace: e.node.trace.clone(),
                        input_count: e.node.input_count,
                    };
                    if let Some(input) = input {
                        let mut expenditures = node.expenditures();
                        expenditures.record(&input);
                        node.trace = Some(Arc::new(Trace {
                            expenditures,
                            previous: node.trace,
                            input,
                        }));
                        node.input_count += 1;
                    }
                    if node.exceeds_potion_budget() {
                        self.stats.resource_limit_states_skipped += 1;
                        if let Some(audit) = &mut pass.audit {
                            audit.note(&node, "resource_limit", self.stats.work());
                        }
                        if let Some(plan) = tactical_plan {
                            plan.abort(&mut self.stats);
                        }
                        continue;
                    }
                    if next_turn
                        && node.state.position().engine == EngineState::CombatPlayerTurn
                        && self.space.terminal(&node.state).is_none()
                    {
                        let observed = project_observed_combat_outcome_with_expenditures_v1(
                            &self.root,
                            node.expenditures(),
                            node.state.position(),
                        );
                        let spent = observed.total_potion_expenditures as usize;
                        if self.config.max_potion_uses.is_none_or(|max| spent <= max) {
                            self.graph.observe_boundary(
                                &node,
                                spent,
                                self.config.resource_model.as_ref(),
                                &mut self.stats,
                            );
                        }
                    }
                    if let Some(audit) = &mut pass.audit {
                        audit.note(&node, "generated", self.stats.work());
                    }
                    let mut continuing_tactical_plan = None;
                    if let Some(plan) = tactical_plan {
                        match plan {
                            tactics::TacticalPlan::Response(plan) => {
                                if plan.finished(&node) {
                                    self.stats.response_plans_completed += 1;
                                    if let Some(audit) = &mut pass.audit {
                                        audit.note(
                                            &node,
                                            "response_plan_completed",
                                            self.stats.work(),
                                        );
                                    }
                                } else if next_turn
                                    || actions::unfinished(&node)
                                    || self.space.terminal(&node.state).is_some()
                                {
                                    self.stats.response_plans_aborted += 1;
                                } else {
                                    continuing_tactical_plan =
                                        Some(tactics::TacticalPlan::Response(plan));
                                }
                            }
                            tactics::TacticalPlan::Attack(plan) => {
                                if plan.achieved(&node) {
                                    self.stats.attack_plans_achieved += 1;
                                    if let Some(audit) = &mut pass.audit {
                                        audit.note(
                                            &node,
                                            "attack_plan_achieved",
                                            self.stats.work(),
                                        );
                                    }
                                } else if next_turn
                                    || actions::unfinished(&node)
                                    || self.space.terminal(&node.state).is_some()
                                {
                                    self.stats.attack_plans_aborted += 1;
                                } else {
                                    continuing_tactical_plan =
                                        Some(tactics::TacticalPlan::Attack(plan));
                                }
                            }
                            tactics::TacticalPlan::Effect(plan) => {
                                let reached = plan.reached_horizon(&node);
                                if reached
                                    || actions::unfinished(&node)
                                    || self.space.terminal(&node.state).is_some()
                                    || plan.target_gone(&node)
                                {
                                    if reached {
                                        self.stats.effect_plans_completed += 1;
                                    } else {
                                        self.stats.effect_plans_aborted += 1;
                                    }
                                    pass.record_effect_proposal(
                                        &tactics::TacticalPlan::Effect(plan),
                                        &node,
                                    );
                                } else {
                                    continuing_tactical_plan =
                                        Some(tactics::TacticalPlan::Effect(plan));
                                }
                            }
                        }
                    }
                    if pass.resolving.is_some() && !actions::unfinished(&node) {
                        self.stats.completed_action_alternatives += 1;
                        if let Some(audit) = &mut pass.audit {
                            audit.note(&node, "action_completed", self.stats.work());
                        }
                    }
                    match self.space.terminal(&node.state) {
                        Some(true) => {
                            self.stats.observed_wins += 1;
                            let observed = project_observed_combat_outcome_with_expenditures_v1(
                                &self.root,
                                node.expenditures(),
                                node.state.position(),
                            );
                            if self.config.max_potion_uses.is_some_and(|max| {
                                observed.total_potion_expenditures as usize > max
                            }) {
                                self.stats.resource_limit_wins_skipped += 1;
                            } else if self.plans.iter().any(|p| {
                                p.outcome.strictly_dominates(&observed)
                                    || (p.outcome.has_same_materialized_resources_v1(&observed)
                                        && p.inputs.len() <= node.input_count)
                            }) {
                                self.stats.inferior_wins_skipped += 1;
                            } else {
                                self.replay = Some(Replay {
                                    origin: SolverPlanOrigin {
                                        width: pass.width,
                                        card_only: pass.card_only,
                                        recovery: pass.primary_done,
                                        observed_at_work: self.stats.work(),
                                        pass_search_work: pass.search_work,
                                        shuffle: None,
                                        potion_edit: None,
                                        order_edit: None,
                                        order_family: None,
                                    },
                                    expected: node.state.position().clone(),
                                    inputs: node.inputs(),
                                    current: self.root_state.clone(),
                                    boundaries: vec![graph::Key::new(
                                        self.root_state.clone(),
                                        Default::default(),
                                    )],
                                    expenditures: Default::default(),
                                    index: 0,
                                    min_stable_hp: self.root.combat.entities.player.current_hp,
                                });
                            }
                        }
                        Some(false) => {
                            if node.state.position().combat.runtime.combat_smoked {
                                self.stats.observed_escapes += 1;
                                continue;
                            }
                            self.stats.observed_losses += 1;
                            if self.loss_sample.as_ref().is_none_or(|sample| {
                                node.state.position().combat.turn.turn_count
                                    > sample.successor.combat.turn.turn_count
                            }) {
                                self.loss_sample = Some(SolverLossSample {
                                    inputs: node.inputs(),
                                    successor: node.state.position().clone(),
                                });
                            }
                        }
                        None => {
                            if let Some(plan) = continuing_tactical_plan {
                                pass.tactical.push_back(tactics::TacticalJob { node, plan });
                                continue;
                            }
                            // Virtual selection assembly has not changed the
                            // game position. Compare submitted successors, not
                            // another score-pruned layer for each member. The
                            // cursor retains all optional members and charges
                            // their work before the real submission.
                            let assembling_selection = node.state.selected_members().is_some();
                            let complete_choice = assembling_selection
                                || ((self.config.complete_actions
                                    || self.config.follow_preparations)
                                    && actions::unfinished(&node));
                            let forced_end = !next_turn
                                // The explicitly requested order-family
                                // experiment compares pre-shuffle leaves.
                                // Preserve that diagnostic admission boundary.
                                && !pass.families_enabled
                                && actions::forced_end_turn(
                                    &node,
                                    &self.space,
                                    pass.card_only,
                                    self.config.max_potion_uses,
                                )?;
                            // Start-of-turn choices belong to the next turn's
                            // quota, even before its first card play.
                            let narrow_choice = !next_turn
                                && !pass.families_enabled
                                && actions::narrow_card_choice(
                                    &node,
                                    &self.space,
                                    pass.card_only,
                                    self.config.max_potion_uses,
                                )?;
                            if !next_turn
                                && (followup_opportunity
                                    || complete_choice
                                    || forced_end
                                    || narrow_choice)
                            {
                                if followup_opportunity {
                                    self.stats.preparation_opportunities += 1;
                                    if let Some(audit) = &mut pass.audit {
                                        audit.note(
                                            &node,
                                            "preparation_followup",
                                            self.stats.work(),
                                        );
                                    }
                                }
                                if forced_end {
                                    if let Some(audit) = &mut pass.audit {
                                        audit.note(
                                            &node,
                                            "forced_end_turn_queued",
                                            self.stats.work(),
                                        );
                                    }
                                }
                                if narrow_choice {
                                    self.stats.narrow_card_choices += 1;
                                    if let Some(audit) = &mut pass.audit {
                                        audit.note(&node, "narrow_card_choice", self.stats.work());
                                    }
                                }
                                pass.enqueue_resolution(
                                    node,
                                    &mut self.stats,
                                    self.config.follow_preparations
                                        || assembling_selection
                                        || forced_end
                                        || narrow_choice,
                                );
                                continue;
                            }
                            let turn = node.state.position().combat.turn.turn_count;
                            let pool = if next_turn {
                                &mut pass.ended
                            } else {
                                &mut pass.next
                            };
                            pool.push(node);
                            if pool
                                .iter()
                                .filter(|node| node.state.position().combat.turn.turn_count == turn)
                                .count()
                                >= pass.width * 8
                            {
                                let nodes = Pass::take_layer(pool, turn);
                                let (kept, omitted) = pass.partition(
                                    nodes,
                                    pass.width * 2,
                                    &mut self.stats,
                                    self.config.diversify_decision_contexts,
                                    next_turn && self.config.preserve_card_compositions,
                                    self.config.resource_model.as_ref(),
                                    if next_turn {
                                        "next_turn_capacity"
                                    } else {
                                        "next_action_capacity"
                                    },
                                    Some(self.graph.retention_guide(
                                        pass.card_only,
                                        self.config.max_potion_uses,
                                    )),
                                );
                                if next_turn {
                                    pass.ended.extend(kept);
                                } else {
                                    pass.next.extend(kept);
                                }
                                pass.defer(omitted, &mut self.stats);
                            }
                        }
                    }
                }
            }
        }
    }

    fn prepare_expansion(&mut self) -> Result<bool, String> {
        let max_wait = self.config.initial_width * self.config.transitions_per_width;
        loop {
            // All retained passes rotate by last service. A completed primary
            // pass may resume saved work while wider primary passes keep their
            // places; no global primary-completion barrier strands its cursors.
            // Prefer the family whose next service uses a narrower width.
            // Equal widths rotate; branching complexity, not a fixed work
            // split, determines how much service each family receives.
            let next = (0..self.beams.len())
                .map(|offset| (self.beam_index + offset) % self.beams.len())
                .filter(|&i| !self.beams[i].done)
                .min_by_key(|&i| {
                    let beam = &self.beams[i];
                    if !self.config.balance_beam_widths {
                        return (0, 0);
                    }
                    // A sustainable narrow pass may never finish. Service an
                    // older beam after one initial-width turn quota of waiting;
                    // widening preference must not starve legal exploration.
                    if self.stats.search_transitions - beam.last_serviced_at >= max_wait {
                        (0, beam.last_serviced_at)
                    } else {
                        (1, beam.width())
                    }
                });
            let Some(index) = next else {
                self.done = true;
                return Ok(false);
            };
            self.beam_index = index;
            let width = self.beams[index].width();
            if self.beams[self.beam_index].prepare(
                &self.root,
                &self.space,
                &self.config,
                &mut self.stats,
                &self.graph,
            )? && (!self.config.balance_beam_widths || self.beams[index].width() == width)
            {
                return Ok(true);
            }
            // prepare may have started a width or finished an exhausted pass.
            // Reconsider the requested widths before executing the next input.
        }
    }

    fn replay_refinement_candidate(
        &mut self,
        inputs: Vec<ClientInput>,
        expected: CombatPosition,
        mut origin: SolverPlanOrigin,
    ) {
        origin.observed_at_work = self.stats.work();
        let observed = project_observed_combat_outcome_with_expenditures_v1(
            &self.root,
            CombatInputExpendituresV1::from_inputs(&inputs),
            &expected,
        );
        if self
            .config
            .max_potion_uses
            .is_some_and(|max| observed.total_potion_expenditures as usize > max)
            || self.plans.iter().any(|p| {
                p.outcome.strictly_dominates(&observed)
                    || (p.outcome.has_same_materialized_resources_v1(&observed)
                        && p.inputs.len() <= inputs.len())
            })
        {
            return;
        }
        self.replay = Some(Replay {
            origin,
            expected,
            inputs,
            current: self.root_state.clone(),
            boundaries: vec![graph::Key::new(self.root_state.clone(), Default::default())],
            expenditures: Default::default(),
            index: 0,
            min_stable_hp: self.root.combat.entities.player.current_hp,
        });
    }

    fn replay_step(&mut self, limits: CombatStepLimits) -> Result<bool, String> {
        let r = self.replay.as_mut().unwrap();
        let input = r.inputs[r.index].clone();
        if !EngineCombatStepper.is_legal_action(r.current.position(), &input) {
            return Err("solver replay input is not legal".into());
        }
        let step = EngineCombatStepper.apply_to_stable(r.current.position(), input.clone(), limits);
        self.stats.engine_steps += step.engine_steps;
        if step.timed_out {
            return Ok(false);
        }
        if step.truncated {
            return Err("solver replay exceeded engine-step limit".into());
        }
        self.stats.replay_transitions += 1;
        let potion_uses =
            r.current.potion_uses() + usize::from(matches!(input, ClientInput::UsePotion { .. }));
        r.expenditures.record(&input);
        r.current = CombatSearchState::new(step.position)?;
        r.current.potion_uses = potion_uses;
        r.boundaries
            .push(graph::Key::new(r.current.clone(), r.expenditures));
        r.min_stable_hp = r
            .min_stable_hp
            .min(r.current.position().combat.entities.player.current_hp);
        r.index += 1;
        if r.index == r.inputs.len() {
            let r = self.replay.take().unwrap();
            if r.current.position() != &r.expected {
                return Err("solver replay differs from searched successor".into());
            }
            let outcome = materialize_replay_checked_combat_outcome_v1(
                &self.root,
                &r.inputs,
                r.current.position(),
            )?;
            self.stats.replayed_wins += 1;
            self.graph.verified_win(
                r.boundaries,
                outcome.clone(),
                r.expenditures,
                &mut self.stats,
            );
            if self.plans.iter().any(|p| {
                p.outcome.strictly_dominates(&outcome)
                    || (p.outcome.has_same_materialized_resources_v1(&outcome)
                        && p.inputs.len() <= r.inputs.len())
            }) {
                return Ok(true);
            }
            self.plans.retain(|p| {
                !outcome.strictly_dominates(&p.outcome)
                    && !outcome.has_same_materialized_resources_v1(&p.outcome)
            });
            let plan = SolverPlan {
                origin: r.origin,
                inputs: r.inputs,
                outcome,
                successor: r.current.position().clone(),
                found_at_work: self.stats.work(),
                min_stable_hp: r.min_stable_hp,
            };
            self.stats.shuffle_retained_plans += usize::from(plan.origin.shuffle.is_some());
            self.stats.potion_edit_retained_plans += usize::from(plan.origin.potion_edit.is_some());
            self.stats.order_edit_retained_plans += usize::from(plan.origin.order_edit.is_some());
            self.stats.order_family_retained_plans +=
                usize::from(plan.origin.order_family.is_some());
            // Only ordinary-search winners schedule broad improvement passes.
            // Discard cleanup below also covers repaired wins, proposing only
            // strictly shorter lines. Superseded pending sources are removed.
            self.shuffle_pending
                .retain(|pending| self.plans.iter().any(|p| p.inputs == pending.inputs));
            let ordinary = plan.origin.shuffle.is_none()
                && plan.origin.potion_edit.is_none()
                && plan.origin.order_edit.is_none()
                && plan.origin.order_family.is_none();
            if self.config.shuffle_continuation_work > 0 && ordinary {
                self.shuffle_pending.push_back(plan.clone());
            }
            self.edit_pending
                .retain(|pending| self.plans.iter().any(|p| p.inputs == pending.inputs));
            let edit_potions = self.config.refine_potion_plans
                && self.config.max_potion_uses != Some(0)
                && (self
                    .root
                    .combat
                    .entities
                    .potions
                    .iter()
                    .any(Option::is_some)
                    || plan.outcome.total_potion_expenditures > 0);
            if ordinary && (edit_potions || self.config.refine_card_order) {
                self.edit_pending.push_back(plan.clone());
            }
            self.discard_cleanup
                .retain(|job| self.plans.iter().any(|p| p.inputs == job.source_inputs()));
            if self.config.refine_potion_plans
                && plan
                    .inputs
                    .iter()
                    .any(|i| matches!(i, ClientInput::DiscardPotion(_)))
            {
                self.stats.plan_edit_jobs += 1;
                self.discard_cleanup
                    .push_back(refinement::PlanEditJob::discard_cleanup(
                        self.root.clone(),
                        plan.clone(),
                        self.config.max_potion_uses,
                    ));
            }
            self.plans.push(plan);
            // Presentation order only: incomparable potion/resource alternatives
            // remain available. No HP-per-potion exchange rate claims dominance.
            sort_plans_for_display(&mut self.plans);
        }
        Ok(true)
    }
}
