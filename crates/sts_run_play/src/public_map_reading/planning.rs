//! Representative connected routes, not outcome predictions or a path ranking.
//! Suffix DP keeps elite-count extremes per first-elite preparation/key class.
//! There are at most 24 valid classes per node, irrespective of path count.

use super::*;
const REST: u8 = 1;
const SHOP: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Profile {
    has_elite: bool,
    preparation: u8,
    key: bool,
    complete: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Path {
    points: Vec<Point>,
    elites: usize,
}

#[derive(Clone, Debug)]
struct Extremes {
    min: Path,
    max: Path,
}

type Classes = BTreeMap<Profile, Extremes>;
type Nodes<'a> = BTreeMap<Point, &'a PlannerPublicMapNode>;

fn retain(classes: &mut Classes, profile: Profile, path: Path) {
    classes
        .entry(profile)
        .and_modify(|bounds| {
            if path.elites < bounds.min.elites
                || (path.elites == bounds.min.elites && path.points < bounds.min.points)
            {
                bounds.min = path.clone();
            }
            if path.elites > bounds.max.elites
                || (path.elites == bounds.max.elites && path.points < bounds.max.points)
            {
                bounds.max = path.clone();
            }
        })
        .or_insert_with(|| Extremes {
            min: path.clone(),
            max: path,
        });
}

fn suffixes(point: Point, nodes: &Nodes<'_>, memo: &mut BTreeMap<Point, Classes>) -> Classes {
    if let Some(classes) = memo.get(&point) {
        return classes.clone();
    }
    let node = nodes.get(&point).copied();
    let next = node.map_or_else(Vec::new, |node| {
        if matches!(node.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory) {
            Vec::new()
        } else {
            let mut next = destinations(node);
            next.sort_by_key(|point| (point.1, point.0));
            next
        }
    });
    let mut children = Classes::new();
    if next.is_empty() {
        retain(
            &mut children,
            Profile {
                has_elite: false,
                preparation: 0,
                key: false,
                complete: node.is_some_and(|node| {
                    matches!(node.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory)
                }),
            },
            Path {
                points: Vec::new(),
                elites: 0,
            },
        );
    } else {
        for next in next {
            for (profile, bounds) in suffixes(next, nodes, memo) {
                retain(&mut children, profile, bounds.min);
                retain(&mut children, profile, bounds.max);
            }
        }
    }
    let mut classes = Classes::new();
    for (mut profile, bounds) in children {
        let elite = node.is_some_and(|node| node.room == PlannerMapRoom::Elite);
        if elite {
            profile.has_elite = true;
            profile.preparation = 0;
            profile.key |= node.unwrap().has_emerald_key;
        } else {
            // This node precedes the suffix's first elite (or its endpoint).
            profile.preparation |= match node.map(|node| node.room) {
                Some(PlannerMapRoom::Rest) => REST,
                Some(PlannerMapRoom::Shop) => SHOP,
                _ => 0,
            };
        }
        for mut path in [bounds.min, bounds.max] {
            path.points.insert(0, point);
            path.elites += usize::from(elite);
            retain(&mut classes, profile, path);
        }
    }
    memo.insert(point, classes.clone());
    classes
}

/// Future rooms start at actual exits, never at the current room's rewards.
pub(crate) fn planning_roots(map: &PlannerPublicMap, legal_routes: &[Point]) -> Vec<Point> {
    let mut roots = if !legal_routes.is_empty() {
        legal_routes.to_vec()
    } else if let Some(node) = map
        .nodes
        .iter()
        .find(|node| (node.x, node.y) == (map.current_x, map.current_y))
    {
        destinations(node)
    } else {
        entrance_nodes(map)
    };
    roots.sort_by_key(|point| (point.1, point.0));
    roots.dedup();
    roots
}

fn compact(
    points: &[Point],
    nodes: &Nodes<'_>,
    start: usize,
    end: usize,
    anchor: Option<Point>,
) -> String {
    let mut result = String::new();
    for index in start..end {
        let point = points[index];
        let node = nodes.get(&point);
        let letter = node.map_or('U', |node| room_letter(node));
        // Every selected exit from a fork is located, including ordinary rooms.
        // Landmarks can also be used directly when comparing two route strings.
        let located = anchor == Some(point)
            || node.is_none()
            || matches!(letter, 'R' | 'S' | 'E' | 'K' | 'T')
            || (index > 0
                && nodes
                    .get(&points[index - 1])
                    .is_some_and(|n| n.edges.len() > 1));
        if located {
            if !result.is_empty() && !result.ends_with(' ') {
                result.push(' ');
            }
            result.push_str(&format!("{letter}({},{}) ", point.0, point.1));
        } else {
            result.push(letter);
        }
    }
    result.trim().to_owned()
}

/// Factor one pair only by exact shared node identity. One whole alternative
/// block cannot accidentally invent combinations at intermediate crossings.
fn paired(bounds: &Extremes, nodes: &Nodes<'_>) -> Vec<String> {
    let a = &bounds.min.points;
    let b = &bounds.max.points;
    if a == b {
        return vec![compact(a, nodes, 0, a.len(), None)];
    }
    let prefix = a.iter().zip(b).take_while(|(a, b)| a == b).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let elite_count = |points: &[Point]| {
        points
            .iter()
            .filter(|p| {
                nodes
                    .get(p)
                    .is_some_and(|node| node.room == PlannerMapRoom::Elite)
            })
            .count()
    };
    if prefix > 0
        && suffix > 0
        // One local elite bypass is easy to compare in square brackets. Different
        // multi-elite approaches deserve separate complete route lines.
        && elite_count(&a[prefix..a.len() - suffix]) <= 1
        && elite_count(&b[prefix..b.len() - suffix]) <= 1
        && nodes.get(&a[a.len() - suffix]).is_some_and(|node| {
            !matches!(node.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory)
        })
    {
        return vec![format!(
            "{} [{} | {}] {}",
            compact(a, nodes, 0, prefix, Some(a[prefix - 1])),
            compact(a, nodes, prefix, a.len() - suffix, None),
            compact(b, nodes, prefix, b.len() - suffix, None),
            compact(
                a,
                nodes,
                a.len() - suffix,
                a.len(),
                Some(a[a.len() - suffix])
            ),
        )];
    }
    // Complex alternatives and routes merging only at the boss stay separate.
    vec![
        compact(a, nodes, 0, a.len(), None),
        compact(b, nodes, 0, b.len(), None),
    ]
}

fn caption(profile: Profile, bounds: &Extremes, locale: Locale) -> String {
    let elites = if bounds.min.elites == bounds.max.elites {
        format!("{}E", bounds.min.elites)
    } else {
        format!("{}-{}E", bounds.min.elites, bounds.max.elites)
    };
    let prep = match profile.preparation {
        0 => text(locale, "noPreparation", &[]),
        REST => "R".into(),
        SHOP => "S".into(),
        _ => "R+S".into(),
    };
    text(
        locale,
        if profile.has_elite {
            "beforeElite"
        } else {
            "beforeEnd"
        },
        &[
            ("elites", &elites),
            ("preparation", &prep),
            (
                "later",
                &if profile.complete {
                    String::new()
                } else {
                    text(locale, "later", &[])
                },
            ),
        ],
    )
}

fn legend(map: &PlannerPublicMap, unpublished: bool, locale: Locale) -> String {
    let mut result = text(locale, "legend", &[]);
    if unpublished {
        result.push_str(&text(locale, "legendUnpublished", &[]));
    }
    if map
        .nodes
        .iter()
        .any(|node| node.room == PlannerMapRoom::Victory)
    {
        result.push_str(&text(locale, "legendVictory", &[]));
    }
    result
}

fn located(point: Point, nodes: &Nodes<'_>) -> String {
    let letter = nodes.get(&point).map_or('U', |node| room_letter(node));
    format!("{letter}({},{})", point.0, point.1)
}

// A shop after the first elite must not disappear just because preparation
// classes stop at that elite. This traverses all ordinary continuations.
fn shop_reachable(point: Point, nodes: &Nodes<'_>, memo: &mut BTreeMap<Point, bool>) -> bool {
    if let Some(&found) = memo.get(&point) {
        return found;
    }
    let found = nodes.get(&point).is_some_and(|node| {
        node.room == PlannerMapRoom::Shop
            || (!matches!(node.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory)
                && destinations(node)
                    .into_iter()
                    .any(|next| shop_reachable(next, nodes, memo)))
    });
    memo.insert(point, found);
    found
}

fn branch_caption(classes: &Classes, shop: bool, locale: Locale) -> String {
    let min = classes
        .values()
        .map(|bounds| bounds.min.elites)
        .min()
        .unwrap();
    let max = classes
        .values()
        .map(|bounds| bounds.max.elites)
        .max()
        .unwrap();
    let complete = classes.keys().all(|profile| profile.complete);
    let elites = if !complete {
        text(
            locale,
            if min == 1 {
                "branchElitesPartial_one"
            } else {
                "branchElitesPartial_other"
            },
            &[("count", &min.to_string())],
        )
    } else if min == max {
        text(
            locale,
            if min == 1 {
                "branchElites_one"
            } else {
                "branchElites_other"
            },
            &[("count", &min.to_string())],
        )
    } else {
        text(
            locale,
            "branchElitesRange",
            &[("min", &min.to_string()), ("max", &max.to_string())],
        )
    };
    let key = if !complete {
        if classes.keys().any(|profile| profile.key) {
            "keyReachable"
        } else {
            "keyUnknown"
        }
    } else if classes.keys().all(|profile| profile.key) {
        "keyRequired"
    } else if classes.keys().any(|profile| profile.key) {
        "keyOptional"
    } else {
        "noKey"
    };
    let shop = if shop {
        "shopReachable"
    } else if complete {
        "noShop"
    } else {
        "shopUnknown"
    };
    text(
        locale,
        "branchCaption",
        &[
            ("elites", &elites),
            ("key", &text(locale, key, &[])),
            ("shop", &text(locale, shop, &[])),
        ],
    )
}

fn preparation_caption(classes: &Classes, locale: Locale) -> String {
    let preparations: BTreeSet<_> = classes
        .keys()
        .filter(|p| p.has_elite)
        .map(|p| p.preparation)
        .collect();
    if preparations.is_empty() {
        return String::new();
    }
    let alternatives = preparations
        .into_iter()
        .map(|prep| match prep {
            0 => text(locale, "noPreparation", &[]),
            REST => "R".into(),
            SHOP => "S".into(),
            _ => "R+S".into(),
        })
        .collect::<Vec<_>>()
        .join(&text(locale, "or", &[]));
    text(
        locale,
        "beforeFirstElite",
        &[("preparation", &alternatives)],
    )
}

/// The explicit anchor is context, not a future room. Whole-route examples
/// start at its exits; later tradeoffs are derived from all suffix classes,
/// rather than inferred from whichever example happened to be retained.
pub(crate) fn route_from(
    map: &PlannerPublicMap,
    legal_routes: &[Point],
    point: Point,
    locale: Locale,
) -> Result<String, String> {
    let reading = PublicRouteReading::new(map, point, locale)?;
    let anchor = reading
        .nodes
        .get(&point)
        .ok_or_else(|| format!("map has no published node at ({},{})", point.0, point.1))?;
    let mut roots = if matches!(anchor.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory) {
        Vec::new()
    } else {
        destinations(anchor)
    };
    roots.sort_by_key(|point| (point.1, point.0));
    let mut lines = vec![
        boss_heading(map, locale),
        text(
            locale,
            "routesAfter",
            &[("node", &located(point, &reading.nodes))],
        ),
    ];
    if !reachable_from_current(map, legal_routes, point)? {
        lines.push(text(locale, "outside", &[]));
    }
    if roots.is_empty() {
        lines.push(
            if matches!(anchor.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory) {
                text(locale, "noNext", &[])
            } else {
                text(locale, "noPublishedNext", &[])
            },
        );
        return Ok(lines.join("\n") + "\n");
    }
    lines.push(legend(
        map,
        reading
            .reachable
            .iter()
            .any(|p| !reading.nodes.contains_key(p)),
        locale,
    ));
    let mut memo = BTreeMap::new();
    let mut shops = BTreeMap::new();
    for root in roots {
        let classes = suffixes(root, &reading.nodes, &mut memo);
        lines.push(format!(
            "  {}  {}",
            located(root, &reading.nodes),
            branch_caption(
                &classes,
                shop_reachable(root, &reading.nodes, &mut shops),
                locale
            )
        ));
        let mut shown = BTreeSet::new();
        for bounds in classes.values() {
            for path in paired(bounds, &reading.nodes) {
                if shown.insert(path.clone()) {
                    lines.push(format!("    {path}"));
                }
            }
        }
    }
    let mut forks: Vec<_> = memo.keys().copied().filter(|p| *p != point).collect();
    forks.sort_by_key(|p| (p.1, p.0));
    let mut tradeoffs = Vec::new();
    for fork in forks {
        let Some(node) = reading.nodes.get(&fork) else {
            continue;
        };
        if node.edges.len() < 2
            || matches!(node.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory)
        {
            continue;
        }
        let mut exits = destinations(node);
        exits.sort_by_key(|p| (p.1, p.0));
        let mut signatures = BTreeSet::new();
        let mut choices = Vec::new();
        for exit in exits {
            let classes = suffixes(exit, &reading.nodes, &mut memo);
            let shop = shop_reachable(exit, &reading.nodes, &mut shops);
            signatures.insert((
                classes
                    .iter()
                    .map(|(profile, bounds)| (*profile, bounds.min.elites, bounds.max.elites))
                    .collect::<Vec<_>>(),
                shop,
            ));
            choices.push((
                format!(
                    "{} {}",
                    located(exit, &reading.nodes),
                    branch_caption(&classes, shop, locale)
                ),
                preparation_caption(&classes, locale),
            ));
        }
        // Equivalent topology profiles need no extra prose. Coordinates and
        // all omitted connections remain available in map full.
        if signatures.len() > 1 {
            let show_preparation = choices
                .iter()
                .map(|(_, prep)| prep)
                .filter(|prep| !prep.is_empty())
                .collect::<BTreeSet<_>>()
                .len()
                > 1;
            let choices = choices
                .into_iter()
                .map(|(mut choice, prep)| {
                    if show_preparation && !prep.is_empty() {
                        choice.push_str(&format!(", {prep}"));
                    }
                    choice
                })
                .collect::<Vec<_>>();
            tradeoffs.push(format!(
                "  {} -> {}",
                located(fork, &reading.nodes),
                choices.join(" | ")
            ));
        }
    }
    if !tradeoffs.is_empty() {
        lines.push(text(locale, "laterChoices", &[]));
        lines.extend(tradeoffs);
    }
    Ok(lines.join("\n") + "\n")
}

pub(crate) fn route_examples(
    map: &PlannerPublicMap,
    roots: &[Point],
    locale: Locale,
) -> Result<String, String> {
    let mut memo = BTreeMap::new();
    let mut whole_paths = BTreeSet::new();
    // Group equal rendered examples across entrances; each entrance still has
    // its own real full-coordinate witness, never a fabricated edge/merge.
    let mut examples = BTreeMap::<(Profile, String, Vec<String>), BTreeSet<Point>>::new();
    for &root in roots {
        let reading = PublicRouteReading::new(map, root, locale)?;
        if roots.len() == 1
            && reading
                .nodes
                .get(&root)
                .is_some_and(|node| node.room == PlannerMapRoom::Boss)
        {
            let boss = map.boss.map_or_else(
                || text(locale, "bossNextHidden", &[]),
                |boss| {
                    text(
                        locale,
                        "bossNext",
                        &[("name", locale.entity("encounter", boss.stable_id()))],
                    )
                },
            );
            return Ok(format!(
                "{}\n",
                text(
                    locale,
                    "nextBoss",
                    &[
                        ("boss", &boss),
                        ("x", &root.0.to_string()),
                        ("y", &root.1.to_string())
                    ]
                )
            ));
        }
        for (profile, bounds) in suffixes(root, &reading.nodes, &mut memo) {
            whole_paths.insert(bounds.min.points.clone());
            whole_paths.insert(bounds.max.points.clone());
            examples
                .entry((
                    profile,
                    caption(profile, &bounds, locale),
                    paired(&bounds, &reading.nodes),
                ))
                .or_default()
                .insert(root);
        }
    }
    let unpublished = examples
        .keys()
        .any(|(_, _, paths)| paths.iter().any(|path| path.contains("U(")));
    let mut lines = vec![boss_heading(map, locale), legend(map, unpublished, locale)];
    for ((_, caption, paths), roots) in examples {
        lines.push(text(
            locale,
            "from",
            &[
                ("caption", &caption),
                (
                    "roots",
                    &roots
                        .iter()
                        .map(|p| format!("({},{})", p.0, p.1))
                        .collect::<Vec<_>>()
                        .join("/"),
                ),
            ],
        ));
        lines.extend(paths.into_iter().map(|path| format!("    {path}")));
    }
    if roots.is_empty() {
        lines.push(text(locale, "noNextIndented", &[]));
    }
    let ordinary = lines.join("\n") + "\n";
    let Some(&root) = roots.first() else {
        return Ok(ordinary);
    };
    let reading = PublicRouteReading::new(map, root, locale)?;
    Ok(
        shared_routes(map, roots, &reading.nodes, &memo, &whole_paths, locale)
            .filter(|factored| factored.len() < ordinary.len())
            .unwrap_or(ordinary),
    )
}

fn root_list(roots: &BTreeSet<Point>) -> String {
    roots
        .iter()
        .map(|p| format!("({},{})", p.0, p.1))
        .collect::<Vec<_>>()
        .join("/")
}

/// Factor at one exact shared room, preserving all retained full paths. Any
/// approach can use any shown continuation: both meet at the same graph node.
/// Per-entrance outlooks still describe all ordinary routes, not just examples.
fn shared_routes(
    map: &PlannerPublicMap,
    roots: &[Point],
    nodes: &Nodes<'_>,
    memo: &BTreeMap<Point, Classes>,
    paths: &BTreeSet<Vec<Point>>,
    locale: Locale,
) -> Option<String> {
    let mut outlooks = BTreeMap::<String, BTreeSet<Point>>::new();
    let mut shops = BTreeMap::new();
    for &root in roots {
        let classes = memo.get(&root)?;
        let mut outlook = branch_caption(classes, shop_reachable(root, nodes, &mut shops), locale);
        let prep = preparation_caption(classes, locale);
        if !prep.is_empty() {
            outlook.push_str(&format!(", {prep}"));
        }
        outlooks.entry(outlook).or_default().insert(root);
    }
    let candidates: BTreeSet<_> = paths
        .iter()
        .flat_map(|path| path.iter().take(path.len().saturating_sub(1)))
        .copied()
        .filter(|p| {
            nodes.get(p).is_some_and(|node| {
                !matches!(node.room, PlannerMapRoom::Boss | PlannerMapRoom::Victory)
            })
        })
        .collect();
    let mut best: Option<String> = None;
    for pivot in candidates {
        let mut approaches = BTreeMap::<String, BTreeSet<Point>>::new();
        let mut continuations = BTreeSet::new();
        let mut other = BTreeMap::<String, BTreeSet<Point>>::new();
        let mut exact_approaches = BTreeSet::new();
        for path in paths {
            if let Some(index) = path.iter().position(|p| *p == pivot) {
                if index + 1 == path.len() {
                    // A terminal occurrence has no continuation to share.
                    other
                        .entry(compact(path, nodes, 0, path.len(), None))
                        .or_default()
                        .insert(path[0]);
                    continue;
                }
                exact_approaches.insert(path[..=index].to_vec());
                approaches
                    .entry(compact(path, nodes, 0, index + 1, Some(pivot)))
                    .or_default()
                    .insert(path[0]);
                continuations.insert(compact(path, nodes, index + 1, path.len(), None));
            } else {
                other
                    .entry(compact(path, nodes, 0, path.len(), None))
                    .or_default()
                    .insert(path[0]);
            }
        }
        if exact_approaches.len() < 2 || continuations.is_empty() {
            continue;
        }
        let mut lines = vec![
            boss_heading(map, locale),
            legend(
                map,
                paths.iter().flatten().any(|p| !nodes.contains_key(p)),
                locale,
            ),
        ];
        for (outlook, roots) in &outlooks {
            lines.push(text(
                locale,
                "from",
                &[("roots", &root_list(roots)), ("caption", outlook)],
            ));
        }
        let anchor = located(pivot, nodes);
        lines.push(text(locale, "approaches", &[("node", &anchor)]));
        for (path, roots) in approaches {
            lines.push(text(
                locale,
                "from",
                &[("roots", &root_list(&roots)), ("caption", &path)],
            ));
        }
        lines.push(text(locale, "continuations", &[("node", &anchor)]));
        lines.extend(continuations.into_iter().map(|path| format!("  {path}")));
        if !other.is_empty() {
            lines.push(text(locale, "bypass", &[("node", &anchor)]));
            for (path, roots) in other {
                lines.push(text(
                    locale,
                    "from",
                    &[("roots", &root_list(&roots)), ("caption", &path)],
                ));
            }
        }
        let rendered = lines.join("\n") + "\n";
        if best
            .as_ref()
            .is_none_or(|current| rendered.len() < current.len())
        {
            best = Some(rendered);
        }
    }
    best
}

#[cfg(test)]
mod tests;
