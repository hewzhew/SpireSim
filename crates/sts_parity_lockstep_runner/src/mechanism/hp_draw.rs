//! Closed captured recipes for HP-loss draw relics. Each before-frame UUID
//! declares a NEW constructed-scenario slot; no independently simulated deck
//! is matched by name, position, or UUID value. Successors only use that lease.
use super::{
    combat_mechanism_input, end_turn::compare_end_turn_with_cards, validate_end_turn_evidence,
};
mod power_cases;
#[cfg(test)]
mod power_tests;
#[cfg(test)]
mod tests;
pub use power_cases::HpDrawPowerCaseV1;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use sts_oracle_run_control::{
    content::{
        cards::CardId,
        monsters::{beyond::spire_growth::SpireGrowth, factory::EncounterId, MonsterBehavior},
        powers::{store, PowerId},
        relics::{RelicId, RelicState},
    },
    engine::core::tick_until_stable_turn,
    runtime::{
        combat::{CardLineageId, CombatCard, Power},
        rng::begin_sts_rng_trace,
    },
    state::{
        core::{ClientInput, EngineState},
        map::RoomType,
    },
};
use sts_parity_lockstep::{
    compare_snapshots_v1, ComparisonStatusV1, DomainObservationV1, NaturalCombatZonesV4,
    NaturalRelicCombatStateV1, NaturalRelicV3, ParityDomainV1, ParitySnapshotV1,
    ProducerIdentityV1, SnapshotComparisonV1, SnapshotContractV1,
};
use sts_parity_lockstep_java::{
    project_cards_relics_mechanism_v1, project_combat_rng_mechanism_v1,
    project_monster_ai_mechanism_v1, project_player_mechanism_v1, RecordedJavaActionV1,
    RecordedJavaBoundaryV1,
};

