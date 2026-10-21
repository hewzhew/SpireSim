use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use sts_paired_root::WorldProvenanceV1;

pub const MANIFEST_VERSION_V1: &str = "STS-MANIFEST 1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestAuthorityV1 {
    ObserveOnly,
    LabHead,
}

impl ManifestAuthorityV1 {
    pub fn token(self) -> &'static str {
        match self {
            Self::ObserveOnly => "observe_only",
            Self::LabHead => "lab_head",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionLeaseEntryV1 {
    pub alias: String,
    pub action_id: Option<String>,
    pub available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticLeaseEntryV1 {
    pub alias: String,
    pub diagnostic_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedManifestV1 {
    pub text: String,
    pub actions: Vec<ActionLeaseEntryV1>,
    pub diagnostics: Vec<DiagnosticLeaseEntryV1>,
}

pub(super) fn manifest_scalar_v1(value: &str) -> Result<String, String> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err("manifest scalar is invalid".to_owned());
    }
    let is_safe_atom = value.bytes().enumerate().all(|(index, byte)| {
        (byte.is_ascii_alphabetic() || byte == b'_') && index == 0
            || index > 0
                && (byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':' | b'+'))
    }) && !matches!(value, "none" | "empty" | "true" | "false");
    if is_safe_atom {
        Ok(value.to_owned())
    } else {
        serde_json::to_string(value)
            .map_err(|error| format!("failed to quote a manifest scalar: {error}"))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum MapRoomTagV1 {
    Monster,
    Question,
    Elite,
    Rest,
    Shop,
    Treasure,
    Boss,
    Victory,
}

impl MapRoomTagV1 {
    fn token(self, burning: bool) -> Result<&'static str, String> {
        match (self, burning) {
            (Self::Elite, true) => Ok("elite+burning"),
            (Self::Elite, false) => Ok("elite"),
            (Self::Monster, false) => Ok("monster"),
            (Self::Question, false) => Ok("question"),
            (Self::Rest, false) => Ok("rest"),
            (Self::Shop, false) => Ok("shop"),
            (Self::Treasure, false) => Ok("treasure"),
            (Self::Boss, false) => Ok("boss"),
            (Self::Victory, false) => Ok("victory"),
            (_, true) => Err("only an elite map node may carry the burning marker".to_owned()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MapCoordinateV1 {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicMapNodeV1 {
    pub coordinate: MapCoordinateV1,
    pub room: MapRoomTagV1,
    pub burning: bool,
    pub edges: Vec<MapCoordinateV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicMapV1 {
    pub current: Option<MapCoordinateV1>,
    pub boss_id: Option<String>,
    pub boss_anchor: Option<MapCoordinateV1>,
    /// Only real topology members belong here. Grid placeholders must be removed
    /// by the exact projection owner before this renderer is called.
    pub nodes: Vec<PublicMapNodeV1>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrailPointV1 {
    Start,
    Node(MapCoordinateV1),
    Boss,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExactTrailV1 {
    Available(Vec<TrailPointV1>),
    CoverageGap { gap_id: String, reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionAvailabilityV1 {
    Verified,
    Unknown { gap_id: String },
    Unsupported { reason: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapActionRouteV1 {
    GraphEdge,
    WingBoots { cost: u8 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapActionV1 {
    /// Stable internal identity. It is returned only in the actor-owned lease,
    /// never written to the model-facing manifest.
    pub action_id: String,
    pub target: MapCoordinateV1,
    pub route: MapActionRouteV1,
    pub availability: ActionAvailabilityV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactParityV1 {
    pub exact_domains: usize,
    pub total_domains: usize,
    pub coverage_gap_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirstDifferenceV1 {
    pub diagnostic_id: String,
    pub domain: String,
    pub first_difference: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParityStateV1 {
    Exact(ExactParityV1),
    Blocked(FirstDifferenceV1),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PublicRunV1 {
    pub seed: String,
    pub ascension: u8,
    pub character: String,
    pub act: u8,
    pub floor: i32,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub final_act_available: bool,
    pub keys: [bool; 3],
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PublicMasterDeckCardV1 {
    pub ordinal: usize,
    pub card_id: String,
    pub upgrades: u8,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PublicRelicV1 {
    pub ordinal: usize,
    pub id: String,
    pub counter: i64,
    #[serde(default)]
    pub used_up: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PublicPersistentPotionSlotV1 {
    pub slot: usize,
    pub id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PublicCombatPotionV1 {
    pub slot: usize,
    pub id: String,
    pub can_use: bool,
    pub can_discard: bool,
    pub target_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublicPotionBeltV1 {
    Persistent(Vec<PublicPersistentPotionSlotV1>),
    Combat(Vec<PublicCombatPotionV1>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedExactSceneV1 {
    pub world: WorldProvenanceV1,
    pub authority: ManifestAuthorityV1,
    pub parity: ParityStateV1,
    pub run: PublicRunV1,
    pub master_deck: Vec<PublicMasterDeckCardV1>,
    pub relics: Vec<PublicRelicV1>,
    pub potions: PublicPotionBeltV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParityLabMapInputV1 {
    pub shared: SharedExactSceneV1,
    pub map: PublicMapV1,
    pub trail: ExactTrailV1,
    pub actions: Vec<MapActionV1>,
}

pub(super) enum ManifestPreludeV1 {
    Exact(String),
    Blocked(RenderedManifestV1),
}

pub fn render_parity_lab_map_v1(input: &ParityLabMapInputV1) -> Result<RenderedManifestV1, String> {
    if let Some(boss) = &input.map.boss_id {
        require_identity(boss, "boss identity")?;
    }
    let mut additional_gaps = Vec::new();
    if let ExactTrailV1::CoverageGap { gap_id, reason } = &input.trail {
        require_token(gap_id, "trail gap identity")?;
        require_token(reason, "trail gap reason")?;
        additional_gaps.push(gap_id.clone());
    }
    let mut text = match render_manifest_prelude_v1(&input.shared, "map", &additional_gaps)? {
        ManifestPreludeV1::Exact(text) => text,
        ManifestPreludeV1::Blocked(manifest) => return Ok(manifest),
    };
    let topology = ValidatedTopologyV1::new(&input.map, input.shared.run.floor)?;
    append_map_context_with_topology_v1(&mut text, input.shared.run.act, &topology, &input.trail)?;
    append_map_actions_v1(&mut text, &topology, &input.actions)
}

pub(super) fn render_manifest_prelude_v1(
    shared: &SharedExactSceneV1,
    scene: &str,
    additional_gap_ids: &[String],
) -> Result<ManifestPreludeV1, String> {
    validate_shared_scene_v1(shared)?;
    require_token(scene, "scene identity")?;
    if let ParityStateV1::Blocked(difference) = &shared.parity {
        require_token(&difference.diagnostic_id, "diagnostic identity")?;
        require_token(&difference.domain, "difference domain")?;
        require_token(&difference.first_difference, "difference path")?;
        let text = format!(
            "{MANIFEST_VERSION_V1}\npolicy=parity_lab\nworld={}\nauthority={}\nscene={scene}\n\n[parity]\nblocked D0\ndomain={}\nfirst_difference={}\n\n[actions]\nnone\n",
            world_token(shared.world),
            shared.authority.token(),
            difference.domain,
            difference.first_difference,
        );
        return Ok(ManifestPreludeV1::Blocked(RenderedManifestV1 {
            text,
            actions: Vec::new(),
            diagnostics: vec![DiagnosticLeaseEntryV1 {
                alias: "D0".to_owned(),
                diagnostic_id: difference.diagnostic_id.clone(),
            }],
        }));
    }

    let ParityStateV1::Exact(parity) = &shared.parity else {
        unreachable!("blocked parity returned above")
    };
    if parity.total_domains == 0 || parity.exact_domains != parity.total_domains {
        return Err(
            "an actionable Parity Lab manifest requires all declared domains Exact".to_owned(),
        );
    }
    let mut gaps = parity.coverage_gap_ids.clone();
    gaps.extend_from_slice(additional_gap_ids);
    for gap_id in &gaps {
        require_token(gap_id, "coverage-gap identity")?;
    }
    gaps.sort();
    gaps.dedup();

    let mut text = String::new();
    writeln!(text, "{MANIFEST_VERSION_V1}").unwrap();
    writeln!(text, "policy=parity_lab").unwrap();
    writeln!(text, "world={}", world_token(shared.world)).unwrap();
    writeln!(text, "authority={}", shared.authority.token()).unwrap();
    writeln!(text, "scene={scene}\n").unwrap();
    writeln!(text, "[parity]").unwrap();
    writeln!(
        text,
        "exact={}/{}",
        parity.exact_domains, parity.total_domains
    )
    .unwrap();
    if gaps.is_empty() {
        writeln!(text, "gaps=none").unwrap();
    } else {
        writeln!(text, "gaps={}", gaps.join(",")).unwrap();
    }
    append_shared_state_v1(&mut text, shared)?;
    Ok(ManifestPreludeV1::Exact(text))
}

fn append_shared_state_v1(text: &mut String, shared: &SharedExactSceneV1) -> Result<(), String> {
    let run = &shared.run;
    writeln!(text, "\n[run]").unwrap();
    writeln!(
        text,
        "seed={} ascension={} character={} act={} floor={} hp={}/{} gold={} final_act={} keys={}",
        run.seed,
        run.ascension,
        manifest_scalar_v1(&run.character)?,
        run.act,
        run.floor,
        run.current_hp,
        run.max_hp,
        run.gold,
        run.final_act_available,
        key_set_v1(run.keys),
    )
    .unwrap();

    writeln!(text, "\n[deck]").unwrap();
    if shared.master_deck.is_empty() {
        writeln!(text, "none").unwrap();
    }
    for card in &shared.master_deck {
        writeln!(
            text,
            "DECK{} id={} upgrades={}",
            card.ordinal,
            manifest_scalar_v1(&card.card_id)?,
            card.upgrades,
        )
        .unwrap();
    }

    writeln!(text, "\n[relics]").unwrap();
    if shared.relics.is_empty() {
        writeln!(text, "none").unwrap();
    }
    for relic in &shared.relics {
        writeln!(
            text,
            "RELIC{} id={} counter={} used={}",
            relic.ordinal,
            manifest_scalar_v1(&relic.id)?,
            relic.counter,
            relic.used_up,
        )
        .unwrap();
    }

    match &shared.potions {
        PublicPotionBeltV1::Persistent(slots) => {
            writeln!(text, "\n[potions kind=persistent slots={}]", slots.len()).unwrap();
            if slots.is_empty() {
                writeln!(text, "none").unwrap();
            }
            for slot in slots {
                match &slot.id {
                    Some(id) => {
                        writeln!(text, "POTION{} id={}", slot.slot, manifest_scalar_v1(id)?,)
                            .unwrap()
                    }
                    None => writeln!(text, "POTION{} empty", slot.slot).unwrap(),
                }
            }
        }
        PublicPotionBeltV1::Combat(potions) => {
            writeln!(text, "\n[potions kind=combat occupied={}]", potions.len()).unwrap();
            if potions.is_empty() {
                writeln!(text, "none").unwrap();
            }
            for (ordinal, potion) in potions.iter().enumerate() {
                writeln!(
                    text,
                    "POTION{ordinal} slot={} id={} can_use={} can_discard={} target={}",
                    potion.slot,
                    manifest_scalar_v1(&potion.id)?,
                    potion.can_use,
                    potion.can_discard,
                    if potion.target_required {
                        "required"
                    } else {
                        "none"
                    },
                )
                .unwrap();
            }
        }
    }
    Ok(())
}

pub(super) fn append_map_context_v1(
    text: &mut String,
    act: u8,
    floor: i32,
    map: &PublicMapV1,
    trail: &ExactTrailV1,
) -> Result<(), String> {
    let topology = ValidatedTopologyV1::new(map, floor)?;
    append_map_context_with_topology_v1(text, act, &topology, trail)
}

fn append_map_context_with_topology_v1(
    text: &mut String,
    act: u8,
    topology: &ValidatedTopologyV1<'_>,
    trail: &ExactTrailV1,
) -> Result<(), String> {
    writeln!(
        text,
        "\n[map act={} current={} boss={}]",
        act,
        topology.current_handle(),
        topology
            .map
            .boss_id
            .as_deref()
            .map(manifest_scalar_v1)
            .transpose()?
            .as_deref()
            .unwrap_or("none")
    )
    .unwrap();
    writeln!(text, "START -> {}", topology.start_targets()).unwrap();
    let mut ordered_nodes = topology.nodes.values().copied().collect::<Vec<_>>();
    ordered_nodes.sort_by_key(|node| (node.coordinate.y, node.coordinate.x));
    for node in ordered_nodes {
        writeln!(text, "{}", topology.render_node(node)?).unwrap();
    }
    if topology.map.boss_anchor.is_some() {
        writeln!(text, "BOSS boss -> none").unwrap();
    }

    writeln!(text, "\n[trail]").unwrap();
    match trail {
        ExactTrailV1::Available(points) => {
            let handles = topology.validate_trail(points)?;
            writeln!(text, "{}", handles.join(" -> ")).unwrap();
        }
        ExactTrailV1::CoverageGap { gap_id, reason } => {
            writeln!(text, "{gap_id} unavailable reason={reason}").unwrap();
        }
    }
    Ok(())
}

fn append_map_actions_v1(
    text: &mut String,
    topology: &ValidatedTopologyV1<'_>,
    actions: &[MapActionV1],
) -> Result<RenderedManifestV1, String> {
    let mut actions = actions.iter().collect::<Vec<_>>();
    actions.sort_by_key(|action| (action.target.y, action.target.x));
    let mut seen_action_ids = BTreeSet::new();
    let mut seen_targets = BTreeSet::new();
    let mut leases = Vec::with_capacity(actions.len());
    writeln!(text, "\n[actions]").unwrap();
    if actions.is_empty() {
        writeln!(text, "none").unwrap();
    }
    for (ordinal, action) in actions.into_iter().enumerate() {
        require_internal_id(&action.action_id)?;
        if !seen_action_ids.insert(action.action_id.as_str()) || !seen_targets.insert(action.target)
        {
            return Err("map actions contain a duplicate identity or target".to_owned());
        }
        topology.validate_action(action)?;
        let alias = format!("A{ordinal}");
        let mut line = format!("{alias} enter {}", topology.edge_target(action.target)?);
        if let MapActionRouteV1::WingBoots { cost } = action.route {
            write!(line, " via=wing_boots cost={cost}").unwrap();
        }
        let available = append_action_availability_v1(&mut line, &action.availability)?;
        writeln!(text, "{line}").unwrap();
        leases.push(ActionLeaseEntryV1 {
            alias,
            action_id: Some(action.action_id.clone()),
            available,
        });
    }

    Ok(RenderedManifestV1 {
        text: std::mem::take(text),
        actions: leases,
        diagnostics: Vec::new(),
    })
}

pub(super) fn validate_shared_scene_v1(shared: &SharedExactSceneV1) -> Result<(), String> {
    let run = &shared.run;
    if run.seed.parse::<u64>().is_err()
        || run.ascension > 20
        || !(1..=4).contains(&run.act)
        || run.floor < 0
        || run.max_hp <= 0
        || run.current_hp < 0
        || run.current_hp > run.max_hp
        || run.gold < 0
    {
        return Err("manifest shared run facts are invalid".to_owned());
    }
    require_identity(&run.character, "character identity")?;
    for (ordinal, card) in shared.master_deck.iter().enumerate() {
        if card.ordinal != ordinal {
            return Err("manifest master deck ordinals are not contiguous".to_owned());
        }
        require_identity(&card.card_id, "card identity")?;
    }
    for (ordinal, relic) in shared.relics.iter().enumerate() {
        if relic.ordinal != ordinal {
            return Err("manifest relic ordinals are not contiguous".to_owned());
        }
        require_identity(&relic.id, "relic identity")?;
    }
    match &shared.potions {
        PublicPotionBeltV1::Persistent(slots) => {
            for (slot, potion) in slots.iter().enumerate() {
                if potion.slot != slot {
                    return Err("manifest persistent potion slots are not contiguous".to_owned());
                }
                if let Some(id) = &potion.id {
                    require_identity(id, "potion identity")?;
                }
            }
        }
        PublicPotionBeltV1::Combat(potions) => {
            let mut previous = None;
            for potion in potions {
                if previous.is_some_and(|slot| slot >= potion.slot) {
                    return Err("manifest combat potion slots are not strictly ordered".to_owned());
                }
                require_identity(&potion.id, "potion identity")?;
                previous = Some(potion.slot);
            }
        }
    }
    Ok(())
}

fn require_identity(value: &str, name: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(format!("{name} is invalid"));
    }
    Ok(())
}

fn key_set_v1(keys: [bool; 3]) -> String {
    let present = ["ruby", "sapphire", "emerald"]
        .into_iter()
        .zip(keys)
        .filter_map(|(name, present)| present.then_some(name))
        .collect::<Vec<_>>();
    if present.is_empty() {
        "none".to_owned()
    } else {
        present.join("+")
    }
}

fn world_token(world: WorldProvenanceV1) -> &'static str {
    match world {
        WorldProvenanceV1::Natural => "natural",
        WorldProvenanceV1::ResampledContinuation => "resampled_continuation",
        WorldProvenanceV1::ValidatedConstructed => "validated_constructed",
        WorldProvenanceV1::Adversarial => "adversarial",
    }
}

pub(super) fn require_token(value: &str, name: &str) -> Result<(), String> {
    if value.is_empty()
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'/' | b':')
        })
    {
        return Err(format!("{name} is not one safe manifest token"));
    }
    Ok(())
}

pub(super) fn require_internal_id(value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err("manifest action omitted its stable internal identity".to_owned());
    }
    Ok(())
}

pub(super) fn append_action_availability_v1(
    line: &mut String,
    availability: &ActionAvailabilityV1,
) -> Result<bool, String> {
    match availability {
        ActionAvailabilityV1::Verified => Ok(true),
        ActionAvailabilityV1::Unknown { gap_id } => {
            require_token(gap_id, "action gap identity")?;
            write!(line, " unavailable=unknown gap={gap_id}").unwrap();
            Ok(false)
        }
        ActionAvailabilityV1::Unsupported { reason } => {
            require_token(reason, "unsupported action reason")?;
            write!(line, " unavailable=unsupported reason={reason}").unwrap();
            Ok(false)
        }
    }
}

struct ValidatedTopologyV1<'a> {
    map: &'a PublicMapV1,
    floor: i32,
    nodes: BTreeMap<MapCoordinateV1, &'a PublicMapNodeV1>,
    start_nodes: Vec<MapCoordinateV1>,
}

impl<'a> ValidatedTopologyV1<'a> {
    fn new(map: &'a PublicMapV1, floor: i32) -> Result<Self, String> {
        if map
            .current
            .is_some_and(|coordinate| coordinate.x < 0 || coordinate.y < 0)
        {
            return Err(
                "None is the only pre-entry coordinate in the manifest contract".to_owned(),
            );
        }
        let mut nodes = BTreeMap::new();
        for node in &map.nodes {
            if node.coordinate.x < 0
                || node.coordinate.y < 0
                || nodes.insert(node.coordinate, node).is_some()
            {
                return Err("map contains a negative or duplicate node coordinate".to_owned());
            }
            node.room.token(node.burning)?;
            if node.edges.iter().collect::<BTreeSet<_>>().len() != node.edges.len() {
                return Err("map contains a duplicate directed edge".to_owned());
            }
        }
        if nodes.is_empty() {
            return Err("map manifest cannot render an empty topology".to_owned());
        }
        let mut incoming = nodes
            .keys()
            .copied()
            .map(|coordinate| (coordinate, BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();
        for node in nodes.values() {
            for destination in &node.edges {
                if Some(*destination) == map.boss_anchor {
                    continue;
                }
                let parents = incoming.get_mut(destination).ok_or_else(|| {
                    format!(
                        "map edge from ({},{}) targets an absent node ({},{})",
                        node.coordinate.x, node.coordinate.y, destination.x, destination.y
                    )
                })?;
                parents.insert(node.coordinate);
            }
        }
        if let Some(anchor) = map.boss_anchor {
            if map.boss_id.is_none() || nodes.contains_key(&anchor) {
                return Err("map boss anchor is not one typed external destination".to_owned());
            }
            let sources = nodes
                .values()
                .filter(|node| node.edges.contains(&anchor))
                .count();
            if sources == 0 {
                return Err("map boss anchor has no incoming edge".to_owned());
            }
        } else if nodes
            .values()
            .any(|node| node.edges.iter().any(|edge| !nodes.contains_key(edge)))
        {
            return Err("map contains an unexplained external edge".to_owned());
        }
        if let Some(current) = map.current {
            if Some(current) != map.boss_anchor && !nodes.contains_key(&current) {
                return Err("map current coordinate is absent from its topology".to_owned());
            }
        }
        for node in nodes.values() {
            let has_incident_edge = !node.edges.is_empty()
                || incoming
                    .get(&node.coordinate)
                    .is_some_and(|parents| !parents.is_empty())
                || map.current == Some(node.coordinate);
            if !has_incident_edge {
                return Err(format!(
                    "map node ({},{}) is an unclassified grid placeholder",
                    node.coordinate.x, node.coordinate.y
                ));
            }
        }
        let minimum_y = nodes
            .keys()
            .map(|coordinate| coordinate.y)
            .min()
            .expect("non-empty topology");
        let mut start_nodes = incoming
            .iter()
            .filter_map(|(coordinate, parents)| parents.is_empty().then_some(*coordinate))
            .collect::<Vec<_>>();
        if start_nodes.is_empty() || start_nodes.iter().any(|node| node.y != minimum_y) {
            return Err("map topology does not have one coherent START frontier".to_owned());
        }
        start_nodes.sort();
        Ok(Self {
            map,
            floor,
            nodes,
            start_nodes,
        })
    }

    fn current_handle(&self) -> String {
        match self.map.current {
            None => "START".to_owned(),
            Some(current) if Some(current) == self.map.boss_anchor => "BOSS".to_owned(),
            Some(current) => self
                .node_handle(current)
                .unwrap_or_else(|_| "INVALID".to_owned()),
        }
    }

    fn global_floor(&self, coordinate: MapCoordinateV1) -> Result<i32, String> {
        let value = match self.map.current {
            None => self.floor + coordinate.y + 1,
            Some(current) if Some(current) == self.map.boss_anchor => {
                return Err(
                    "matrix floor conversion is unavailable from the boss anchor".to_owned(),
                )
            }
            Some(current) => self.floor + coordinate.y - current.y,
        };
        if value <= 0 {
            return Err("map coordinate converts to a non-positive global floor".to_owned());
        }
        Ok(value)
    }

    fn node_handle(&self, coordinate: MapCoordinateV1) -> Result<String, String> {
        if !self.nodes.contains_key(&coordinate) {
            return Err("map handle requested for an absent node".to_owned());
        }
        let floor = self.global_floor(coordinate)?;
        Ok(format!("F{floor:02}X{:02}", coordinate.x))
    }

    fn node_tag(&self, coordinate: MapCoordinateV1) -> Result<&'static str, String> {
        let node = self
            .nodes
            .get(&coordinate)
            .ok_or_else(|| "map tag requested for an absent node".to_owned())?;
        node.room.token(node.burning)
    }

    fn edge_target(&self, coordinate: MapCoordinateV1) -> Result<String, String> {
        if Some(coordinate) == self.map.boss_anchor {
            return Ok("BOSS:boss".to_owned());
        }
        Ok(format!(
            "{}:{}",
            self.node_handle(coordinate)?,
            self.node_tag(coordinate)?
        ))
    }

    fn start_targets(&self) -> String {
        self.start_nodes
            .iter()
            .map(|coordinate| self.edge_target(*coordinate).expect("validated start node"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn render_node(&self, node: &PublicMapNodeV1) -> Result<String, String> {
        let mut edges = node.edges.clone();
        edges.sort();
        let targets = if edges.is_empty() {
            "none".to_owned()
        } else {
            edges
                .into_iter()
                .map(|coordinate| self.edge_target(coordinate))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        };
        Ok(format!(
            "{} {} -> {targets}",
            self.node_handle(node.coordinate)?,
            node.room.token(node.burning)?
        ))
    }

    fn validate_trail(&self, points: &[TrailPointV1]) -> Result<Vec<String>, String> {
        if points.is_empty() || points[0] != TrailPointV1::Start {
            return Err("exact map trail must begin at START".to_owned());
        }
        let mut handles = vec!["START".to_owned()];
        for pair in points.windows(2) {
            let valid = match pair {
                [TrailPointV1::Start, TrailPointV1::Node(target)] => {
                    self.start_nodes.contains(target)
                }
                [TrailPointV1::Node(source), TrailPointV1::Node(target)] => self
                    .nodes
                    .get(source)
                    .is_some_and(|node| node.edges.contains(target)),
                [TrailPointV1::Node(source), TrailPointV1::Boss] => {
                    self.nodes.get(source).is_some_and(|node| {
                        self.map
                            .boss_anchor
                            .is_some_and(|boss| node.edges.contains(&boss))
                    })
                }
                _ => false,
            };
            if !valid {
                return Err("exact map trail is not a path through the public topology".to_owned());
            }
            handles.push(match pair[1] {
                TrailPointV1::Start => unreachable!(),
                TrailPointV1::Node(node) => self.node_handle(node)?,
                TrailPointV1::Boss => "BOSS".to_owned(),
            });
        }
        let expected = match self.map.current {
            None => TrailPointV1::Start,
            Some(current) if Some(current) == self.map.boss_anchor => TrailPointV1::Boss,
            Some(current) => TrailPointV1::Node(current),
        };
        if points.last().copied() != Some(expected) {
            return Err("exact map trail does not end at the current map position".to_owned());
        }
        Ok(handles)
    }

    fn validate_action(&self, action: &MapActionV1) -> Result<(), String> {
        if !self.nodes.contains_key(&action.target) && Some(action.target) != self.map.boss_anchor {
            return Err("map action targets an absent topology node".to_owned());
        }
        match action.route {
            MapActionRouteV1::GraphEdge => {
                let valid = match self.map.current {
                    None => self.start_nodes.contains(&action.target),
                    Some(current) if Some(current) == self.map.boss_anchor => false,
                    Some(current) => self
                        .nodes
                        .get(&current)
                        .is_some_and(|node| node.edges.contains(&action.target)),
                };
                if !valid {
                    return Err(
                        "ordinary map action is not an edge from the current position".to_owned(),
                    );
                }
            }
            MapActionRouteV1::WingBoots { cost } => {
                if cost == 0 {
                    return Err("Wing Boots action must consume a positive charge".to_owned());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_scalar_uses_one_closed_lexical_contract() {
        assert_eq!(manifest_scalar_v1("Strike_R").unwrap(), "Strike_R");
        assert_eq!(
            manifest_scalar_v1("Perfected Strike").unwrap(),
            "\"Perfected Strike\""
        );
        assert_eq!(manifest_scalar_v1("none").unwrap(), "\"none\"");
        assert!(manifest_scalar_v1(" ").is_err());
        assert!(manifest_scalar_v1("bad\nvalue").is_err());
    }

    fn coordinate(x: i32, y: i32) -> MapCoordinateV1 {
        MapCoordinateV1 { x, y }
    }

    fn fixture() -> ParityLabMapInputV1 {
        ParityLabMapInputV1 {
            shared: SharedExactSceneV1 {
                world: WorldProvenanceV1::Natural,
                authority: ManifestAuthorityV1::LabHead,
                parity: ParityStateV1::Exact(ExactParityV1 {
                    exact_domains: 10,
                    total_domains: 10,
                    coverage_gap_ids: Vec::new(),
                }),
                run: PublicRunV1 {
                    seed: "1".to_owned(),
                    ascension: 20,
                    character: "IRONCLAD".to_owned(),
                    act: 1,
                    floor: 2,
                    current_hp: 75,
                    max_hp: 75,
                    gold: 99,
                    final_act_available: true,
                    keys: [false; 3],
                },
                master_deck: Vec::new(),
                relics: Vec::new(),
                potions: PublicPotionBeltV1::Persistent(Vec::new()),
            },
            map: PublicMapV1 {
                current: Some(coordinate(2, 1)),
                boss_id: Some("hexaghost".to_owned()),
                boss_anchor: Some(coordinate(3, 16)),
                nodes: vec![
                    PublicMapNodeV1 {
                        coordinate: coordinate(1, 0),
                        room: MapRoomTagV1::Monster,
                        burning: false,
                        edges: vec![coordinate(2, 1)],
                    },
                    PublicMapNodeV1 {
                        coordinate: coordinate(2, 1),
                        room: MapRoomTagV1::Question,
                        burning: false,
                        edges: vec![coordinate(2, 2), coordinate(3, 2)],
                    },
                    PublicMapNodeV1 {
                        coordinate: coordinate(2, 2),
                        room: MapRoomTagV1::Monster,
                        burning: false,
                        edges: vec![coordinate(3, 16)],
                    },
                    PublicMapNodeV1 {
                        coordinate: coordinate(3, 2),
                        room: MapRoomTagV1::Elite,
                        burning: true,
                        edges: vec![coordinate(3, 16)],
                    },
                ],
            },
            trail: ExactTrailV1::Available(vec![
                TrailPointV1::Start,
                TrailPointV1::Node(coordinate(1, 0)),
                TrailPointV1::Node(coordinate(2, 1)),
            ]),
            actions: vec![
                MapActionV1 {
                    action_id: "paired-action-v1-first".to_owned(),
                    target: coordinate(3, 2),
                    route: MapActionRouteV1::GraphEdge,
                    availability: ActionAvailabilityV1::Unknown {
                        gap_id: "elite-transition-gap".to_owned(),
                    },
                },
                MapActionV1 {
                    action_id: "paired-action-v1-second".to_owned(),
                    target: coordinate(2, 2),
                    route: MapActionRouteV1::GraphEdge,
                    availability: ActionAvailabilityV1::Verified,
                },
            ],
        }
    }

    #[test]
    fn exact_map_manifest_is_complete_deterministic_and_hash_free() {
        let rendered = render_parity_lab_map_v1(&fixture()).unwrap();
        assert_eq!(
            rendered.text,
            concat!(
                "STS-MANIFEST 1\n",
                "policy=parity_lab\n",
                "world=natural\n",
                "authority=lab_head\n",
                "scene=map\n\n",
                "[parity]\n",
                "exact=10/10\n",
                "gaps=none\n\n",
                "[run]\n",
                "seed=1 ascension=20 character=IRONCLAD act=1 floor=2 hp=75/75 gold=99 final_act=true keys=none\n\n",
                "[deck]\n",
                "none\n\n",
                "[relics]\n",
                "none\n\n",
                "[potions kind=persistent slots=0]\n",
                "none\n\n",
                "[map act=1 current=F02X02 boss=hexaghost]\n",
                "START -> F01X01:monster\n",
                "F01X01 monster -> F02X02:question\n",
                "F02X02 question -> F03X02:monster, F03X03:elite+burning\n",
                "F03X02 monster -> BOSS:boss\n",
                "F03X03 elite+burning -> BOSS:boss\n",
                "BOSS boss -> none\n\n",
                "[trail]\n",
                "START -> F01X01 -> F02X02\n\n",
                "[actions]\n",
                "A0 enter F03X02:monster\n",
                "A1 enter F03X03:elite+burning unavailable=unknown gap=elite-transition-gap\n",
            )
        );
        assert_eq!(rendered.actions[0].alias, "A0");
        assert_eq!(
            rendered.actions[0].action_id.as_deref(),
            Some("paired-action-v1-second")
        );
        assert!(rendered.actions[0].available);
        assert!(!rendered.actions[1].available);
        assert!(!rendered.text.contains("paired-action-v1"));
        assert!(!rendered.text.contains("sha"));
    }

    #[test]
    fn missing_trail_is_an_explicit_coverage_gap() {
        let mut input = fixture();
        input.trail = ExactTrailV1::CoverageGap {
            gap_id: "map-trail-gap".to_owned(),
            reason: "exact_history_not_projected".to_owned(),
        };
        let rendered = render_parity_lab_map_v1(&input).unwrap();
        assert!(rendered.text.contains("gaps=map-trail-gap"));
        assert!(rendered
            .text
            .contains("map-trail-gap unavailable reason=exact_history_not_projected"));
    }

    #[test]
    fn blocked_parity_exposes_only_one_diagnostic_alias() {
        let mut input = fixture();
        input.shared.parity = ParityStateV1::Blocked(FirstDifferenceV1 {
            diagnostic_id: "divergence-internal-1".to_owned(),
            domain: "event_identity".to_owned(),
            first_difference: "canonical_id".to_owned(),
        });
        let rendered = render_parity_lab_map_v1(&input).unwrap();
        assert!(rendered.text.contains("blocked D0"));
        assert!(rendered.text.contains("[actions]\nnone"));
        assert!(!rendered.text.contains("[map"));
        assert!(rendered.actions.is_empty());
        assert_eq!(rendered.diagnostics[0].alias, "D0");
        assert_eq!(
            rendered.diagnostics[0].diagnostic_id,
            "divergence-internal-1"
        );
    }

    #[test]
    fn renderer_rejects_grid_placeholders_before_any_runtime_work() {
        let mut input = fixture();
        input.map.nodes.push(PublicMapNodeV1 {
            coordinate: coordinate(6, 8),
            room: MapRoomTagV1::Treasure,
            burning: false,
            edges: Vec::new(),
        });
        let error = render_parity_lab_map_v1(&input).unwrap_err();
        assert!(error.contains("grid placeholder"));
    }

    #[test]
    fn renderer_rejects_invalid_edges_trails_and_action_targets() {
        let mut absent_edge = fixture();
        absent_edge.map.nodes[0].edges = vec![coordinate(6, 7)];
        assert!(render_parity_lab_map_v1(&absent_edge)
            .unwrap_err()
            .contains("absent node"));

        let mut false_trail = fixture();
        false_trail.trail = ExactTrailV1::Available(vec![
            TrailPointV1::Start,
            TrailPointV1::Node(coordinate(3, 2)),
        ]);
        assert!(render_parity_lab_map_v1(&false_trail)
            .unwrap_err()
            .contains("not a path"));

        let mut false_action = fixture();
        false_action.actions[0].target = coordinate(1, 0);
        assert!(render_parity_lab_map_v1(&false_action)
            .unwrap_err()
            .contains("not an edge"));
    }

    #[test]
    fn external_boss_anchor_can_be_the_current_map_action() {
        let mut input = fixture();
        input.shared.run.floor = 3;
        input.map.current = Some(coordinate(2, 2));
        input.trail = ExactTrailV1::Available(vec![
            TrailPointV1::Start,
            TrailPointV1::Node(coordinate(1, 0)),
            TrailPointV1::Node(coordinate(2, 1)),
            TrailPointV1::Node(coordinate(2, 2)),
        ]);
        input.actions = vec![MapActionV1 {
            action_id: "paired-action-v1-boss".to_owned(),
            target: coordinate(3, 16),
            route: MapActionRouteV1::GraphEdge,
            availability: ActionAvailabilityV1::Verified,
        }];

        let rendered = render_parity_lab_map_v1(&input).unwrap();
        assert!(rendered.text.contains("A0 enter BOSS:boss"));
    }
}
