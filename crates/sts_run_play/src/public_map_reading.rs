//! Read-only topology of the published map. Elite counts describe ordinary
//! connections only; they do not predict room outcomes or future flight.

use std::collections::{BTreeMap, BTreeSet};
use sts_presentation::Locale;

use sts_oracle_learning::ai::planner_core::{
    public_map_preparation_paths, PlannerMapRoom, PlannerPublicMap, PlannerPublicMapNode,
    PublicMapPreparationEndpoint, PublicMapPreparationPath,
};

type Point = (i32, i32);

pub(crate) fn text(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("map", key, parameters)
        .expect("public map message")
}

fn boss_heading(map: &PlannerPublicMap, locale: Locale) -> String {
    map.boss.map_or_else(
        || text(locale, "bossHidden", &[]),
        |boss| {
            text(
                locale,
                "boss",
                &[("name", locale.entity("encounter", boss.stable_id()))],
            )
        },
    )
}

mod planning;
pub(crate) use planning::{planning_roots, route_examples, route_from};

pub(crate) struct PublicRouteReading<'a> {
    locale: Locale,
    root: Point,
    nodes: BTreeMap<Point, &'a PlannerPublicMapNode>,
    reachable: BTreeSet<Point>,
    /// Each original reachable edge occurs exactly once across these segments.
    /// Segment endpoints are repeated to preserve branch/merge identity.
    segments: Vec<Vec<Point>>,
    first_landmarks: Vec<Point>,
    preparation_paths: Vec<PublicMapPreparationPath>,
}

impl<'a> PublicRouteReading<'a> {
    pub(crate) fn new(
        map: &'a PlannerPublicMap,
        root: Point,
        locale: Locale,
    ) -> Result<Self, String> {
        let mut nodes = BTreeMap::new();
        for node in &map.nodes {
            if nodes.insert((node.x, node.y), node).is_some() {
                return Err(format!("duplicate_node@({},{})", node.x, node.y));
            }
        }
        // Iterative DFS records each reachable node once, including unpublished
        // destinations. Such endpoints remain Unknown, not inferred boss nodes.
        let mut visiting = BTreeSet::new();
        let mut reachable = BTreeSet::new();
        let mut stack = vec![(root, false)];
        while let Some((point, exiting)) = stack.pop() {
            if exiting {
                visiting.remove(&point);
                reachable.insert(point);
                continue;
            }
            if reachable.contains(&point) {
                continue;
            }
            if !visiting.insert(point) {
                return Err(format!("cycle@({},{})", point.0, point.1));
            }
            stack.push((point, true));
            if let Some(node) = nodes.get(&point) {
                let edges = destinations(node);
                if edges.iter().copied().collect::<BTreeSet<_>>().len() != edges.len() {
                    return Err(format!("duplicate_edge@({},{})", point.0, point.1));
                }
                stack.extend(edges.into_iter().rev().map(|next| (next, false)));
            }
        }
        let mut indegree = BTreeMap::<Point, usize>::new();
        for point in &reachable {
            if let Some(node) = nodes.get(point) {
                for next in destinations(node) {
                    *indegree.entry(next).or_default() += 1;
                }
            }
        }
        let is_anchor = |point: &Point| {
            *point == root
                || indegree.get(point).copied().unwrap_or(0) != 1
                || nodes.get(point).map_or(0, |node| node.edges.len()) != 1
        };
        let mut anchors: Vec<_> = reachable.iter().copied().filter(is_anchor).collect();
        anchors.sort_by_key(|point| (*point != root, point.1, point.0));
        let mut segments = Vec::new();
        for anchor in anchors {
            let next = nodes
                .get(&anchor)
                .map_or_else(Vec::new, |node| destinations(node));
            if next.is_empty() && anchor == root {
                segments.push(vec![root]);
            }
            for next in next {
                let mut segment = vec![anchor, next];
                let mut point = next;
                while !is_anchor(&point) {
                    point = destinations(nodes[&point])[0];
                    segment.push(point);
                }
                segments.push(segment);
            }
        }
        let mut first_landmarks = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![root];
        while let Some(point) = stack.pop() {
            if !seen.insert(point) {
                continue;
            }
            let Some(node) = nodes.get(&point) else {
                first_landmarks.push(point);
                continue;
            };
            if node.has_emerald_key
                || node.edges.is_empty()
                || !matches!(
                    node.room,
                    PlannerMapRoom::Monster | PlannerMapRoom::QuestionMark
                )
            {
                first_landmarks.push(point);
            } else {
                stack.extend(destinations(node).into_iter().rev());
            }
        }
        Ok(Self {
            locale,
            root,
            nodes,
            reachable,
            segments,
            first_landmarks,
            preparation_paths: public_map_preparation_paths(map, root.0, root.1)?,
        })
    }