pub const HP_DRAW_MECHANISM_SCOPE_V1: &str = "Constructed A20 HP-loss EndTurn with Centennial Puzzle/Runic Cube and optional Buffer/Tungsten Rod: creatures, ordered source-bound combat cards, relic state, AI/shuffle states and complete RNG calls; closed captured scenario only, no natural certification";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HpDrawCaseV1 {
    Power(HpDrawPowerCaseV1),
    PuzzleBurnAnchor,
    CubeBurnAnchor,
    PuzzleBurn,
    PuzzleBurnReset,
    PuzzleMixedBuffer,
    PuzzleDecayTungsten,
    PuzzleMixedTungsten,
    CubeBurn,
    CubeMixed,
    CubeMixedBuffer,
    CubeDecayTungsten,
    PuzzleCubeMixed,
    CubePuzzleMixed,
}
impl HpDrawCaseV1 {
    pub(super) const PREVENTION: &[Self] = &[Self::PuzzleBurnAnchor, Self::CubeBurnAnchor];
    pub(super) const ALL: &[Self] = &[
        Self::PuzzleBurn,
        Self::PuzzleBurnReset,
        Self::PuzzleMixedBuffer,
        Self::PuzzleDecayTungsten,
        Self::PuzzleMixedTungsten,
        Self::CubeBurn,
        Self::CubeMixed,
        Self::CubeMixedBuffer,
        Self::CubeDecayTungsten,
        Self::PuzzleCubeMixed,
        Self::CubePuzzleMixed,
    ];
    pub(super) const POWERS: &[Self] = HpDrawPowerCaseV1::ALL;
    pub(super) fn name(self) -> String {
        if let Self::Power(case) = self {
            return serde_json::to_value(case)
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned();
        }
        serde_json::to_value(self)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }
    fn cards(self) -> Vec<CardId> {
        match self {
            Self::Power(case) => case
                .master_deck()
                .into_iter()
                .filter(|(id, _)| {
                    !matches!(id, CardId::Rupture | CardId::Combust | CardId::Brutality)
                })
                .map(|(id, _)| id)
                .collect(),
            Self::PuzzleBurn
            | Self::PuzzleBurnReset
            | Self::CubeBurn
            | Self::PuzzleBurnAnchor
            | Self::CubeBurnAnchor => vec![CardId::Burn; 10],
            Self::PuzzleDecayTungsten | Self::CubeDecayTungsten => vec![CardId::Decay; 10],
            _ => [vec![CardId::Burn; 5], vec![CardId::Regret; 5]].concat(),
        }
    }
    fn relics(self) -> Vec<RelicId> {
        use RelicId::*;
        match self {
            Self::Power(case) => case.relics(),
            Self::PuzzleBurnAnchor => vec![CentennialPuzzle, Anchor],
            Self::CubeBurnAnchor => vec![RunicCube, Anchor],
            Self::PuzzleBurn | Self::PuzzleBurnReset => vec![CentennialPuzzle],
            Self::PuzzleMixedBuffer => vec![CentennialPuzzle, FossilizedHelix],
            Self::PuzzleDecayTungsten | Self::PuzzleMixedTungsten => {
                vec![CentennialPuzzle, TungstenRod]
            }
            Self::CubeBurn | Self::CubeMixed => vec![RunicCube],
            Self::CubeMixedBuffer => vec![RunicCube, FossilizedHelix],
            Self::CubeDecayTungsten => vec![RunicCube, TungstenRod],
            Self::PuzzleCubeMixed => vec![CentennialPuzzle, RunicCube],
            Self::CubePuzzleMixed => vec![RunicCube, CentennialPuzzle],
        }
    }
    pub(super) fn preparations(self) -> Vec<(CardId, u8)> {
        match self {
            Self::Power(case) => case.preparations(),
            _ => Vec::new(),
        }
    }
    fn energy(self) -> u8 {
        match self {
            Self::Power(case) => case.energy(),
            _ => 3,
        }
    }
    fn input_powers(self) -> Vec<Power> {
        let mut powers = Vec::new();
        if self.relics().contains(&RelicId::FossilizedHelix) {
            powers.push(power_cases::power(PowerId::Buffer, 1, 0));
        }
        if let Self::Power(case) = self {
            powers.extend(case.powers());
        }
        powers
    }
    pub(super) fn draft(self) -> serde_json::Value {
        let master = match self {
            Self::Power(case) => case.master_deck(),
            _ => self.cards().into_iter().map(|id| (id, 0)).collect(),
        };
        let cards: Vec<_> = master
            .iter()
            .map(|(id, upgrades)| json!({"card_id":id.stable_id(),"upgrades":upgrades}))
            .collect();
        let relics: Vec<_> = self
            .relics()
            .iter()
            .map(|id| {
                json!({"relic_id": match id {
                    RelicId::CentennialPuzzle => "Centennial Puzzle",
                    RelicId::RunicCube => "Runic Cube",
                    other => other.stable_id(),
                }})
            })
            .collect();
        json!({
            "schema":"sts_java_play_scenario_draft_v3", "provenance":"constructed_validated", "case_id":format!("hp-draw-{}",self.name()),
            "run":{"character":"IRONCLAD","ascension":20,"seed":"20260907","content_profile":"all_unlocked"},
            "recipe":{"recipe_id":"fresh_neow_to_combat_root.v1","capabilities":[
                {"capability_id":"combat.encounter.exact.v1","payload":{"encounter_id":"Spire Growth"}},
                {"capability_id":"player.health.v1","payload":{"current_hp":80,"max_hp":80}},
                {"capability_id":"player.master_deck.exact.v1","payload":{"cards":cards}},
                {"capability_id":"player.relics.fresh_exact.v1","payload":{"relics":relics}}
            ]}
        })
    }
}

