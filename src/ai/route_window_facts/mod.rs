//! Route-window facts are a modal, value-free projection over the visible map.
//!
//! This module deliberately does not score routes and does not answer event
//! decisions. It only states what the currently reachable path family proves,
//! may prove, cannot prove, or leaves unknown.

use serde::{Deserialize, Serialize};

use crate::content::relics::RelicId;
use crate::state::map::node::{MapRoomNode, RoomType};
use crate::state::RunState;

pub const ROUTE_WINDOW_FACTS_SCHEMA_NAME: &str = "RouteWindowFacts";
pub const ROUTE_WINDOW_FACTS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWindowFactsConfig {
    /// Number of future map nodes to inspect after the current decision resolves.
    pub horizon_nodes: usize,
    /// Maximum number of path suffixes to enumerate before downgrading universal
    /// and negative claims to unknown.
    pub path_budget: usize,
}

impl Default for RouteWindowFactsConfig {
    fn default() -> Self {
        Self {
            horizon_nodes: 5,
            path_budget: 2_000,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWindowFacts {
    pub schema_name: String,
    pub schema_version: u32,
    pub cursor: RouteWindowCursor,
    pub coverage: RouteWindowCoverage,
    pub observed_path_count: usize,
    pub facts: Vec<RouteWindowFact>,
}

impl RouteWindowFacts {
    pub fn facts_for(&self, predicate: &RouteWindowPredicate) -> Vec<&RouteWindowFact> {
        self.facts
            .iter()
            .filter(|fact| &fact.predicate == predicate)
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWindowCursor {
    pub current_x: i32,
    pub current_y: i32,
    pub starts_after_current_decision: bool,
    pub start_targets: Vec<RouteWindowNode>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteWindowCoverageKind {
    CompleteWithinHorizon,
    PartialPathBudget,
    PartialUnmodeledMobility,
    UnavailableMap,
    NoVisibleContinuation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWindowCoverage {
    pub kind: RouteWindowCoverageKind,
    pub horizon_nodes: usize,
    pub path_budget: usize,
    pub path_budget_exhausted: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<RouteWindowLimitation>,
}

impl RouteWindowCoverage {
    fn can_prove_universal_and_negative(&self) -> bool {
        self.kind == RouteWindowCoverageKind::CompleteWithinHorizon
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteWindowLimitation {
    MapPlaceholder,
    PathBudgetExhausted,
    FutureWingBootsJumpsNotEnumerated,
    NoLegalFutureTargets,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWindowNode {
    pub x: i32,
    pub y: i32,
    pub room_type: Option<RoomType>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWindowTarget {
    pub x: i32,
    pub y: i32,
    pub room_type: Option<RoomType>,
    pub has_emerald_key: bool,
    pub uses_wing_boots: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteFirstEliteSegment {
    pub paths_with_first_elite: usize,
    pub forced: bool,
    pub optional: bool,
    pub min_hallway_fights_before: usize,
    pub max_hallway_fights_before: usize,
    pub min_unknowns_before: usize,
    pub max_unknowns_before: usize,
    pub min_campfires_before: usize,
    pub max_campfires_before: usize,
    pub min_shops_before: usize,
    pub max_shops_before: usize,
    pub can_bail_to_rest_before: bool,
    pub can_bail_to_shop_before: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutePathFamilySummary {
    pub path_count: usize,
    pub path_budget_exhausted: bool,
    pub min_early_pressure: usize,
    pub max_early_pressure: usize,
    pub min_elites: usize,
    pub max_elites: usize,
    pub min_shops: usize,
    pub max_shops: usize,
    pub min_campfires: usize,
    pub max_campfires: usize,
    pub min_unknowns: usize,
    pub max_unknowns: usize,
    pub min_treasures: usize,
    pub max_treasures: usize,
    pub first_shop_floor: Option<i32>,
    pub first_campfire_floor: Option<i32>,
    pub min_damage_rooms_before_recovery: usize,
    pub max_damage_rooms_before_recovery: usize,
    pub min_unknowns_before_recovery: usize,
    pub max_unknowns_before_recovery: usize,
    pub paths_with_recovery_before_damage: usize,
    pub first_elite: RouteFirstEliteSegment,
}

/// One value-free public path-family summary beginning at a currently legal
/// map entry.  This is useful before the player reaches a Map decision (for
/// example at Neow): it preserves the target identity and coverage boundary
/// without assigning a route score or selecting an entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteEntryPathSummaryV1 {
    pub target: RouteWindowTarget,
    pub coverage: RouteWindowCoverageKind,
    pub path: RoutePathFamilySummary,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWindowFact {
    pub window: RouteWindowKind,
    pub predicate: RouteWindowPredicate,
    pub modality: RouteWindowModality,
    pub scope: RouteWindowScope,
    pub horizon_nodes: usize,
    pub provenance: RouteWindowProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteWindowKind {
    Danger,
    Recovery,
    Liquidity,
    Payoff,
    Coverage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RouteWindowPredicate {
    ReachableWithin {
        subject: RouteWindowSubject,
        nodes: usize,
    },
    PresentInWindow {
        subject: RouteWindowSubject,
    },
    CountRangeInWindow {
        subject: RouteWindowSubject,
        min: usize,
        max: usize,
    },
    OccursBefore {
        subject: RouteWindowSubject,
        before: RouteWindowSubject,
    },
    CoReachableWithin {
        left: RouteWindowSubject,
        right: RouteWindowSubject,
        nodes: usize,
    },
    BypassExists {
        subject: RouteWindowSubject,
    },
    UnknownOpportunity {
        subject: RouteWindowSubject,
    },
    Coverage {
        coverage_kind: RouteWindowCoverageKind,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteWindowSubject {
    KnownCombat,
    HallwayCombat,
    Elite,
    Boss,
    Campfire,
    Shop,
    Treasure,
    UnknownRoom,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteWindowModality {
    Must,
    Can,
    Cannot,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteWindowScope {
    PathFamily,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteWindowProvenance {
    AllCoveredPaths,
    SomeCoveredPath,
    NoCoveredPathComplete,
    PartialObservation,
    MapUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWindowPath {
    pub nodes: Vec<RouteWindowNode>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteWindowPathFamily {
    pub coverage: RouteWindowCoverage,
    pub paths: Vec<RouteWindowPath>,
}

pub fn route_window_targets(run_state: &RunState) -> Vec<RouteWindowTarget> {
    let map = &run_state.map;
    if map.current_y == 14 && map.can_travel_to(0, 15, false) {
        return vec![boss_target()];
    }
    let target_y = if map.current_y == -1 {
        0
    } else {
        map.current_y + 1
    };
    if target_y == 15 && map.can_travel_to(0, 15, false) {
        return vec![boss_target()];
    }
    let wing_boots_available = wing_boots_charges(run_state) > 0;
    map.graph
        .get(target_y.max(0) as usize)
        .into_iter()
        .flat_map(|row| row.iter())
        .filter_map(|node| {
            let normal = map.can_travel_to(node.x, node.y, false);
            let wing = wing_boots_available && map.can_travel_to(node.x, node.y, true);
            if normal || wing {
                Some(RouteWindowTarget {
                    x: node.x,
                    y: node.y,
                    room_type: node.class,
                    has_emerald_key: node.has_emerald_key,
                    uses_wing_boots: !normal && wing,
                })
            } else {
                None
            }
        })
        .collect()
}

pub fn summarize_route_from_target(
    run_state: &RunState,
    x: i32,
    y: i32,
    path_budget: usize,
) -> RoutePathFamilySummary {
    if y >= 15 {
        return RoutePathFamilySummary {
            path_count: 1,
            ..RoutePathFamilySummary::default()
        };
    }
    let family = build_route_path_family_from_target(
        run_state,
        x,
        y,
        RouteWindowFactsConfig {
            horizon_nodes: 15_usize.saturating_sub(y.max(0) as usize),
            path_budget,
        },
    );
    summarize_route_path_family(&family)
}

/// Summarizes every currently legal public map entry with the same path-family
/// owner used by route decisions.  Candidate order is coordinate-stable and
/// the result contains facts only: no prior, band, dominance claim, or chosen
/// action is introduced here.
pub fn summarize_route_entry_paths_v1(
    run_state: &RunState,
    path_budget: usize,
) -> Vec<RouteEntryPathSummaryV1> {
    let mut entries = route_window_targets(run_state)
        .into_iter()
        .map(|target| {
            let horizon_nodes = 15_usize.saturating_sub(target.y.max(0) as usize);
            let family = build_route_path_family_from_target(
                run_state,
                target.x,
                target.y,
                RouteWindowFactsConfig {
                    horizon_nodes,
                    path_budget,
                },
            );
            RouteEntryPathSummaryV1 {
                target,
                coverage: family.coverage.kind,
                path: summarize_route_path_family(&family),
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| (entry.target.y, entry.target.x));
    entries
}

pub fn summarize_route_path_family(family: &RouteWindowPathFamily) -> RoutePathFamilySummary {
    let paths = family
        .paths
        .iter()
        .map(route_path_stats)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return RoutePathFamilySummary {
            path_budget_exhausted: family.coverage.path_budget_exhausted,
            ..RoutePathFamilySummary::default()
        };
    }
    let min = |read: fn(&RoutePathStats) -> usize| paths.iter().map(read).min().unwrap_or_default();
    let max = |read: fn(&RoutePathStats) -> usize| paths.iter().map(read).max().unwrap_or_default();
    RoutePathFamilySummary {
        path_count: paths.len(),
        path_budget_exhausted: family.coverage.path_budget_exhausted,
        min_early_pressure: min(|stats| stats.early_pressure),
        max_early_pressure: max(|stats| stats.early_pressure),
        min_elites: min(|stats| stats.elites),
        max_elites: max(|stats| stats.elites),
        min_shops: min(|stats| stats.shops),
        max_shops: max(|stats| stats.shops),
        min_campfires: min(|stats| stats.campfires),
        max_campfires: max(|stats| stats.campfires),
        min_unknowns: min(|stats| stats.unknowns),
        max_unknowns: max(|stats| stats.unknowns),
        min_treasures: min(|stats| stats.treasures),
        max_treasures: max(|stats| stats.treasures),
        first_shop_floor: paths
            .iter()
            .filter_map(|stats| stats.first_shop_floor)
            .min(),
        first_campfire_floor: paths
            .iter()
            .filter_map(|stats| stats.first_campfire_floor)
            .min(),
        min_damage_rooms_before_recovery: min(|stats| stats.damage_rooms_before_recovery),
        max_damage_rooms_before_recovery: max(|stats| stats.damage_rooms_before_recovery),
        min_unknowns_before_recovery: min(|stats| stats.unknowns_before_recovery),
        max_unknowns_before_recovery: max(|stats| stats.unknowns_before_recovery),
        paths_with_recovery_before_damage: paths
            .iter()
            .filter(|stats| stats.recovery_before_damage)
            .count(),
        first_elite: first_elite_segment(&paths),
    }
}

pub fn build_route_window_facts(
    run_state: &RunState,
    config: RouteWindowFactsConfig,
) -> RouteWindowFacts {
    let cursor = route_window_cursor(run_state);
    let mut facts = Vec::new();
    let starts = cursor
        .start_targets
        .iter()
        .map(|target| (target.x, target.y))
        .collect::<Vec<_>>();
    let family = build_route_path_family(run_state, &starts, config.clone());
    let paths = &family.paths;
    let coverage = &family.coverage;

    if coverage.kind == RouteWindowCoverageKind::UnavailableMap {
        facts.push(coverage_fact(coverage));
        return RouteWindowFacts {
            schema_name: ROUTE_WINDOW_FACTS_SCHEMA_NAME.to_string(),
            schema_version: ROUTE_WINDOW_FACTS_SCHEMA_VERSION,
            cursor,
            coverage: coverage.clone(),
            observed_path_count: 0,
            facts,
        };
    }

    facts.push(coverage_fact(coverage));
    facts.extend(derive_subject_facts(paths, coverage, config.horizon_nodes));
    facts.extend(derive_before_facts(paths, coverage, config.horizon_nodes));
    facts.extend(derive_coreachability_facts(
        paths,
        coverage,
        config.horizon_nodes,
    ));
    facts.extend(derive_unknown_opportunity_facts(
        paths,
        coverage,
        config.horizon_nodes,
    ));
    facts.sort_by_key(fact_sort_key);
    facts.dedup();

    RouteWindowFacts {
        schema_name: ROUTE_WINDOW_FACTS_SCHEMA_NAME.to_string(),
        schema_version: ROUTE_WINDOW_FACTS_SCHEMA_VERSION,
        cursor,
        coverage: coverage.clone(),
        observed_path_count: paths.len(),
        facts,
    }
}

pub fn build_route_path_family_from_target(
    run_state: &RunState,
    x: i32,
    y: i32,
    config: RouteWindowFactsConfig,
) -> RouteWindowPathFamily {
    build_route_path_family(run_state, &[(x, y)], config)
}

fn build_route_path_family(
    run_state: &RunState,
    starts: &[(i32, i32)],
    config: RouteWindowFactsConfig,
) -> RouteWindowPathFamily {
    if run_state.map.is_checkpoint_externalized_placeholder() || run_state.map.graph.is_empty() {
        return RouteWindowPathFamily {
            coverage: RouteWindowCoverage {
                kind: RouteWindowCoverageKind::UnavailableMap,
                horizon_nodes: config.horizon_nodes,
                path_budget: config.path_budget,
                path_budget_exhausted: false,
                limitations: vec![RouteWindowLimitation::MapPlaceholder],
            },
            paths: Vec::new(),
        };
    }

    let mut paths = Vec::new();
    let mut path_budget_exhausted = false;
    for &(x, y) in starts {
        collect_path_suffixes(
            run_state,
            x,
            y,
            Vec::new(),
            config.horizon_nodes,
            config.path_budget,
            &mut paths,
            &mut path_budget_exhausted,
        );
        if path_budget_exhausted {
            break;
        }
    }

    let has_unmodeled_future_mobility = wing_boots_charges(run_state) > 0;
    let mut limitations = Vec::new();
    if path_budget_exhausted {
        limitations.push(RouteWindowLimitation::PathBudgetExhausted);
    }
    if has_unmodeled_future_mobility {
        limitations.push(RouteWindowLimitation::FutureWingBootsJumpsNotEnumerated);
    }
    if paths.is_empty() {
        limitations.push(RouteWindowLimitation::NoLegalFutureTargets);
    }

    let kind = if path_budget_exhausted {
        RouteWindowCoverageKind::PartialPathBudget
    } else if paths.is_empty() {
        RouteWindowCoverageKind::NoVisibleContinuation
    } else if has_unmodeled_future_mobility {
        RouteWindowCoverageKind::PartialUnmodeledMobility
    } else {
        RouteWindowCoverageKind::CompleteWithinHorizon
    };
    RouteWindowPathFamily {
        coverage: RouteWindowCoverage {
            kind,
            horizon_nodes: config.horizon_nodes,
            path_budget: config.path_budget,
            path_budget_exhausted,
            limitations,
        },
        paths,
    }
}

fn boss_target() -> RouteWindowTarget {
    RouteWindowTarget {
        x: 0,
        y: 15,
        room_type: Some(RoomType::MonsterRoomBoss),
        has_emerald_key: false,
        uses_wing_boots: false,
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct RoutePathStats {
    early_pressure: usize,
    elites: usize,
    shops: usize,
    campfires: usize,
    unknowns: usize,
    treasures: usize,
    first_shop_floor: Option<i32>,
    first_campfire_floor: Option<i32>,
    first_elite_seen: bool,
    hallway_fights_before_first_elite: usize,
    unknowns_before_first_elite: usize,
    campfires_before_first_elite: usize,
    shops_before_first_elite: usize,
    first_recovery_seen: bool,
    damage_rooms_before_recovery: usize,
    unknowns_before_recovery: usize,
    recovery_before_damage: bool,
}

fn route_path_stats(path: &RouteWindowPath) -> RoutePathStats {
    path.nodes
        .iter()
        .fold(RoutePathStats::default(), update_route_path_stats)
}

fn update_route_path_stats(mut stats: RoutePathStats, node: &RouteWindowNode) -> RoutePathStats {
    match node.room_type {
        Some(RoomType::MonsterRoom) => {
            if node.y <= 3 {
                stats.early_pressure += 1;
            }
            if !stats.first_elite_seen {
                stats.hallway_fights_before_first_elite += 1;
            }
            if !stats.first_recovery_seen {
                stats.damage_rooms_before_recovery += 1;
            }
        }
        Some(RoomType::MonsterRoomElite) => {
            stats.elites += 1;
            if node.y <= 3 {
                stats.early_pressure += 1;
            }
            stats.first_elite_seen = true;
            if !stats.first_recovery_seen {
                stats.damage_rooms_before_recovery += 1;
            }
        }
        Some(RoomType::ShopRoom) => {
            stats.shops += 1;
            stats.first_shop_floor.get_or_insert(node.y + 1);
            if !stats.first_elite_seen {
                stats.shops_before_first_elite += 1;
            }
        }
        Some(RoomType::RestRoom) => {
            stats.campfires += 1;
            stats.first_campfire_floor.get_or_insert(node.y + 1);
            if !stats.first_elite_seen {
                stats.campfires_before_first_elite += 1;
            }
            observe_recovery(&mut stats);
        }
        Some(RoomType::EventRoom) => {
            stats.unknowns += 1;
            if !stats.first_elite_seen {
                stats.unknowns_before_first_elite += 1;
            }
            if !stats.first_recovery_seen {
                stats.unknowns_before_recovery += 1;
            }
        }
        Some(RoomType::TreasureRoom) => stats.treasures += 1,
        _ => {}
    }
    stats
}

fn observe_recovery(stats: &mut RoutePathStats) {
    if !stats.first_recovery_seen {
        stats.recovery_before_damage =
            stats.damage_rooms_before_recovery == 0 && stats.unknowns_before_recovery == 0;
        stats.first_recovery_seen = true;
    }
}

fn first_elite_segment(paths: &[RoutePathStats]) -> RouteFirstEliteSegment {
    let elite_paths = paths
        .iter()
        .filter(|stats| stats.first_elite_seen)
        .collect::<Vec<_>>();
    if elite_paths.is_empty() {
        return RouteFirstEliteSegment::default();
    }
    let min = |read: fn(&RoutePathStats) -> usize| {
        elite_paths
            .iter()
            .map(|stats| read(stats))
            .min()
            .unwrap_or_default()
    };
    let max = |read: fn(&RoutePathStats) -> usize| {
        elite_paths
            .iter()
            .map(|stats| read(stats))
            .max()
            .unwrap_or_default()
    };
    RouteFirstEliteSegment {
        paths_with_first_elite: elite_paths.len(),
        forced: elite_paths.len() == paths.len(),
        optional: elite_paths.len() < paths.len(),
        min_hallway_fights_before: min(|stats| stats.hallway_fights_before_first_elite),
        max_hallway_fights_before: max(|stats| stats.hallway_fights_before_first_elite),
        min_unknowns_before: min(|stats| stats.unknowns_before_first_elite),
        max_unknowns_before: max(|stats| stats.unknowns_before_first_elite),
        min_campfires_before: min(|stats| stats.campfires_before_first_elite),
        max_campfires_before: max(|stats| stats.campfires_before_first_elite),
        min_shops_before: min(|stats| stats.shops_before_first_elite),
        max_shops_before: max(|stats| stats.shops_before_first_elite),
        can_bail_to_rest_before: elite_paths
            .iter()
            .any(|stats| stats.campfires_before_first_elite > 0),
        can_bail_to_shop_before: elite_paths
            .iter()
            .any(|stats| stats.shops_before_first_elite > 0),
    }
}

fn route_window_cursor(run_state: &RunState) -> RouteWindowCursor {
    let start_targets = if run_state.map.is_checkpoint_externalized_placeholder()
        || run_state.map.graph.is_empty()
    {
        Vec::new()
    } else {
        route_window_targets(run_state)
            .into_iter()
            .map(|target| RouteWindowNode {
                x: target.x,
                y: target.y,
                room_type: target.room_type,
            })
            .collect()
    };
    RouteWindowCursor {
        current_x: run_state.map.current_x,
        current_y: run_state.map.current_y,
        starts_after_current_decision: true,
        start_targets,
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_path_suffixes(
    run_state: &RunState,
    x: i32,
    y: i32,
    mut prefix: Vec<RouteWindowNode>,
    horizon_nodes: usize,
    path_budget: usize,
    paths: &mut Vec<RouteWindowPath>,
    path_budget_exhausted: &mut bool,
) {
    if paths.len() >= path_budget {
        *path_budget_exhausted = true;
        return;
    }
    if prefix.len() >= horizon_nodes {
        paths.push(RouteWindowPath { nodes: prefix });
        return;
    }
    let Some(node) = node_at(run_state, x, y) else {
        if x == 0 && y == 15 && run_state.map.can_travel_to(0, 15, false) {
            prefix.push(RouteWindowNode {
                x,
                y,
                room_type: Some(RoomType::MonsterRoomBoss),
            });
        }
        paths.push(RouteWindowPath { nodes: prefix });
        return;
    };
    prefix.push(RouteWindowNode {
        x: node.x,
        y: node.y,
        room_type: node.class,
    });
    if prefix.len() >= horizon_nodes || node.edges.is_empty() || node.y >= 15 {
        paths.push(RouteWindowPath { nodes: prefix });
        return;
    }
    for edge in &node.edges {
        collect_path_suffixes(
            run_state,
            edge.dst_x,
            edge.dst_y,
            prefix.clone(),
            horizon_nodes,
            path_budget,
            paths,
            path_budget_exhausted,
        );
        if *path_budget_exhausted {
            return;
        }
    }
}

fn node_at(run_state: &RunState, x: i32, y: i32) -> Option<&MapRoomNode> {
    run_state
        .map
        .graph
        .get(y.max(0) as usize)
        .and_then(|row| row.get(x.max(0) as usize))
}

fn wing_boots_charges(run_state: &RunState) -> i32 {
    run_state
        .relics
        .iter()
        .find(|relic| relic.id == RelicId::WingBoots && !relic.used_up)
        .map(|relic| relic.counter.max(0))
        .unwrap_or_default()
}

fn coverage_fact(coverage: &RouteWindowCoverage) -> RouteWindowFact {
    RouteWindowFact {
        window: RouteWindowKind::Coverage,
        predicate: RouteWindowPredicate::Coverage {
            coverage_kind: coverage.kind,
        },
        modality: match coverage.kind {
            RouteWindowCoverageKind::CompleteWithinHorizon => RouteWindowModality::Must,
            RouteWindowCoverageKind::UnavailableMap
            | RouteWindowCoverageKind::NoVisibleContinuation => RouteWindowModality::Unknown,
            RouteWindowCoverageKind::PartialPathBudget
            | RouteWindowCoverageKind::PartialUnmodeledMobility => RouteWindowModality::Unknown,
        },
        scope: RouteWindowScope::PathFamily,
        horizon_nodes: coverage.horizon_nodes,
        provenance: match coverage.kind {
            RouteWindowCoverageKind::CompleteWithinHorizon => {
                RouteWindowProvenance::AllCoveredPaths
            }
            RouteWindowCoverageKind::UnavailableMap => RouteWindowProvenance::MapUnavailable,
            RouteWindowCoverageKind::PartialPathBudget
            | RouteWindowCoverageKind::PartialUnmodeledMobility
            | RouteWindowCoverageKind::NoVisibleContinuation => {
                RouteWindowProvenance::PartialObservation
            }
        },
    }
}

fn derive_subject_facts(
    paths: &[RouteWindowPath],
    coverage: &RouteWindowCoverage,
    horizon_nodes: usize,
) -> Vec<RouteWindowFact> {
    let mut facts = Vec::new();
    for subject in standard_subjects() {
        let counts = paths
            .iter()
            .map(|path| {
                path.nodes
                    .iter()
                    .filter(|node| subject_matches(*subject, node))
                    .count()
            })
            .collect::<Vec<_>>();
        let min = counts.iter().copied().min().unwrap_or(0);
        let max = counts.iter().copied().max().unwrap_or(0);
        facts.push(RouteWindowFact {
            window: window_for_subject(*subject),
            predicate: RouteWindowPredicate::CountRangeInWindow {
                subject: *subject,
                min,
                max,
            },
            modality: if paths.is_empty() {
                RouteWindowModality::Unknown
            } else if min > 0 && coverage.can_prove_universal_and_negative() {
                RouteWindowModality::Must
            } else if max > 0 {
                RouteWindowModality::Can
            } else if coverage.can_prove_universal_and_negative() {
                RouteWindowModality::Cannot
            } else {
                RouteWindowModality::Unknown
            },
            scope: RouteWindowScope::PathFamily,
            horizon_nodes,
            provenance: provenance_for_counts(min, max, coverage),
        });

        facts.push(presence_fact(
            RouteWindowPredicate::PresentInWindow { subject: *subject },
            *subject,
            min,
            max,
            coverage,
            horizon_nodes,
        ));

        for nodes in 1..=horizon_nodes {
            let within = paths
                .iter()
                .map(|path| first_subject_index(path, *subject).is_some_and(|idx| idx < nodes))
                .collect::<Vec<_>>();
            let can = within.iter().any(|value| *value);
            let must = !within.is_empty() && within.iter().all(|value| *value);
            facts.push(RouteWindowFact {
                window: window_for_subject(*subject),
                predicate: RouteWindowPredicate::ReachableWithin {
                    subject: *subject,
                    nodes,
                },
                modality: modality_from_can_must(can, must, coverage),
                scope: RouteWindowScope::PathFamily,
                horizon_nodes,
                provenance: provenance_from_can_must(can, must, coverage),
            });
        }

        let bypass_count = paths
            .iter()
            .filter(|path| first_subject_index(path, *subject).is_none())
            .count();
        if bypass_count > 0 {
            facts.push(RouteWindowFact {
                window: window_for_subject(*subject),
                predicate: RouteWindowPredicate::BypassExists { subject: *subject },
                modality: RouteWindowModality::Can,
                scope: RouteWindowScope::PathFamily,
                horizon_nodes,
                provenance: RouteWindowProvenance::SomeCoveredPath,
            });
        } else {
            facts.push(RouteWindowFact {
                window: window_for_subject(*subject),
                predicate: RouteWindowPredicate::BypassExists { subject: *subject },
                modality: if paths.is_empty() || !coverage.can_prove_universal_and_negative() {
                    RouteWindowModality::Unknown
                } else {
                    RouteWindowModality::Cannot
                },
                scope: RouteWindowScope::PathFamily,
                horizon_nodes,
                provenance: if paths.is_empty() || !coverage.can_prove_universal_and_negative() {
                    RouteWindowProvenance::PartialObservation
                } else {
                    RouteWindowProvenance::NoCoveredPathComplete
                },
            });
        }
    }
    facts
}

fn derive_before_facts(
    paths: &[RouteWindowPath],
    coverage: &RouteWindowCoverage,
    horizon_nodes: usize,
) -> Vec<RouteWindowFact> {
    let pairs = [
        (
            RouteWindowSubject::KnownCombat,
            RouteWindowSubject::Campfire,
        ),
        (RouteWindowSubject::KnownCombat, RouteWindowSubject::Shop),
        (RouteWindowSubject::Elite, RouteWindowSubject::Campfire),
        (RouteWindowSubject::Campfire, RouteWindowSubject::Elite),
        (RouteWindowSubject::Shop, RouteWindowSubject::KnownCombat),
        (RouteWindowSubject::Boss, RouteWindowSubject::Campfire),
    ];
    pairs
        .into_iter()
        .map(|(subject, before)| {
            let observations = paths
                .iter()
                .map(|path| occurs_before(path, subject, before))
                .collect::<Vec<_>>();
            let can = observations.iter().any(|value| *value);
            let must = !observations.is_empty() && observations.iter().all(|value| *value);
            RouteWindowFact {
                window: window_for_subject(subject),
                predicate: RouteWindowPredicate::OccursBefore { subject, before },
                modality: modality_from_can_must(can, must, coverage),
                scope: RouteWindowScope::PathFamily,
                horizon_nodes,
                provenance: provenance_from_can_must(can, must, coverage),
            }
        })
        .collect()
}

fn derive_coreachability_facts(
    paths: &[RouteWindowPath],
    coverage: &RouteWindowCoverage,
    horizon_nodes: usize,
) -> Vec<RouteWindowFact> {
    let pairs = [
        (RouteWindowSubject::Shop, RouteWindowSubject::Campfire),
        (RouteWindowSubject::Elite, RouteWindowSubject::Campfire),
        (RouteWindowSubject::Shop, RouteWindowSubject::Elite),
        (RouteWindowSubject::Treasure, RouteWindowSubject::Shop),
    ];
    pairs
        .into_iter()
        .map(|(left, right)| {
            let observations = paths
                .iter()
                .map(|path| {
                    first_subject_index(path, left).is_some()
                        && first_subject_index(path, right).is_some()
                })
                .collect::<Vec<_>>();
            let can = observations.iter().any(|value| *value);
            let must = !observations.is_empty() && observations.iter().all(|value| *value);
            RouteWindowFact {
                window: window_for_subject(left),
                predicate: RouteWindowPredicate::CoReachableWithin {
                    left,
                    right,
                    nodes: horizon_nodes,
                },
                modality: modality_from_can_must(can, must, coverage),
                scope: RouteWindowScope::PathFamily,
                horizon_nodes,
                provenance: provenance_from_can_must(can, must, coverage),
            }
        })
        .collect()
}

fn derive_unknown_opportunity_facts(
    paths: &[RouteWindowPath],
    coverage: &RouteWindowCoverage,
    horizon_nodes: usize,
) -> Vec<RouteWindowFact> {
    let has_unknown = paths
        .iter()
        .any(|path| first_subject_index(path, RouteWindowSubject::UnknownRoom).is_some());
    [
        RouteWindowSubject::Shop,
        RouteWindowSubject::Treasure,
        RouteWindowSubject::KnownCombat,
    ]
    .into_iter()
    .map(|subject| RouteWindowFact {
        window: window_for_subject(subject),
        predicate: RouteWindowPredicate::UnknownOpportunity { subject },
        modality: if has_unknown {
            RouteWindowModality::Unknown
        } else if coverage.can_prove_universal_and_negative() {
            RouteWindowModality::Cannot
        } else {
            RouteWindowModality::Unknown
        },
        scope: RouteWindowScope::PathFamily,
        horizon_nodes,
        provenance: if has_unknown || !coverage.can_prove_universal_and_negative() {
            RouteWindowProvenance::PartialObservation
        } else {
            RouteWindowProvenance::NoCoveredPathComplete
        },
    })
    .collect()
}

fn presence_fact(
    predicate: RouteWindowPredicate,
    subject: RouteWindowSubject,
    min: usize,
    max: usize,
    coverage: &RouteWindowCoverage,
    horizon_nodes: usize,
) -> RouteWindowFact {
    RouteWindowFact {
        window: window_for_subject(subject),
        predicate,
        modality: if min > 0 && coverage.can_prove_universal_and_negative() {
            RouteWindowModality::Must
        } else if max > 0 {
            RouteWindowModality::Can
        } else if coverage.can_prove_universal_and_negative() {
            RouteWindowModality::Cannot
        } else {
            RouteWindowModality::Unknown
        },
        scope: RouteWindowScope::PathFamily,
        horizon_nodes,
        provenance: provenance_for_counts(min, max, coverage),
    }
}

fn standard_subjects() -> &'static [RouteWindowSubject] {
    &[
        RouteWindowSubject::KnownCombat,
        RouteWindowSubject::HallwayCombat,
        RouteWindowSubject::Elite,
        RouteWindowSubject::Boss,
        RouteWindowSubject::Campfire,
        RouteWindowSubject::Shop,
        RouteWindowSubject::Treasure,
        RouteWindowSubject::UnknownRoom,
    ]
}

fn window_for_subject(subject: RouteWindowSubject) -> RouteWindowKind {
    match subject {
        RouteWindowSubject::KnownCombat
        | RouteWindowSubject::HallwayCombat
        | RouteWindowSubject::Elite
        | RouteWindowSubject::Boss => RouteWindowKind::Danger,
        RouteWindowSubject::Campfire => RouteWindowKind::Recovery,
        RouteWindowSubject::Shop => RouteWindowKind::Liquidity,
        RouteWindowSubject::Treasure | RouteWindowSubject::UnknownRoom => RouteWindowKind::Payoff,
    }
}

fn subject_matches(subject: RouteWindowSubject, node: &RouteWindowNode) -> bool {
    match subject {
        RouteWindowSubject::KnownCombat => matches!(
            node.room_type,
            Some(RoomType::MonsterRoom)
                | Some(RoomType::MonsterRoomElite)
                | Some(RoomType::MonsterRoomBoss)
        ),
        RouteWindowSubject::HallwayCombat => node.room_type == Some(RoomType::MonsterRoom),
        RouteWindowSubject::Elite => node.room_type == Some(RoomType::MonsterRoomElite),
        RouteWindowSubject::Boss => node.room_type == Some(RoomType::MonsterRoomBoss),
        RouteWindowSubject::Campfire => node.room_type == Some(RoomType::RestRoom),
        RouteWindowSubject::Shop => node.room_type == Some(RoomType::ShopRoom),
        RouteWindowSubject::Treasure => node.room_type == Some(RoomType::TreasureRoom),
        RouteWindowSubject::UnknownRoom => node.room_type == Some(RoomType::EventRoom),
    }
}

fn first_subject_index(path: &RouteWindowPath, subject: RouteWindowSubject) -> Option<usize> {
    path.nodes
        .iter()
        .position(|node| subject_matches(subject, node))
}

fn occurs_before(
    path: &RouteWindowPath,
    subject: RouteWindowSubject,
    before: RouteWindowSubject,
) -> bool {
    let Some(subject_idx) = first_subject_index(path, subject) else {
        return false;
    };
    match first_subject_index(path, before) {
        Some(before_idx) => subject_idx < before_idx,
        None => true,
    }
}

fn modality_from_can_must(
    can: bool,
    must: bool,
    coverage: &RouteWindowCoverage,
) -> RouteWindowModality {
    if must && coverage.can_prove_universal_and_negative() {
        RouteWindowModality::Must
    } else if can {
        RouteWindowModality::Can
    } else if coverage.can_prove_universal_and_negative() {
        RouteWindowModality::Cannot
    } else {
        RouteWindowModality::Unknown
    }
}

fn provenance_from_can_must(
    can: bool,
    must: bool,
    coverage: &RouteWindowCoverage,
) -> RouteWindowProvenance {
    if must && coverage.can_prove_universal_and_negative() {
        RouteWindowProvenance::AllCoveredPaths
    } else if can {
        RouteWindowProvenance::SomeCoveredPath
    } else if coverage.can_prove_universal_and_negative() {
        RouteWindowProvenance::NoCoveredPathComplete
    } else {
        RouteWindowProvenance::PartialObservation
    }
}

fn provenance_for_counts(
    min: usize,
    max: usize,
    coverage: &RouteWindowCoverage,
) -> RouteWindowProvenance {
    if min > 0 && coverage.can_prove_universal_and_negative() {
        RouteWindowProvenance::AllCoveredPaths
    } else if max > 0 {
        RouteWindowProvenance::SomeCoveredPath
    } else if coverage.can_prove_universal_and_negative() {
        RouteWindowProvenance::NoCoveredPathComplete
    } else {
        RouteWindowProvenance::PartialObservation
    }
}

fn fact_sort_key(fact: &RouteWindowFact) -> (u8, String, u8, u8, usize, u8) {
    (
        window_sort_key(fact.window),
        format!("{:?}", fact.predicate),
        modality_sort_key(fact.modality),
        scope_sort_key(fact.scope),
        fact.horizon_nodes,
        provenance_sort_key(fact.provenance),
    )
}

fn window_sort_key(window: RouteWindowKind) -> u8 {
    match window {
        RouteWindowKind::Danger => 0,
        RouteWindowKind::Recovery => 1,
        RouteWindowKind::Liquidity => 2,
        RouteWindowKind::Payoff => 3,
        RouteWindowKind::Coverage => 4,
    }
}

fn modality_sort_key(modality: RouteWindowModality) -> u8 {
    match modality {
        RouteWindowModality::Must => 0,
        RouteWindowModality::Can => 1,
        RouteWindowModality::Cannot => 2,
        RouteWindowModality::Unknown => 3,
    }
}

fn scope_sort_key(scope: RouteWindowScope) -> u8 {
    match scope {
        RouteWindowScope::PathFamily => 0,
    }
}

fn provenance_sort_key(provenance: RouteWindowProvenance) -> u8 {
    match provenance {
        RouteWindowProvenance::AllCoveredPaths => 0,
        RouteWindowProvenance::SomeCoveredPath => 1,
        RouteWindowProvenance::NoCoveredPathComplete => 2,
        RouteWindowProvenance::PartialObservation => 3,
        RouteWindowProvenance::MapUnavailable => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::map::node::{MapEdge, MapRoomNode};

    fn node(x: i32, y: i32, class: RoomType) -> MapRoomNode {
        let mut node = MapRoomNode::new(x, y);
        node.class = Some(class);
        node
    }

    fn run_with_graph(graph: Vec<Vec<MapRoomNode>>, current_x: i32, current_y: i32) -> RunState {
        let mut run_state = RunState::new(1, 0, false, "Ironclad");
        run_state.map = crate::state::map::state::MapState::new(graph);
        run_state.map.current_x = current_x;
        run_state.map.current_y = current_y;
        run_state
    }

    fn has_fact(
        facts: &RouteWindowFacts,
        predicate: RouteWindowPredicate,
        modality: RouteWindowModality,
        provenance: RouteWindowProvenance,
    ) -> bool {
        facts.facts.iter().any(|fact| {
            fact.predicate == predicate
                && fact.modality == modality
                && fact.provenance == provenance
        })
    }

    #[test]
    fn current_node_is_not_counted_as_future_window() {
        let mut event = node(0, 0, RoomType::EventRoom);
        event.edges.insert(MapEdge::new(0, 0, 0, 1));
        let fire = node(0, 1, RoomType::RestRoom);
        let run_state = run_with_graph(vec![vec![event], vec![fire]], 0, 0);

        let facts = build_route_window_facts(
            &run_state,
            RouteWindowFactsConfig {
                horizon_nodes: 2,
                path_budget: 16,
            },
        );

        assert!(has_fact(
            &facts,
            RouteWindowPredicate::PresentInWindow {
                subject: RouteWindowSubject::UnknownRoom
            },
            RouteWindowModality::Cannot,
            RouteWindowProvenance::NoCoveredPathComplete,
        ));
        assert!(has_fact(
            &facts,
            RouteWindowPredicate::PresentInWindow {
                subject: RouteWindowSubject::Campfire
            },
            RouteWindowModality::Must,
            RouteWindowProvenance::AllCoveredPaths,
        ));
    }

    #[test]
    fn can_is_not_upgraded_to_must_for_optional_shop() {
        let mut start = node(0, 0, RoomType::MonsterRoom);
        start.edges.insert(MapEdge::new(0, 0, 0, 1));
        start.edges.insert(MapEdge::new(0, 0, 1, 1));
        let shop = node(0, 1, RoomType::ShopRoom);
        let fire = node(1, 1, RoomType::RestRoom);
        let run_state = run_with_graph(vec![vec![start], vec![shop, fire]], 0, 0);

        let facts = build_route_window_facts(
            &run_state,
            RouteWindowFactsConfig {
                horizon_nodes: 1,
                path_budget: 16,
            },
        );

        assert!(has_fact(
            &facts,
            RouteWindowPredicate::ReachableWithin {
                subject: RouteWindowSubject::Shop,
                nodes: 1,
            },
            RouteWindowModality::Can,
            RouteWindowProvenance::SomeCoveredPath,
        ));
        assert!(has_fact(
            &facts,
            RouteWindowPredicate::BypassExists {
                subject: RouteWindowSubject::Shop,
            },
            RouteWindowModality::Can,
            RouteWindowProvenance::SomeCoveredPath,
        ));
    }

    #[test]
    fn partial_coverage_does_not_claim_cannot() {
        let mut start = node(0, 0, RoomType::MonsterRoom);
        start.edges.insert(MapEdge::new(0, 0, 0, 1));
        start.edges.insert(MapEdge::new(0, 0, 1, 1));
        let combat = node(0, 1, RoomType::MonsterRoom);
        let shop = node(1, 1, RoomType::ShopRoom);
        let run_state = run_with_graph(vec![vec![start], vec![combat, shop]], 0, 0);

        let facts = build_route_window_facts(
            &run_state,
            RouteWindowFactsConfig {
                horizon_nodes: 1,
                path_budget: 1,
            },
        );

        assert_eq!(
            facts.coverage.kind,
            RouteWindowCoverageKind::PartialPathBudget
        );
        assert!(!has_fact(
            &facts,
            RouteWindowPredicate::PresentInWindow {
                subject: RouteWindowSubject::Shop
            },
            RouteWindowModality::Cannot,
            RouteWindowProvenance::NoCoveredPathComplete,
        ));
    }

    #[test]
    fn candidate_path_family_preserves_ordered_visible_nodes() {
        let mut combat = node(0, 0, RoomType::MonsterRoom);
        combat.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut shop = node(0, 1, RoomType::ShopRoom);
        shop.edges.insert(MapEdge::new(0, 1, 0, 2));
        let elite = node(0, 2, RoomType::MonsterRoomElite);
        let run_state = run_with_graph(vec![vec![combat], vec![shop], vec![elite]], -1, -1);

        let family = build_route_path_family_from_target(
            &run_state,
            0,
            0,
            RouteWindowFactsConfig {
                horizon_nodes: 3,
                path_budget: 16,
            },
        );

        assert_eq!(family.paths.len(), 1);
        assert_eq!(
            family.paths[0]
                .nodes
                .iter()
                .map(|node| node.room_type)
                .collect::<Vec<_>>(),
            vec![
                Some(RoomType::MonsterRoom),
                Some(RoomType::ShopRoom),
                Some(RoomType::MonsterRoomElite),
            ]
        );
        assert_eq!(
            family.coverage.kind,
            RouteWindowCoverageKind::CompleteWithinHorizon
        );
    }

    #[test]
    fn neow_entry_summaries_reuse_the_public_path_family_owner() {
        let mut left = node(0, 0, RoomType::MonsterRoom);
        left.edges.insert(MapEdge::new(0, 0, 0, 1));
        let mut right = node(1, 0, RoomType::MonsterRoom);
        right.edges.insert(MapEdge::new(1, 0, 1, 1));
        let left_elite = node(0, 1, RoomType::MonsterRoomElite);
        let right_shop = node(1, 1, RoomType::ShopRoom);
        let run_state = run_with_graph(
            vec![vec![left, right], vec![left_elite, right_shop]],
            -1,
            -1,
        );

        let entries = summarize_route_entry_paths_v1(&run_state, 16);

        assert_eq!(entries.len(), 2);
        assert_eq!((entries[0].target.x, entries[0].target.y), (0, 0));
        assert_eq!(
            entries[0].coverage,
            RouteWindowCoverageKind::CompleteWithinHorizon
        );
        assert_eq!(entries[0].path.first_elite.paths_with_first_elite, 1);
        assert_eq!((entries[1].target.x, entries[1].target.y), (1, 0));
        assert_eq!(entries[1].path.first_shop_floor, Some(2));
        assert_eq!(entries[1].path.first_elite.paths_with_first_elite, 0);
    }

    #[test]
    fn shop_is_liquidity_not_guaranteed_recovery() {
        let family = RouteWindowPathFamily {
            coverage: RouteWindowCoverage {
                kind: RouteWindowCoverageKind::CompleteWithinHorizon,
                horizon_nodes: 2,
                path_budget: 16,
                path_budget_exhausted: false,
                limitations: Vec::new(),
            },
            paths: vec![RouteWindowPath {
                nodes: vec![
                    RouteWindowNode {
                        x: 0,
                        y: 0,
                        room_type: Some(RoomType::ShopRoom),
                    },
                    RouteWindowNode {
                        x: 0,
                        y: 1,
                        room_type: Some(RoomType::MonsterRoomElite),
                    },
                ],
            }],
        };

        let summary = summarize_route_path_family(&family);

        assert_eq!(summary.min_shops, 1);
        assert_eq!(summary.paths_with_recovery_before_damage, 0);
        assert_eq!(summary.min_damage_rooms_before_recovery, 1);
    }

    #[test]
    fn candidate_path_family_reports_partial_path_budget() {
        let mut start = node(0, 0, RoomType::MonsterRoom);
        start.edges.insert(MapEdge::new(0, 0, 0, 1));
        start.edges.insert(MapEdge::new(0, 0, 1, 1));
        let combat = node(0, 1, RoomType::MonsterRoom);
        let shop = node(1, 1, RoomType::ShopRoom);
        let run_state = run_with_graph(vec![vec![start], vec![combat, shop]], -1, -1);

        let family = build_route_path_family_from_target(
            &run_state,
            0,
            0,
            RouteWindowFactsConfig {
                horizon_nodes: 2,
                path_budget: 1,
            },
        );

        assert_eq!(family.paths.len(), 1);
        assert_eq!(
            family.coverage.kind,
            RouteWindowCoverageKind::PartialPathBudget
        );
    }
}