    fn label(&self, point: Point) -> String {
        match self.nodes.get(&point) {
            None => text(
                self.locale,
                "nodeUnknown",
                &[("x", &point.0.to_string()), ("y", &point.1.to_string())],
            ),
            Some(node) => format!(
                "({},{}) {}{}{}",
                point.0,
                point.1,
                room_label(node.room, self.locale),
                if node.has_emerald_key {
                    self.locale.entry("map", "keyNode").unwrap()
                } else {
                    ""
                },
                if node.edges.is_empty() {
                    self.locale.entry("map", "endpoint").unwrap()
                } else {
                    ""
                },
            ),
        }
    }

    /// Render the shared public-map owner's connected preparation examples.
    fn preparation_windows(&self) -> String {
        let mut lines = vec![text(self.locale, "windows", &[])];
        for path in &self.preparation_paths {
            let points = &path.nodes;
            let point = *points.last().unwrap();
            let first_elite = path.endpoint == PublicMapPreparationEndpoint::Elite;
            let before = if first_elite {
                &points[..points.len() - 1]
            } else {
                &points[..]
            };
            let count = |room| {
                before
                    .iter()
                    .filter(|point| self.nodes.get(point).is_some_and(|node| node.room == room))
                    .count()
            };
            let sites = |room| {
                let sites: Vec<_> = before
                    .iter()
                    .filter(|point| self.nodes.get(point).is_some_and(|node| node.room == room))
                    .map(|point| format!("({},{})", point.0, point.1))
                    .collect();
                if sites.is_empty() {
                    text(self.locale, "none", &[])
                } else {
                    sites.join(", ")
                }
            };
            lines.push(text(
                self.locale,
                "window",
                &[
                    (
                        "scope",
                        &text(
                            self.locale,
                            if first_elite {
                                "firstElite"
                            } else {
                                "eliteFree"
                            },
                            &[],
                        ),
                    ),
                    ("node", &self.label(point)),
                    ("monsters", &count(PlannerMapRoom::Monster).to_string()),
                    (
                        "questions",
                        &count(PlannerMapRoom::QuestionMark).to_string(),
                    ),
                    ("rests", &sites(PlannerMapRoom::Rest)),
                    ("shops", &sites(PlannerMapRoom::Shop)),
                    (
                        "path",
                        &points
                            .iter()
                            .map(|point| self.label(*point))
                            .collect::<Vec<_>>()
                            .join(" -> "),
                    ),
                ],
            ));
        }
        lines.join("\n")
    }

    pub(crate) fn summary(&self) -> String {
        let [elites, shop, keys, boss] = self.summary_parts();
        text(
            self.locale,
            "summary",
            &[
                ("elites", &elites),
                ("shop", &shop),
                ("keys", &keys),
                ("boss", &boss),
                (
                    "landmarks",
                    &self
                        .first_landmarks
                        .iter()
                        .map(|point| self.label(*point))
                        .collect::<Vec<_>>()
                        .join(" / "),
                ),
                ("windows", &self.preparation_windows()),
            ],
        )
    }