/// A scoped source witness, derived solely from the validated BEFORE frame.
/// Ordinals declare fresh recipe slots, never a mapping to an existing run.
#[derive(Debug, Serialize)]
pub(super) struct CapturedHpDrawRecipeV1 {
    schema: &'static str,
    producer: ProducerIdentityV1,
    frame_id: u64,
    cards: Vec<CapturedCardSlot>,
    hand_len: usize,
    #[serde(skip)]
    java_bindings: BTreeMap<String, String>,
    #[serde(skip)]
    rust_bindings: BTreeMap<CardLineageId, String>,
}
#[derive(Debug, Serialize)]
struct CapturedCardSlot {
    recipe_slot: u32,
    java_uuid: String,
    card_id: CardId,
}
impl CapturedHpDrawRecipeV1 {
    pub(super) fn compile(
        case: HpDrawCaseV1,
        before: &RecordedJavaBoundaryV1,
    ) -> Result<Self, String> {
        before.validate()?;
        let frame = &before.observation.frame;
        let (player, powers) = project_player_mechanism_v1(&before.observation)?;
        let (monsters, _) = project_monster_ai_mechanism_v1(&before.observation)?;
        if frame["scenario"]["provenance"] != "constructed_validated"
            || frame["scenario"]["capabilities"] != case.draft()["recipe"]["capabilities"]
            || frame["scenario"]["case_id"] != json!(format!("hp-draw-{}", case.name()))
            || frame["public_base"]["run"]["ascension"] != 20
            || frame["public_base"]["run"]["character"] != "IRONCLAD"
            || frame["public_base"]["combat"]["turn"] != 1
            || player.current_hp != 80
            || player.max_hp != 80
            || player.block
                != if case.relics().contains(&RelicId::Anchor) {
                    10
                } else {
                    0
                }
            || player.energy != case.energy()
            || monsters.len() != 1
            || monsters[0].enemy_id != "spiregrowth"
            || monsters[0].next_move_id != 2
            || monsters[0].dead_or_escaped
            || monsters[0].current_hp != monsters[0].max_hp
            || !monsters[0].powers.is_empty()
        {
            return Err("HP draw requires its closed fresh A20 Spire Growth input".into());
        }
        let expected_powers = case.input_powers();
        if powers.len() != expected_powers.len()
            || powers
                .iter()
                .zip(&expected_powers)
                .any(|(observed, expected)| {
                    observed.power_id
                        != sts_java_parity_protocol::canonical_game_id(
                            expected.power_type.stable_id(),
                        )
                        || observed.amount != expected.amount
                        || observed.occurrence != 0
                        || observed.state
                            != if expected.power_type == PowerId::Combust {
                                sts_parity_lockstep::NaturalCombatPowerStateV4::Combust {
                                    hp_loss: expected.extra_data,
                                }
                            } else {
                                sts_parity_lockstep::NaturalCombatPowerStateV4::Stateless
                            }
                })
        {
            return Err("HP draw input has undeclared powers or private state".into());
        }
        let zones = &frame["parity_internal"]["zones"];
        let mut recipe = Self {
            schema: "sts-captured-hp-draw-recipe-v1",
            producer: before.observation.producer.clone(),
            frame_id: before.observation.frame_id,
            cards: vec![],
            hand_len: 5 - case.preparations().len(),
            java_bindings: BTreeMap::new(),
            rust_bindings: BTreeMap::new(),
        };
        for zone in ["hand", "draw", "discard", "exhaust", "limbo"] {
            let cards = zones[zone]["cards"]
                .as_array()
                .ok_or("HP draw omitted a card zone")?;
            let expected_len = match zone {
                "hand" => recipe.hand_len,
                "draw" => 5,
                _ => 0,
            };
            if cards.len() != expected_len {
                return Err("HP draw requires the declared post-preparation hand, five draw cards and other piles empty".into());
            }
            for raw in cards {
                let uuid = raw["instance_uuid"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("HP draw card UUID missing")?;
                let id = match raw["card_id"].as_str() {
                    Some("Burn") => CardId::Burn,
                    Some("Regret") => CardId::Regret,
                    Some("Decay") => CardId::Decay,
                    Some("Wound") => CardId::Wound,
                    _ => return Err("HP draw card is outside the closed recipe".into()),
                };
                if raw["upgrades"] != 0 || raw["combat_cost"] != -2 || raw["cost_for_turn"] != -2 {
                    return Err("HP draw recipe requires unmodified unplayable cards".into());
                }
                let slot = recipe.cards.len() as u32;
                let binding = format!("captured-hp-draw/{slot}");
                if recipe
                    .java_bindings
                    .insert(uuid.to_owned(), binding.clone())
                    .is_some()
                {
                    return Err("HP draw before frame repeats a card UUID".into());
                }
                recipe
                    .rust_bindings
                    .insert(CombatCard::for_scenario(id, slot).lineage, binding);
                recipe.cards.push(CapturedCardSlot {
                    recipe_slot: slot,
                    java_uuid: uuid.to_owned(),
                    card_id: id,
                });
            }
        }
        let counts = |cards: Vec<CardId>| {
            let mut result = BTreeMap::new();
            for id in cards {
                *result.entry(id.stable_id()).or_insert(0usize) += 1;
            }
            result
        };
        if counts(recipe.cards.iter().map(|c| c.card_id).collect()) != counts(case.cards()) {
            return Err("HP draw captured cards differ from declared ingredients".into());
        }
        let (_, relics) =
            project_cards_relics_mechanism_v1(&before.observation, &recipe.java_bindings)?;
        let expected: Vec<_> = case
            .relics()
            .iter()
            .map(|id| sts_java_parity_protocol::canonical_game_id(id.stable_id()))
            .collect();
        if relics.iter().map(|r| &r.id).ne(expected.iter())
            || relics.iter().any(|r| {
                r.counter != -1
                    || r.used_up
                    || r.bound_card.is_some()
                    || if r.id == "centennialpuzzle" {
                        r.combat_state
                            != Some(NaturalRelicCombatStateV1::CentennialPuzzle {
                                used_this_combat: false,
                            })
                    } else {
                        r.combat_state.is_some()
                    }
            })
        {
            return Err("HP draw requires exact fresh relic order and private state".into());
        }
        Ok(recipe)
    }
}

pub fn replay_hp_draw_mechanism_v1(
    case: HpDrawCaseV1,
    before: &RecordedJavaBoundaryV1,
    action: &RecordedJavaActionV1,
    after: &RecordedJavaBoundaryV1,
) -> Result<SnapshotComparisonV1, String> {
    validate_end_turn_evidence(before, action, after)?;
    if after.observation.frame["scenario"] != before.observation.frame["scenario"] {
        return Err("HP draw successor changed scenario".into());
    }
    let (mut session, recipe) = build_input(case, before)?;
    let combat = &mut session
        .active_combat
        .as_mut()
        .ok_or("Rust combat missing")?
        .combat_state;
    combat.rng.pool.bind_trace_owners();
    let trace = begin_sts_rng_trace().map_err(|e| e.to_string())?;
    if !tick_until_stable_turn(&mut session.engine_state, combat, ClientInput::EndTurn)
        || session.engine_state != EngineState::CombatPlayerTurn
    {
        return Err("HP draw did not settle at next player turn".into());
    }
    let calls = trace.finish().map_err(|e| e.to_string())?;
    let java_cards = project_cards_relics_mechanism_v1(&after.observation, &recipe.java_bindings)?;
    let rust_cards =
        sts_parity_lockstep_rust::project_cards_relics_mechanism_v1(combat, &recipe.rust_bindings)?;
    compare_end_turn_with_cards(
        "sts-hp-draw-cards-relics-creatures-rng-v1",
        combat,
        before,
        action,
        after,
        &calls,
        Some([java_cards, rust_cards]),
    )
}

fn card_snapshot(
    producer: ProducerIdentityV1,
    (zones, relics): (NaturalCombatZonesV4, Vec<NaturalRelicV3>),
) -> Result<ParitySnapshotV1, String> {
    Ok(ParitySnapshotV1 {
        observation_id: "hp-draw-mechanism".into(),
        boundary_kind: "constructed_hp_draw".into(),
        producer,
        contract: SnapshotContractV1::new(
            "sts-hp-draw-cards-relics-v1",
            [
                (ParityDomainV1::Combat, true),
                (ParityDomainV1::Relics, true),
            ],
        )?,
        domains: [
            (
                ParityDomainV1::Combat,
                DomainObservationV1::observed("sts-mechanism-combat-zones-v4", json!(zones)),
            ),
            (
                ParityDomainV1::Relics,
                DomainObservationV1::observed(
                    sts_parity_lockstep::NATURAL_RUN_RELICS_SCHEMA_V3,
                    json!(relics),
                ),
            ),
        ]
        .into_iter()
        .collect(),
    })
}

/// Rebuild and verify the complete declared BEFORE input before Java commits.
/// No successor data or natural-run card identity enters this constructor.
pub(super) fn build_input(
    case: HpDrawCaseV1,
    before: &RecordedJavaBoundaryV1,
) -> Result<
    (
        sts_oracle_run_control::eval::run_control::RunControlSession,
        CapturedHpDrawRecipeV1,
    ),
    String,
> {
    let recipe = CapturedHpDrawRecipeV1::compile(case, before)?;
    let (monsters, _) = project_monster_ai_mechanism_v1(&before.observation)?;
    let (mut session, _) =
        combat_mechanism_input(20, EncounterId::SpireGrowth, RoomType::MonsterRoom)?;
    let combat = &mut session
        .active_combat
        .as_mut()
        .ok_or("Rust combat missing")?
        .combat_state;
    let input = &monsters[0];
    let monster = &mut combat.entities.monsters[0];
    monster.current_hp = input.current_hp;
    monster.max_hp = input.max_hp;
    monster.block = input.block;
    monster.set_planned_move_id(2);
    monster.move_history_mut().clear();
    monster.move_history_mut().extend(&input.move_history);
    let plan = SpireGrowth::turn_plan(combat, &combat.entities.monsters[0]);
    combat.entities.monsters[0].set_planned_steps(plan.steps);
    combat.entities.monsters[0].set_planned_visible_spec(plan.visible_spec);
    combat.entities.player.current_hp = 80;
    combat.entities.player.max_hp = 80;
    combat.entities.player.block = if case.relics().contains(&RelicId::Anchor) {
        10
    } else {
        0
    };
    combat.turn.energy = case.energy();
    combat.entities.player.relics.clear();
    combat.entities.player.relic_buses = Default::default();
    for id in case.relics() {
        combat.entities.player.add_relic(RelicState::new(id));
    }
    store::set_powers_for(combat, 0, case.input_powers());
    let materialize = |c: &CapturedCardSlot| CombatCard::for_scenario(c.card_id, c.recipe_slot);
    combat.zones.hand = recipe.cards[..recipe.hand_len]
        .iter()
        .map(materialize)
        .collect();
    // Java CardGroup top is last; Rust draw is next-first. The slots themselves
    // were already declared by the before frame, independent of this ordering.
    combat.zones.draw_pile = recipe.cards[recipe.hand_len..]
        .iter()
        .rev()
        .map(materialize)
        .collect();
    combat.zones.discard_pile.clear();
    combat.zones.exhaust_pile.clear();
    combat.zones.limbo.clear();
    for stream in project_combat_rng_mechanism_v1(&before.observation)?.0 {
        let rng = match stream.owner.as_str() {
            "floor.ai" => &mut combat.rng.ai_rng,
            "floor.shuffle" => &mut combat.rng.shuffle_rng,
            _ => continue,
        };
        rng.seed0 = u64::from_str_radix(&stream.state_hex[0], 16).map_err(|e| e.to_string())?;
        rng.seed1 = u64::from_str_radix(&stream.state_hex[1], 16).map_err(|e| e.to_string())?;
        rng.counter = stream.counter.ok_or("HP draw RNG counter missing")?;
    }
    // Verify construction with normal projections BEFORE executing. Successor
    // state never participates in reconstruction or binding allocation.
    let compare_cards = |combat: &sts_oracle_run_control::runtime::combat::CombatState,
                         java: &RecordedJavaBoundaryV1| {
        let rust = sts_parity_lockstep_rust::project_cards_relics_mechanism_v1(
            combat,
            &recipe.rust_bindings,
        )?;
        let java_value =
            project_cards_relics_mechanism_v1(&java.observation, &recipe.java_bindings)?;
        compare_snapshots_v1(
            &card_snapshot(java.observation.producer.clone(), java_value)?,
            &card_snapshot(
                ProducerIdentityV1 {
                    implementation: "sts_oracle_run_control".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    session_epoch: "constructed-offline".into(),
                },
                rust,
            )?,
        )
    };
    if compare_cards(combat, before)?.status != ComparisonStatusV1::Exact {
        return Err("constructed cards/relics do not match declared before frame".into());
    }

    let rust_creatures = sts_parity_lockstep_rust::project_creatures_mechanism_v1(combat)?;
    let (player, powers) = project_player_mechanism_v1(&before.observation)?;
    if rust_creatures != (player, powers, monsters) {
        return Err("constructed creature input does not match the original before frame".into());
    }
    Ok((session, recipe))
}
