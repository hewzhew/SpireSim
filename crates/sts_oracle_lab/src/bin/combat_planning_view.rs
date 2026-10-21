//! Read-only plan-transition and guide-lane views.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Value};
use sts_combat_planner::{LocalTurnGraphPlanTransitionEdgeSnapshot, LocalTurnGraphWitnessSession};
use sts_combat_strategy::{CombatPlanTransitionAnnotationV1, CombatPlanTransitionV1};

#[derive(Clone, Debug, Default, Serialize)]
struct CombatPlanTransitionServiceAggregateV1 {
    generated_edges: usize,
    edge_served_edges: usize,
    unserved_edges: usize,
    successor_visited_edges: usize,
    root_generated_edges: usize,
    root_edge_served_edges: usize,
    total_edge_visits: usize,
    total_anchor_visits: usize,
    total_guide_visits: usize,
    total_backed_visits: usize,
    total_successor_visits: usize,
    minimum_negative_log_policy: Option<f64>,
    minimum_action_count: Option<usize>,
    maximum_player_hp_before: Option<i32>,
    maximum_player_hp_after: Option<i32>,
    maximum_visible_damage_margin_after: Option<i32>,
    maximum_player_intangible_before: Option<i32>,
    maximum_player_intangible_after: Option<i32>,
    maximum_strength_reduction_before: Option<u16>,
    maximum_strength_reduction_after: Option<u16>,
    maximum_intangible_sources_before: Option<u16>,
    maximum_intangible_sources_after: Option<u16>,
    minimum_priority_target_hp_after: Option<i32>,
    minimum_phase_transition_damage_after: Option<i32>,
}

#[derive(Clone, Debug, Default, Serialize)]
struct CombatPlanMilestoneClosureAggregateV1 {
    retained_roots: usize,
    unvisited_roots: usize,
    ungenerated_roots: usize,
    nonnegative_damage_margin_roots: usize,
    damage_margin_at_least_negative_five_roots: usize,
    damage_margin_at_least_negative_ten_roots: usize,
    maximum_player_hp: Option<i32>,
    maximum_visible_damage_margin: Option<i32>,
    minimum_visible_damage_margin: Option<i32>,
}

impl CombatPlanTransitionServiceAggregateV1 {
    fn observe(&mut self, edge: &LocalTurnGraphPlanTransitionEdgeSnapshot) {
        let (_, transition) = plan_transition_parts(&edge.plan_transition_annotation);
        self.generated_edges = self.generated_edges.saturating_add(1);
        if edge.edge_visits > 0 {
            self.edge_served_edges = self.edge_served_edges.saturating_add(1);
        } else {
            self.unserved_edges = self.unserved_edges.saturating_add(1);
        }
        if edge.successor_visits > 0 {
            self.successor_visited_edges = self.successor_visited_edges.saturating_add(1);
        }
        if edge.parent_relative_turn_depth == 0 {
            self.root_generated_edges = self.root_generated_edges.saturating_add(1);
            if edge.edge_visits > 0 {
                self.root_edge_served_edges = self.root_edge_served_edges.saturating_add(1);
            }
        }
        self.total_edge_visits = self.total_edge_visits.saturating_add(edge.edge_visits);
        self.total_anchor_visits = self.total_anchor_visits.saturating_add(edge.anchor_visits);
        self.total_guide_visits = self.total_guide_visits.saturating_add(edge.guide_visits);
        self.total_backed_visits = self.total_backed_visits.saturating_add(edge.backed_visits);
        self.total_successor_visits = self
            .total_successor_visits
            .saturating_add(edge.successor_visits);
        self.minimum_negative_log_policy = Some(
            self.minimum_negative_log_policy
                .map_or(edge.negative_log_policy, |current| {
                    current.min(edge.negative_log_policy)
                }),
        );
        self.minimum_action_count = Some(
            self.minimum_action_count
                .map_or(edge.action_count, |current| current.min(edge.action_count)),
        );
        self.maximum_player_hp_before = Some(
            self.maximum_player_hp_before
                .map_or(transition.envelope_before.player_hp, |current| {
                    current.max(transition.envelope_before.player_hp)
                }),
        );
        self.maximum_player_intangible_before = Some(self.maximum_player_intangible_before.map_or(
            transition.envelope_before.player_intangible_turns,
            |current| current.max(transition.envelope_before.player_intangible_turns),
        ));
        self.maximum_strength_reduction_before =
            Some(self.maximum_strength_reduction_before.map_or(
                transition.resources_before.remaining_strength_reduction,
                |current| current.max(transition.resources_before.remaining_strength_reduction),
            ));
        self.maximum_intangible_sources_before =
            Some(self.maximum_intangible_sources_before.map_or(
                transition.resources_before.remaining_intangible_sources,
                |current| current.max(transition.resources_before.remaining_intangible_sources),
            ));
        if let Some(envelope) = transition.envelope_after {
            self.maximum_player_hp_after = Some(
                self.maximum_player_hp_after
                    .map_or(envelope.player_hp, |current| {
                        current.max(envelope.player_hp)
                    }),
            );
            self.maximum_visible_damage_margin_after = Some(
                self.maximum_visible_damage_margin_after
                    .map_or(envelope.visible_damage_margin, |current| {
                        current.max(envelope.visible_damage_margin)
                    }),
            );
            self.maximum_player_intangible_after = Some(
                self.maximum_player_intangible_after
                    .map_or(envelope.player_intangible_turns, |current| {
                        current.max(envelope.player_intangible_turns)
                    }),
            );
            if let Some(target_hp) = envelope.priority_target_hp_with_block {
                self.minimum_priority_target_hp_after = Some(
                    self.minimum_priority_target_hp_after
                        .map_or(target_hp, |current| current.min(target_hp)),
                );
            }
            if let Some(damage) = envelope.phase_transition_damage_remaining {
                self.minimum_phase_transition_damage_after = Some(
                    self.minimum_phase_transition_damage_after
                        .map_or(damage, |current| current.min(damage)),
                );
            }
        }
        if let Some(resources) = transition.resources_after {
            self.maximum_strength_reduction_after = Some(
                self.maximum_strength_reduction_after
                    .map_or(resources.remaining_strength_reduction, |current| {
                        current.max(resources.remaining_strength_reduction)
                    }),
            );
            self.maximum_intangible_sources_after = Some(
                self.maximum_intangible_sources_after
                    .map_or(resources.remaining_intangible_sources, |current| {
                        current.max(resources.remaining_intangible_sources)
                    }),
            );
        }
    }
}