    fn summary_parts(&self) -> [String; 4] {
        let range = elite_range(self.root, &self.nodes, &mut BTreeMap::new());
        let elites = if range.complete {
            if range.min_elites == range.max_elites {
                text(
                    self.locale,
                    "elites",
                    &[("count", &range.min_elites.to_string())],
                )
            } else {
                text(
                    self.locale,
                    "elites",
                    &[(
                        "count",
                        &format!("{}..{}", range.min_elites, range.max_elites),
                    )],
                )
            }
        } else {
            text(
                self.locale,
                "elitesPartial",
                &[("count", &range.min_elites.to_string())],
            )
        };
        let first_shop = self
            .reachable
            .iter()
            .filter(|point| {
                self.nodes
                    .get(point)
                    .is_some_and(|node| node.room == PlannerMapRoom::Shop)
            })
            .map(|point| point.1)
            .min();
        let shop = match first_shop {
            Some(row) => text(self.locale, "reachableRow", &[("row", &row.to_string())]),
            None if range.complete => text(self.locale, "none", &[]),
            None => text(self.locale, "unknown", &[]),
        };
        let bosses = self
            .reachable
            .iter()
            .filter(|point| {
                self.nodes
                    .get(point)
                    .is_some_and(|node| node.room == PlannerMapRoom::Boss)
            })
            .map(|point| format!("({},{})", point.0, point.1))
            .collect::<Vec<_>>();
        let boss = if bosses.is_empty() {
            if range.complete {
                text(self.locale, "nonePublished", &[])
            } else {
                text(self.locale, "unpublished", &[])
            }
        } else {
            text(self.locale, "reachable", &[("nodes", &bosses.join(" / "))])
        };
        let keys = match (range.complete, range.min_keys > 0, range.max_keys > 0) {
            (true, _, false) => "keyNone",
            (true, true, _) => "keyEvery",
            (true, false, true) => "keySome",
            (false, _, true) => "keyPartial",
            (false, _, false) => "unpublished",
        };
        [
            elites,
            text(self.locale, "shopSummary", &[("summary", &shop)]),
            text(
                self.locale,
                "keySummary",
                &[("summary", &text(self.locale, keys, &[]))],
            ),
            text(self.locale, "bossSummary", &[("summary", &boss)]),
        ]
    }

    pub(crate) fn detail(&self) -> String {
        let mut lines = vec![
            self.summary(),
            text(self.locale, "landmarks", &[]),
            text(
                self.locale,
                "connections",
                &[("count", &self.reachable.len().to_string())],
            ),
        ];
        lines.extend(self.segments.iter().map(|segment| {
            segment
                .iter()
                .map(|point| self.label(*point))
                .collect::<Vec<_>>()
                .join(" → ")
        }));
        lines.join("\n")
    }

    pub(crate) fn single_path(&self) -> Option<String> {
        let mut point = self.root;
        let mut steps = Vec::new();
        let stop = loop {
            let Some(node) = self.nodes.get(&point) else {
                break text(
                    self.locale,
                    "unknownAt",
                    &[("x", &point.0.to_string()), ("y", &point.1.to_string())],
                );
            };
            match destinations(node).as_slice() {
                [] => {
                    break text(
                        self.locale,
                        "endpointAt",
                        &[("x", &point.0.to_string()), ("y", &point.1.to_string())],
                    )
                }
                [next] => {
                    point = *next;
                    steps.push(self.label(point));
                }
                edges => {
                    break text(
                        self.locale,
                        "forkAt",
                        &[
                            ("x", &point.0.to_string()),
                            ("y", &point.1.to_string()),
                            ("count", &edges.len().to_string()),
                        ],
                    )
                }
            }
        };
        (!steps.is_empty()).then(|| {
            text(
                self.locale,
                "singlePath",
                &[("path", &steps.join(" -> ")), ("stop", &stop)],
            )
        })
    }
}

fn destinations(node: &PlannerPublicMapNode) -> Vec<Point> {
    node.edges
        .iter()
        .map(|edge| (edge.destination_x, edge.destination_y))
        .collect()
}

pub(crate) fn preparation_routes(
    map: &PlannerPublicMap,
    locale: Locale,
) -> Result<Option<String>, String> {
    let current = (map.current_x, map.current_y);
    let Some(node) = map.nodes.iter().find(|node| (node.x, node.y) == current) else {
        return Ok(None);
    };
    if node.edges.is_empty() {
        return Ok(None);
    }
    let mut lines = vec![text(
        locale,
        "afterRoom",
        &[
            ("x", &node.x.to_string()),
            ("y", &node.y.to_string()),
            ("room", room_label(node.room, locale)),
        ],
    )];
    lines.push(text(locale, "branchChoice", &[]));
    for next in destinations(node) {
        let route = PublicRouteReading::new(map, next, locale)?;
        lines.push(text(
            locale,
            "via",
            &[
                ("node", &route.label(next)),
                ("summary", &route.summary().replace('\n', "\n    ")),
            ],
        ));
    }
    Ok(Some(lines.join("\n")))
}

fn room_label(room: PlannerMapRoom, locale: Locale) -> &'static str {
    let key = match room {
        PlannerMapRoom::Monster => "monster",
        PlannerMapRoom::QuestionMark => "question",
        PlannerMapRoom::Elite => "elite",
        PlannerMapRoom::Rest => "rest",
        PlannerMapRoom::Shop => "shop",
        PlannerMapRoom::Treasure => "treasure",
        PlannerMapRoom::Boss => "boss",
        PlannerMapRoom::Victory => "victory",
    };
    locale
        .entry("map", &format!("room.{key}"))
        .expect("map room label")
}

fn room_letter(node: &PlannerPublicMapNode) -> char {
    if node.has_emerald_key && node.room == PlannerMapRoom::Elite {
        return 'K';
    }
    match node.room {
        PlannerMapRoom::Monster => 'M',
        PlannerMapRoom::QuestionMark => '?',
        PlannerMapRoom::Elite => 'E',
        PlannerMapRoom::Rest => 'R',
        PlannerMapRoom::Shop => 'S',
        PlannerMapRoom::Treasure => 'T',
        PlannerMapRoom::Boss => 'B',
        PlannerMapRoom::Victory => 'V',
    }
}

pub(crate) fn map_grid(
    map: &PlannerPublicMap,
    legal_routes: &[Point],
    locale: Locale,
    flipped: bool,
) -> String {
    let cells: BTreeMap<_, _> = map
        .nodes
        .iter()
        .map(|node| ((node.x, node.y), node))
        .collect();
    let mut lines = vec![
        text(
            locale,
            "gridHeading",
            &[
                ("x", &map.current_x.to_string()),
                ("y", &map.current_y.to_string()),
                (
                    "boss",
                    &map.boss.map_or_else(
                        || text(locale, "unknown", &[]),
                        |boss| locale.entity("encounter", boss.stable_id()).to_string(),
                    ),
                ),
            ],
        ),
        text(locale, "gridLegend", &[]),
        text(locale, "gridSymbols", &[]),
    ];
    let mut rows: Vec<_> = map.nodes.iter().map(|node| node.y).collect();
    rows.sort_unstable();
    rows.dedup();
    if !flipped {
        rows.reverse();
    }
    if flipped {
        lines.push(text(locale, "gridDownward", &[]));
    }
    for y in rows {
        let mut line = format!("y{y:02}");
        let row = map
            .nodes
            .iter()
            .filter(|node| node.y == y)
            .collect::<Vec<_>>();
        let has_boss = map
            .nodes
            .iter()
            .any(|node| node.room == PlannerMapRoom::Boss);
        if has_boss && row.iter().all(|node| node.room == PlannerMapRoom::Treasure) {
            line.push_str(&text(locale, "treasureRow", &[]));
        } else if row.iter().all(|node| node.room == PlannerMapRoom::Rest)
            && map
                .nodes
                .iter()
                .any(|node| node.room == PlannerMapRoom::Boss && node.y == y + 1)
        {
            line.push_str(&text(locale, "bossRestRow", &[]));
        }
        for (&(x, _), node) in cells.iter().filter(|(point, _)| point.1 == y) {
            let point = (x, y);
            let prefix = if point == (map.current_x, map.current_y) {
                "*"
            } else if legal_routes.contains(&point) {
                ">"
            } else {
                ""
            };
            line.push(' ');
            line.push_str(prefix);
            let mut next = destinations(node);
            next.sort_by_key(|point| (point.1, point.0));
            let exits = if next.is_empty() {
                text(locale, "gridEnd", &[])
            } else {
                next.iter()
                    .map(|p| format!("({},{})", p.0, p.1))
                    .collect::<Vec<_>>()
                    .join("/")
            };
            line.push_str(&format!("{}({x},{y})->{exits}", room_letter(node)));
        }
        lines.push(line.trim_end().to_owned());
    }
    lines.push(text(locale, "gridHelp", &[]));
    lines.join("\n") + "\n"
}