fn serialized_plan_label<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value).expect("typed plan labels must serialize") {
        Value::String(label) => label,
        other => other.to_string(),
    }
}

fn plan_transition_parts(
    annotation: &CombatPlanTransitionAnnotationV1,
) -> (&'static str, &CombatPlanTransitionV1) {
    match annotation {
        CombatPlanTransitionAnnotationV1::AwakenedOnePhaseControl(transition) => {
            ("awakened_one_phase_control", transition)
        }
        CombatPlanTransitionAnnotationV1::BronzeAutomatonControl(transition) => {
            ("bronze_automaton_control", transition)
        }
        CombatPlanTransitionAnnotationV1::ChampPhaseControl(transition) => {
            ("champ_phase_control", transition)
        }
        CombatPlanTransitionAnnotationV1::DonuAndDecaGrowthControl(transition) => {
            ("donu_and_deca_growth_control", transition)
        }
    }
}

pub(super) fn combat_plan_transition_portfolio_v1(session: &LocalTurnGraphWitnessSession) -> Value {
    let edges = session.plan_transition_edge_snapshots();
    let mut overall = CombatPlanTransitionServiceAggregateV1::default();
    let mut plans = BTreeMap::<String, CombatPlanTransitionServiceAggregateV1>::new();
    let mut stage_transitions = BTreeMap::<String, CombatPlanTransitionServiceAggregateV1>::new();
    let mut completed_milestones =
        BTreeMap::<String, CombatPlanTransitionServiceAggregateV1>::new();
    let mut events = BTreeMap::<String, CombatPlanTransitionServiceAggregateV1>::new();

    for edge in &edges {
        overall.observe(edge);
        let (plan, transition) = plan_transition_parts(&edge.plan_transition_annotation);
        plans.entry(plan.to_string()).or_default().observe(edge);
        let before = serialized_plan_label(&transition.before_stage);
        let after = transition
            .after_stage
            .as_ref()
            .map(serialized_plan_label)
            .unwrap_or_else(|| "terminal_or_unowned".to_string());
        stage_transitions
            .entry(format!("{before}->{after}"))
            .or_default()
            .observe(edge);
        for milestone in &transition.completed_milestones {
            completed_milestones
                .entry(serialized_plan_label(milestone))
                .or_default()
                .observe(edge);
        }
        for event in &transition.events {
            let event = serde_json::to_value(event).expect("typed plan events must serialize");
            let kind = event
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("unknown_event");
            events.entry(kind.to_string()).or_default().observe(edge);
        }
    }

    let mut milestone_closure = BTreeMap::<String, CombatPlanMilestoneClosureAggregateV1>::new();
    for path in session.milestone_closure_path_snapshots() {
        let aggregate = milestone_closure
            .entry(serialized_plan_label(&path.target_milestone))
            .or_default();
        aggregate.retained_roots = aggregate.retained_roots.saturating_add(1);
        aggregate.unvisited_roots = aggregate
            .unvisited_roots
            .saturating_add(usize::from(path.visits == 0));
        aggregate.ungenerated_roots = aggregate
            .ungenerated_roots
            .saturating_add(usize::from(path.generation_work == 0));
        let margin = path.plan.envelope.visible_damage_margin;
        aggregate.nonnegative_damage_margin_roots = aggregate
            .nonnegative_damage_margin_roots
            .saturating_add(usize::from(margin >= 0));
        aggregate.damage_margin_at_least_negative_five_roots = aggregate
            .damage_margin_at_least_negative_five_roots
            .saturating_add(usize::from(margin >= -5));
        aggregate.damage_margin_at_least_negative_ten_roots = aggregate
            .damage_margin_at_least_negative_ten_roots
            .saturating_add(usize::from(margin >= -10));
        aggregate.maximum_player_hp = Some(
            aggregate
                .maximum_player_hp
                .map_or(path.plan.envelope.player_hp, |current| {
                    current.max(path.plan.envelope.player_hp)
                }),
        );
        aggregate.maximum_visible_damage_margin = Some(
            aggregate
                .maximum_visible_damage_margin
                .map_or(path.plan.envelope.visible_damage_margin, |current| {
                    current.max(path.plan.envelope.visible_damage_margin)
                }),
        );
        aggregate.minimum_visible_damage_margin = Some(
            aggregate
                .minimum_visible_damage_margin
                .map_or(path.plan.envelope.visible_damage_margin, |current| {
                    current.min(path.plan.envelope.visible_damage_margin)
                }),
        );
    }

    json!({
        "schema_name": "CombatPlanTransitionPortfolioV1",
        "schema_version": 1,
        "authority": "diagnostic_only",
        "changes_search_order": false,
        "overall": overall,
        "plans": plans,
        "stage_transitions": stage_transitions,
        "completed_milestones": completed_milestones,
        "milestone_closure_frontier": milestone_closure,
        "events": events,
    })
}