#[derive(Clone)]
struct ElitePath {
    points: Vec<Point>,
    elites: usize,
    key_elites: usize,
    unknown_end: bool,
}

#[derive(Clone, Copy)]
struct EliteRange {
    min_elites: usize,
    max_elites: usize,
    min_keys: usize,
    max_keys: usize,
    complete: bool,
}

fn elite_range(
    point: Point,
    nodes: &BTreeMap<Point, &PlannerPublicMapNode>,
    memo: &mut BTreeMap<Point, EliteRange>,
) -> EliteRange {
    if let Some(range) = memo.get(&point) {
        return *range;
    }
    let Some(node) = nodes.get(&point) else {
        return EliteRange {
            min_elites: 0,
            max_elites: 0,
            min_keys: 0,
            max_keys: 0,
            complete: false,
        };
    };
    let children: Vec<_> = destinations(node)
        .into_iter()
        .map(|next| elite_range(next, nodes, memo))
        .collect();
    let elite = usize::from(node.room == PlannerMapRoom::Elite);
    let key = usize::from(node.room == PlannerMapRoom::Elite && node.has_emerald_key);
    let range = EliteRange {
        min_elites: elite
            + children
                .iter()
                .map(|child| child.min_elites)
                .min()
                .unwrap_or(0),
        max_elites: elite
            + children
                .iter()
                .map(|child| child.max_elites)
                .max()
                .unwrap_or(0),
        min_keys: key
            + children
                .iter()
                .map(|child| child.min_keys)
                .min()
                .unwrap_or(0),
        max_keys: key
            + children
                .iter()
                .map(|child| child.max_keys)
                .max()
                .unwrap_or(0),
        complete: if children.is_empty() {
            matches!(node.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory)
        } else {
            children.iter().all(|child| child.complete)
        },
    };
    memo.insert(point, range);
    range
}

fn extreme_elite_path(
    point: Point,
    nodes: &BTreeMap<Point, &PlannerPublicMapNode>,
    memo: &mut BTreeMap<Point, ElitePath>,
    maximize: bool,
) -> ElitePath {
    if let Some(path) = memo.get(&point) {
        return path.clone();
    }
    let Some(node) = nodes.get(&point) else {
        return ElitePath {
            points: vec![point],
            elites: 0,
            key_elites: 0,
            unknown_end: true,
        };
    };
    let mut children = destinations(node)
        .into_iter()
        .map(|next| extreme_elite_path(next, nodes, memo, maximize));
    let mut best = children.next().unwrap_or(ElitePath {
        points: Vec::new(),
        elites: 0,
        key_elites: 0,
        unknown_end: false,
    });
    for candidate in children {
        let candidate_counts = (candidate.elites, candidate.key_elites);
        let best_counts = (best.elites, best.key_elites);
        let better_counts = if maximize {
            candidate_counts > best_counts
        } else {
            candidate_counts < best_counts
        };
        if better_counts || (candidate_counts == best_counts && candidate.points < best.points) {
            best = candidate;
        }
    }
    best.points.insert(0, point);
    best.elites += usize::from(node.room == PlannerMapRoom::Elite);
    best.key_elites += usize::from(node.room == PlannerMapRoom::Elite && node.has_emerald_key);
    memo.insert(point, best.clone());
    best
}

fn format_path(points: &[Point], nodes: &BTreeMap<Point, &PlannerPublicMapNode>) -> String {
    points
        .iter()
        .map(|point| {
            let letter = nodes.get(point).map_or('?', |node| room_letter(node));
            format!("{},{}{letter}", point.0, point.1)
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

fn first_elite_context(
    points: &[Point],
    nodes: &BTreeMap<Point, &PlannerPublicMapNode>,
    locale: Locale,
) -> String {
    let Some(index) = points.iter().position(|point| {
        nodes
            .get(point)
            .is_some_and(|node| node.room == PlannerMapRoom::Elite)
    }) else {
        return text(locale, "firstEliteNone", &[]);
    };
    let before = &points[..index];
    let monsters = before
        .iter()
        .filter(|point| {
            nodes
                .get(point)
                .is_some_and(|node| node.room == PlannerMapRoom::Monster)
        })
        .count();
    let questions = before
        .iter()
        .filter(|point| {
            nodes
                .get(point)
                .is_some_and(|node| node.room == PlannerMapRoom::QuestionMark)
        })
        .count();
    let rest_before = before
        .iter()
        .rev()
        .find(|point| {
            nodes
                .get(point)
                .is_some_and(|node| node.room == PlannerMapRoom::Rest)
        })
        .map_or_else(
            || text(locale, "none", &[]),
            |point| text(locale, "mapRow", &[("row", &point.1.to_string())]),
        );
    let rest_after = points[index + 1..]
        .iter()
        .find(|point| {
            nodes
                .get(point)
                .is_some_and(|node| node.room == PlannerMapRoom::Rest)
        })
        .map_or_else(
            || text(locale, "none", &[]),
            |point| text(locale, "mapRow", &[("row", &point.1.to_string())]),
        );
    text(
        locale,
        "firstEliteContext",
        &[
            ("row", &points[index].1.to_string()),
            ("monsters", &monsters.to_string()),
            ("questions", &questions.to_string()),
            ("before", &rest_before),
            ("after", &rest_after),
        ],
    )
}

/// Reachability, not a single favored path: stop each continuation at its
/// first rest, further elite, boss or endpoint. Shared suffixes are read once.
fn recovery_after_elite(
    elite: Point,
    forced: &[Point],
    nodes: &BTreeMap<Point, &PlannerPublicMapNode>,
    locale: Locale,
) -> String {
    let is_frontier = |point: &Point| {
        nodes.get(point).is_none_or(|node| {
            node.edges.is_empty()
                || matches!(
                    node.room,
                    PlannerMapRoom::Rest
                        | PlannerMapRoom::Elite
                        | PlannerMapRoom::Boss
                        | PlannerMapRoom::Victory
                )
        })
    };
    let mut seen = BTreeSet::new();
    let mut frontier = BTreeSet::new();
    let mut parents = BTreeMap::new();
    // An elite in the shared corridor precedes the selected branch. Keep
    // that selection rather than advertising recovery via its siblings.
    let mut previous = elite;
    for &point in forced {
        parents.insert(point, previous);
        previous = point;
        if is_frontier(&point) {
            break;
        }
    }
    let mut stack = if previous != elite {
        vec![previous]
    } else {
        let children = nodes
            .get(&elite)
            .map_or_else(Vec::new, |node| destinations(node));
        for &child in &children {
            parents.insert(child, elite);
        }
        children
    };
    while let Some(point) = stack.pop() {
        if !seen.insert(point) {
            continue;
        }
        if is_frontier(&point) {
            frontier.insert(point);
        } else {
            for next in destinations(nodes[&point]) {
                if let std::collections::btree_map::Entry::Vacant(entry) = parents.entry(next) {
                    entry.insert(point);
                    stack.push(next);
                }
            }
        }
    }
    let mut frontier: Vec<_> = frontier.into_iter().collect();
    frontier.sort_by_key(|point| (point.1, point.0));
    let alternatives = frontier
        .iter()
        .map(|point| {
            let room = nodes.get(point).map_or_else(
                || text(locale, "unknownEndpoint", &[]),
                |node| room_label(node.room, locale).to_string(),
            );
            let mut path = vec![*point];
            let mut cursor = *point;
            while cursor != elite {
                cursor = parents[&cursor];
                path.push(cursor);
            }
            path.reverse();
            text(
                locale,
                "recoveryNode",
                &[
                    ("room", &room),
                    ("row", &point.1.to_string()),
                    ("x", &point.0.to_string()),
                    ("y", &point.1.to_string()),
                    ("path", &format_path(&path, nodes)),
                ],
            )
        })
        .collect::<Vec<_>>()
        .join(" / ");
    text(
        locale,
        "recovery",
        &[
            ("x", &elite.0.to_string()),
            ("y", &elite.1.to_string()),
            (
                "alternatives",
                &if alternatives.is_empty() {
                    text(locale, "recordedEndpoint", &[])
                } else {
                    alternatives
                },
            ),
        ],
    )
}

/// Ranges cover every published ordinary route below each next node.
/// Maximum and minimum witnesses use the same memoized public graph traversal.
/// Neither witness nor its timing is a route recommendation or an HP forecast.
pub(crate) fn elite_routes(
    map: &PlannerPublicMap,
    roots: &[Point],
    corridor: &[Point],
    locale: Locale,
) -> Result<String, String> {
    let nodes: BTreeMap<_, _> = map
        .nodes
        .iter()
        .map(|node| ((node.x, node.y), node))
        .collect();
    let mut readings = BTreeMap::new();
    for root in roots {
        readings.insert(*root, PublicRouteReading::new(map, *root, locale)?);
    }
    let mut path_memo = BTreeMap::new();
    let mut minimum_path_memo = BTreeMap::new();
    let mut range_memo = BTreeMap::new();
    let mut branches: Vec<_> = roots
        .iter()
        .map(|root| {
            (
                elite_range(*root, &nodes, &mut range_memo),
                extreme_elite_path(*root, &nodes, &mut path_memo, true),
            )
        })
        .collect();
    branches.sort_by(|(_, a), (_, b)| {
        b.elites
            .cmp(&a.elites)
            .then_with(|| b.key_elites.cmp(&a.key_elites))
            .then_with(|| a.points.cmp(&b.points))
    });
    let mut output = format!(
        "{}\n{}\n",
        boss_heading(map, locale),
        text(locale, "eliteHeader", &[])
    );
    for (range, path) in &branches {
        let counts = if range.complete {
            let elites = if range.min_elites == range.max_elites {
                range.min_elites.to_string()
            } else {
                format!("{}..{}", range.min_elites, range.max_elites)
            };
            let keys = if range.min_keys == range.max_keys {
                range.min_keys.to_string()
            } else {
                format!("{}..{}", range.min_keys, range.max_keys)
            };
            let key_status = if range.min_keys > 0 {
                text(locale, "burningRequired", &[])
            } else if range.max_keys > 0 {
                text(locale, "burningOptional", &[])
            } else {
                String::new()
            };
            text(
                locale,
                "eliteCounts",
                &[
                    ("elites", &elites),
                    ("keys", &keys),
                    ("status", &key_status),
                ],
            )
        } else {
            text(
                locale,
                "eliteCountsPartial",
                &[
                    ("elites", &range.min_elites.to_string()),
                    ("keys", &range.min_keys.to_string()),
                ],
            )
        };
        let mut full_path = corridor.to_vec();
        full_path.extend_from_slice(&path.points);
        output.push_str(&format!(
            "  {}: {counts}\n{}\n",
            format_path(&path.points[..1], &nodes),
            text(
                locale,
                "mostElites",
                &[
                    ("context", &first_elite_context(&full_path, &nodes, locale)),
                    ("path", &format_path(&path.points, &nodes)),
                    (
                        "end",
                        &if path.unknown_end {
                            text(locale, "unpublishedEnd", &[])
                        } else {
                            String::new()
                        }
                    )
                ]
            )
        ));
        if range.min_elites != range.max_elites {
            let minimum = extreme_elite_path(path.points[0], &nodes, &mut minimum_path_memo, false);
            let mut minimum_full = corridor.to_vec();
            minimum_full.extend_from_slice(&minimum.points);
            output.push_str(&text(
                locale,
                "fewestElites",
                &[
                    ("elites", &minimum.elites.to_string()),
                    ("keys", &minimum.key_elites.to_string()),
                    (
                        "context",
                        &first_elite_context(&minimum_full, &nodes, locale),
                    ),
                    ("path", &format_path(&minimum.points, &nodes)),
                    (
                        "end",
                        &if minimum.unknown_end {
                            text(locale, "unpublishedEnd", &[])
                        } else {
                            String::new()
                        },
                    ),
                ],
            ));
            output.push('\n');
        }
        if let Some(index) = full_path.iter().position(|point| {
            nodes
                .get(point)
                .is_some_and(|node| node.room == PlannerMapRoom::Elite)
        }) {
            output.push_str(&format!(
                "    {}\n",
                recovery_after_elite(
                    full_path[index],
                    if index < corridor.len() {
                        &full_path[index + 1..=corridor.len()]
                    } else {
                        &[]
                    },
                    &nodes,
                    locale,
                )
            ));
        }
        output.push_str(&format!(
            "    {}\n",
            readings[&path.points[0]]
                .preparation_windows()
                .replace('\n', "\n    ")
        ));
    }
    if branches.is_empty() {
        output.push_str(&text(locale, "noNextNode", &[]));
    }
    Ok(output)
}

pub(crate) fn next_nodes(
    map: &PlannerPublicMap,
    legal_routes: &[Point],
    from: Option<Point>,
) -> Result<(Point, Vec<Point>, Vec<Point>), String> {
    let current = from.unwrap_or((map.current_x, map.current_y));
    if from.is_none() && !legal_routes.is_empty() {
        return Ok((current, legal_routes.to_vec(), vec![current]));
    }
    if from.is_none() && !map.nodes.iter().any(|node| (node.x, node.y) == current) {
        return Ok((current, entrance_nodes(map), vec![current]));
    }
    PublicRouteReading::new(map, current, Locale::En)?;
    let mut cursor = current;
    let mut corridor = vec![current];
    loop {
        let node = map
            .nodes
            .iter()
            .find(|node| (node.x, node.y) == cursor)
            .ok_or_else(|| format!("map has no published node at ({},{})", cursor.0, cursor.1))?;
        let next = destinations(node);
        if next.len() != 1 {
            return Ok((cursor, next, corridor));
        }
        let child = map.nodes.iter().find(|node| (node.x, node.y) == next[0]);
        if child.map_or(true, |node| node.edges.is_empty()) {
            return Ok((cursor, next, corridor));
        }
        cursor = next[0];
        corridor.push(cursor);
    }
}

fn entrance_nodes(map: &PlannerPublicMap) -> Vec<Point> {
    let Some(y) = map.nodes.iter().map(|node| node.y).min() else {
        return Vec::new();
    };
    let mut roots: Vec<_> = map
        .nodes
        .iter()
        .filter(|node| node.y == y)
        .map(|node| (node.x, node.y))
        .collect();
    roots.sort_unstable();
    roots
}

pub(crate) fn format_corridor(map: &PlannerPublicMap, corridor: &[Point]) -> String {
    corridor
        .iter()
        .map(|point| {
            let room = map
                .nodes
                .iter()
                .find(|node| (node.x, node.y) == *point)
                .map_or('?', room_letter);
            format!("{},{}{room}", point.0, point.1)
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

pub(crate) fn reachable_from_current(
    map: &PlannerPublicMap,
    legal_routes: &[Point],
    target: Point,
) -> Result<bool, String> {
    let current = (map.current_x, map.current_y);
    let mut roots = legal_routes.to_vec();
    if map.nodes.iter().any(|node| (node.x, node.y) == current) {
        roots.push(current);
    } else if roots.is_empty() {
        roots = entrance_nodes(map);
    }
    for root in roots {
        if PublicRouteReading::new(map, root, Locale::En)?
            .reachable
            .contains(&target)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests;
