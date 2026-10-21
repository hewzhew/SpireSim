//! Small, explicitly scoped resource evaluator. Predictions schedule work; they
//! are neither combat facts nor bounds. The simulator still executes every move.
use crate::CombatSearchState;
use serde::{Deserialize, Serialize};
use sts_core::content::{
    cards::{
        can_play_card, evaluate_card_for_play, get_card_definition,
        mechanics::{
            card_base_block_delivery_v1, card_base_damage_delivery_v1,
            unconditional_card_draw_request_v1, CardAttackHitsV1, CardBaseDamageDeliveryV1,
        },
        CardId,
    },
    monsters::EnemyId,
    powers::{mechanics::typed_profile, PowerId},
    relics::RelicId,
};
use sts_core::sim::combat::CombatPosition;
use sts_core::sim::combat_projection::project_monster_move_preview_in_combat;
use sts_core::state::{
    core::{EngineState, GridSelectReason, HandSelectReason, PendingChoice, PileType},
    selection::SelectionReason,
};

pub const SCHEMA: u32 = 4;
pub const OBJECTIVE: &str = "guardian_no_potions_terminal_hp_v1";
/// Value of one named deterministic continuation, not an optimal-value target.
pub const CONTINUATION_OBJECTIVE: &str = "guardian_retained_tactical_argmax_return_v1";
/// Return of a fixed-budget planner with an executed tactical control.
pub const SEARCH_CONTINUATION_OBJECTIVE: &str = "guardian_solver_plus_tactical_return_v1";
/// Fresh search plus independently replayed winning controls already observed.
pub const WITNESS_CONTINUATION_OBJECTIVE: &str = "guardian_solver_plus_replayed_witness_return_v1";
/// Monotone retained-HP goals for one declared executor, not optimal play.
pub const RESOURCE_CURVE_OBJECTIVE: &str = "guardian_fixed_executor_resource_curve_v1";
pub mod data;
// The first study has a declared vocabulary, not a silently lossy unknown-card bucket.
pub const CARDS: &[CardId] = &[
    CardId::Strike,
    CardId::Defend,
    CardId::Bash,
    CardId::AscendersBane,
    CardId::Carnage,
    CardId::PommelStrike,
    CardId::ShrugItOff,
    CardId::Inflame,
    CardId::Disarm,
    CardId::Impervious,
    CardId::Headbutt,
    CardId::TwinStrike,
    CardId::SwordBoomerang,
    CardId::BodySlam,
    CardId::PowerThrough,
    CardId::Wound,
    CardId::Dazed,
    CardId::Burn,
    CardId::TrueGrit,
    CardId::Armaments,
];
const PLAYER_POWERS: &[PowerId] = &[
    PowerId::Strength,
    PowerId::Dexterity,
    PowerId::Weak,
    PowerId::Vulnerable,
    PowerId::Frail,
    PowerId::Metallicize,
    PowerId::PlatedArmor,
    PowerId::FeelNoPain,
    PowerId::DarkEmbrace,
    PowerId::Barricade,
    PowerId::Artifact,
];
const ENEMY_POWERS: &[PowerId] = &[
    PowerId::Strength,
    PowerId::Weak,
    PowerId::Vulnerable,
    PowerId::SharpHide,
    PowerId::ModeShift,
    PowerId::Artifact,
];

/// Check the deliberately small pilot domain, without claiming natural origins.
pub fn validate_position(p: &CombatPosition) -> Result<(), String> {
    let c = &p.combat;
    if c.meta.ascension_level != 20
        || c.entities.monsters.len() != 1
        || c.entities.monsters[0].enemy_id != EnemyId::TheGuardian
        || c.entities.potions.iter().any(Option::is_some)
        || c.entities.player.max_hp != 75
        || c.zones.hand.len() > 10
        || matches!(&p.engine, EngineState::PendingChoice(choice) if !matches!(
            choice,
            PendingChoice::HandSelect {
                reason: HandSelectReason::Exhaust | HandSelectReason::Upgrade, ..
            } | PendingChoice::GridSelect {
                source_pile: PileType::Discard,
                reason: GridSelectReason::MoveToDrawPile, ..
            }
        ))
        || c.entities.power_db.iter().any(|(owner, powers)| {
            powers.iter().any(|p| {
                if *owner == c.entities.player.id {
                    !PLAYER_POWERS.contains(&p.power_type)
                } else {
                    !ENEMY_POWERS.contains(&p.power_type)
                }
            })
        })
        || c.zones
            .hand
            .iter()
            .chain(c.zones.draw_pile.iter())
            .chain(c.zones.discard_pile.iter())
            .chain(c.zones.exhaust_pile.iter())
            .chain(c.zones.limbo.iter())
            .any(|card| !CARDS.contains(&card.id))
        || c.entities.player.relics.iter().any(|r| {
            !matches!(
                r.id,
                RelicId::BurningBlood | RelicId::Vajra | RelicId::BagOfPreparation
            )
        })
    {
        return Err("resource evaluator requires its declared A20 Guardian/75 HP/card/relic domain and empty potion slots".into());
    }
    Ok(())
}

struct Features {
    names: Vec<String>,
    values: Vec<f32>,
    named: bool,
}
impl Features {
    fn add(&mut self, name: impl FnOnce() -> String, value: f32) {
        if self.named {
            self.names.push(name());
        }
        self.values.push(value);
    }
}

fn encode(state: &CombatSearchState, named: bool) -> Features {
    let c = &state.position().combat;
    let m = &c.entities.monsters[0];
    // These are facts about the unfinished effect, not recommendations. Card
    // zones alone cannot say what a pending selection does or which instances
    // the engine allows it to affect. Validation declares the supported kinds.
    let selection = match &state.position().engine {
        EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        }) => Some((
            SelectionReason::from(*reason),
            PileType::Hand,
            *min_cards,
            *max_cards,
            *can_cancel,
            candidate_lineages.as_slice(),
        )),
        EngineState::PendingChoice(PendingChoice::GridSelect {
            source_pile,
            candidate_lineages,
            min_cards,
            max_cards,
            can_cancel,
            reason,
        }) => Some((
            SelectionReason::from(*reason),
            *source_pile,
            *min_cards,
            *max_cards,
            *can_cancel,
            candidate_lineages.as_slice(),
        )),
        _ => None,
    };
    let mut f = Features {
        names: vec![],
        values: Vec::with_capacity(512),
        named,
    };
    for (name, value) in [
        ("hp", c.entities.player.current_hp as f32 / 75.),
        ("block", c.entities.player.block as f32 / 50.),
        ("energy", c.turn.energy as f32 / 3.),
        ("turn", c.turn.turn_count as f32 / 20.),
        (
            "pending",
            f32::from(!matches!(
                state.position().engine,
                sts_core::state::core::EngineState::CombatPlayerTurn
            )),
        ),
        ("assembling", f32::from(state.selected_members().is_some())),
        (
            "selected",
            state.selected_members().map_or(0, <[_]>::len) as f32 / 10.,
        ),
    ] {
        f.add(|| name.into(), value);
    }
    f.add(
        || "terminal_hp_ceiling".into(),
        ((c.entities.player.current_hp
            + if c.entities.player.has_relic(RelicId::BurningBlood) {
                6
            } else {
                0
            })
        .min(75)
        .max(0)) as f32
            / 75.,
    );
    for &id in CARDS {
        let mut count = 0.;
        if let (Some(selected), Some(members)) =
            (state.selected_members(), state.selection_members())
        {
            for &i in selected {
                let lineage = match members[i] {
                    crate::SelectionMember::Card(l)
                    | crate::SelectionMember::Scry { lineage: l, .. } => l,
                };
                if c.zones
                    .hand
                    .iter()
                    .chain(c.zones.draw_pile.iter())
                    .chain(c.zones.discard_pile.iter())
                    .chain(c.zones.exhaust_pile.iter())
                    .any(|card| card.lineage == lineage && card.id == id)
                {
                    count += 0.1;
                }
            }
        }
        f.add(|| format!("selected/{id:?}"), count);
    }
    for &power in PLAYER_POWERS {
        f.add(
            || format!("player/{power:?}"),
            c.get_power(c.entities.player.id, power) as f32 / 10.,
        );
    }
    let incoming = project_monster_move_preview_in_combat(c, m);
    for (name, value) in [
        ("enemy/hp", m.current_hp as f32 / 250.),
        ("enemy/block", m.block as f32 / 50.),
        (
            "enemy/incoming",
            incoming.total_damage.unwrap_or(0) as f32 / 50.,
        ),
        ("guardian/open", f32::from(m.guardian.is_open)),
        (
            "guardian/threshold",
            m.guardian.damage_threshold as f32 / 50.,
        ),
        (
            "guardian/damage_taken",
            m.guardian.damage_taken as f32 / 50.,
        ),
        (
            "guardian/remaining",
            (m.guardian.damage_threshold - m.guardian.damage_taken) as f32 / 50.,
        ),
        (
            "guardian/triggered",
            f32::from(m.guardian.close_up_triggered),
        ),
    ] {
        f.add(|| name.into(), value);
    }
    for move_id in 0..8 {
        f.add(
            || format!("enemy/move/{move_id}"),
            f32::from(m.planned_move_id() == move_id),
        );
    }
    for &power in ENEMY_POWERS {
        f.add(
            || format!("enemy/{power:?}"),
            c.get_power(m.id, power) as f32 / 10.,
        );
    }
    for (zone, cards) in [
        c.zones.hand.iter().collect::<Vec<_>>(),
        c.zones.draw_pile.iter().collect(),
        c.zones.discard_pile.iter().collect(),
        c.zones.exhaust_pile.iter().collect(),
        c.zones.limbo.iter().collect(),
    ]
    .into_iter()
    .enumerate()
    {
        // Identity counts preserve the supported card's special effect; moments
        // and top-draw rows expose order without treating this as exact identity.
        for &id in CARDS {
            let mut stats = [0.; 3];
            for (i, card) in cards.iter().enumerate().filter(|(_, card)| card.id == id) {
                stats[0] += 0.2;
                stats[1] += card.upgrades as f32 / 5.;
                stats[2] += (i + 1) as f32 / 160.;
            }
            for (j, x) in stats.into_iter().enumerate() {
                f.add(|| format!("zone/{zone}/{id:?}/{j}"), x);
            }
        }
        let mut types = [0.; 5];
        let mut totals = [0.; 4];
        for card in &cards {
            let def = get_card_definition(card.id);
            types[def.card_type as usize] += 0.1;
            let evaluated = evaluate_card_for_play(card, c, Some(m.id));
            totals[0] += evaluated.base_damage_mut as f32 / 100.;
            totals[1] += evaluated.base_block_mut as f32 / 100.;
            totals[2] += evaluated.cost_for_turn_java().max(0) as f32 / 20.;
            // This is a trigger relation, not a recommendation to attack or block.
            if typed_profile(PowerId::SharpHide)
                .unwrap()
                .triggered_by_card_type(def.card_type)
            {
                totals[3] += c.get_power(m.id, PowerId::SharpHide).max(0) as f32 / 50.;
            }
        }
        for (i, x) in types.into_iter().enumerate() {
            f.add(|| format!("zone/{zone}/type/{i}"), x);
        }
        for (i, x) in totals.into_iter().enumerate() {
            f.add(|| format!("zone/{zone}/numbers/{i}"), x);
        }
    }
    // Actual simulator draw order is Oracle information. This is not a No-SL view.
    for i in 0..5 {
        for &id in CARDS {
            f.add(
                || format!("draw/{i}/{id:?}"),
                f32::from(c.zones.draw_pile.get(i).is_some_and(|c| c.id == id)),
            );
        }
    }
    for reason in [
        SelectionReason::Exhaust,
        SelectionReason::Upgrade,
        SelectionReason::MoveToDrawPile,
    ] {
        f.add(
            || format!("selection/reason/{reason:?}"),
            f32::from(selection.is_some_and(|s| s.0 == reason)),
        );
    }
    for pile in [PileType::Hand, PileType::Discard] {
        f.add(
            || format!("selection/pile/{pile:?}"),
            f32::from(selection.is_some_and(|s| s.1 == pile)),
        );
    }
    for (name, value) in [
        ("selection/min", selection.map_or(0., |s| s.2 as f32 / 10.)),
        ("selection/max", selection.map_or(0., |s| s.3 as f32 / 10.)),
        (
            "selection/cancel",
            f32::from(selection.is_some_and(|s| s.4)),
        ),
        (
            "selection/candidates",
            selection.map_or(0., |s| s.5.len() as f32 / 10.),
        ),
    ] {
        f.add(|| name.into(), value);
    }
    for &id in CARDS {
        let mut stats = [0.; 2];
        for card in c.zones.hand.iter().chain(c.zones.discard_pile.iter()) {
            if card.id == id && selection.is_some_and(|s| s.5.contains(&card.lineage)) {
                stats[0] += 0.1;
                stats[1] += card.upgrades as f32 / 10.;
            }
        }
        for (i, value) in stats.into_iter().enumerate() {
            f.add(|| format!("selection/candidate/{id:?}/{i}"), value);
        }
    }
    // Keep instance costs and effects attached to the same card. These are
    // requests/previews, not guaranteed damage, draw or a recommendation. The
    // original context still carries order; pooling tokens never merges states.
    for i in 0..10 {
        let card = c.zones.hand.get(i);
        let mut numbers = [0.; 18];
        if let Some(card) = card {
            let evaluated = evaluate_card_for_play(card, c, Some(m.id));
            let hits = match card_base_damage_delivery_v1(card.id, card.upgrades) {
                Some(CardBaseDamageDeliveryV1::Attack {
                    hits: CardAttackHitsV1::Fixed { count },
                    ..
                }) => count as f32,
                _ => 0.,
            };
            let block = card_base_block_delivery_v1(card.id)
                .is_some_and(|rule| rule.has_unconditional_base_request());
            let cost = card.get_cost() as f32;
            numbers = [
                1.,
                f32::from(
                    state.selected_members().is_none()
                        && matches!(
                            state.position().engine,
                            sts_core::state::core::EngineState::CombatPlayerTurn
                        )
                        && can_play_card(card, c).is_ok(),
                ),
                cost / 3.,
                (c.turn.energy as f32 - cost.max(0.)) / 3.,
                if hits > 0. {
                    evaluated.base_damage_mut as f32 / 50.
                } else {
                    0.
                },
                hits / 5.,
                if block {
                    evaluated.base_block_mut as f32 / 50.
                } else {
                    0.
                },
                unconditional_card_draw_request_v1(card.id, evaluated.base_magic_num_mut)
                    .unwrap_or(0) as f32
                    / 5.,
                evaluated.base_magic_num_mut as f32 / 10.,
                f32::from(
                    sts_core::content::cards::mechanics::card_exhausts_when_played(
                        card.id,
                        card.upgrades,
                    ),
                ),
                f32::from(sts_core::content::cards::mechanics::card_is_ethereal(
                    card.id,
                    card.upgrades,
                )),
                card.upgrades as f32,
                if typed_profile(PowerId::SharpHide)
                    .unwrap()
                    .triggered_by_card_type(get_card_definition(card.id).card_type)
                {
                    c.get_power(m.id, PowerId::SharpHide) as f32 / 50.
                } else {
                    0.
                },
                (m.guardian.damage_threshold - m.guardian.damage_taken) as f32 / 50.,
                m.block as f32 / 50.,
                incoming.total_damage.unwrap_or(0) as f32 / 50.,
                c.entities.player.block as f32 / 50.,
                c.get_power(c.entities.player.id, PowerId::Strength) as f32 / 10.,
            ];
        }
        for (name, value) in [
            "present",
            "playable",
            "cost",
            "energy_after_cost",
            "damage_per_hit",
            "hits",
            "block",
            "draw",
            "magic",
            "exhaust",
            "ethereal",
            "upgrades",
            "sharp_hide",
            "mode_remaining",
            "enemy_block",
            "incoming",
            "player_block",
            "strength",
        ]
        .into_iter()
        .zip(numbers)
        {
            f.add(|| format!("hand/{i}/{name}"), value);
        }
        for &id in CARDS {
            f.add(
                || format!("hand/{i}/card/{id:?}"),
                f32::from(card.is_some_and(|c| c.id == id)),
            );
        }
        f.add(
            || format!("hand/{i}/selection_eligible"),
            f32::from(
                card.is_some_and(|card| selection.is_some_and(|s| s.5.contains(&card.lineage))),
            ),
        );
    }
    f
}

pub fn features(state: &CombatSearchState) -> Vec<f32> {
    encode(state, false).values
}
pub fn feature_names(state: &CombatSearchState) -> Vec<String> {
    encode(state, true).names
}

/// A shared learned transform for each hand instance, followed by sum/10 and
/// maximum pooling. The full non-hand context remains separate. These vectors
/// are only value inputs, never exact-state keys or action equivalences.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HandEncoder {
    pub start: usize,
    pub width: usize,
    pub weights: Vec<Vec<f32>>,
    pub bias: Vec<f32>,
}
impl HandEncoder {
    fn project(&self, x: &[f32]) -> Vec<f32> {
        let h = self.bias.len();
        let mut result = x[..self.start].to_vec();
        result.resize(self.start + 2 * h, 0.);
        for card in x[self.start..].chunks_exact(self.width) {
            if card[0] == 0. {
                continue;
            }
            for (j, (weights, bias)) in self.weights.iter().zip(&self.bias).enumerate() {
                let value = weights
                    .iter()
                    .zip(card)
                    .fold(*bias, |v, (w, x)| v + w * x)
                    .max(0.);
                result[self.start + j] += value / 10.;
                result[self.start + h + j] = result[self.start + h + j].max(value);
            }
        }
        result
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCurveHead {
    pub slope_weights: Vec<f32>,
    pub slope_bias: f32,
}

/// Offline readings on actual materialized-input boundaries. A witnessed
/// suffix is not the model's declared executor and supplies no training label.
#[derive(Clone, Debug, Serialize)]
pub struct PlanPredictionFrame {
    pub after_inputs: usize,
    pub features: Vec<f32>,
    pub predicted_return: Option<f32>,
    pub actual_terminal_return: Option<f32>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlanPredictionTrace {
    pub objective: String,
    pub feature_names: Vec<String>,
    pub frames: Vec<PlanPredictionFrame>,
    pub replay_inputs: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValueModel {
    pub schema: u32,
    pub objective: String,
    pub feature_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand_encoder: Option<HandEncoder>,
    pub hidden_weights: Vec<Vec<f32>>,
    pub hidden_bias: Vec<f32>,
    pub output_weights: Vec<f32>,
    pub output_bias: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_curve: Option<ResourceCurveHead>,
}
impl ValueModel {
    /// Replay a supplied concrete plan without searching or changing its source.
    /// Known terminal results are recorded as facts, never network predictions.
    /// Cost belongs to offline diagnosis, separately from combat search work.
    pub fn prediction_trace(
        &self,
        root: &CombatPosition,
        inputs: &[sts_core::state::core::ClientInput],
    ) -> Result<PlanPredictionTrace, String> {
        use sts_core::sim::combat::{CombatStepper, CombatTerminal, EngineCombatStepper};
        let source = CombatSearchState::new(root.clone())?;
        self.validate(&source)?;
        let positions = crate::shuffle_order::replay(root, inputs)?;
        let mut frames = Vec::with_capacity(positions.len());
        for (after_inputs, position) in positions.into_iter().enumerate() {
            let state = CombatSearchState::new(position)?;
            self.validate(&state)?;
            let terminal = EngineCombatStepper.terminal(state.position());
            let actual_terminal_return = match terminal {
                CombatTerminal::Win if EngineCombatStepper.is_exact_victory(state.position()) => {
                    Some(state.position().combat.entities.player.current_hp as f32 / 75.)
                }
                CombatTerminal::Loss => Some(-1.),
                CombatTerminal::Unresolved => None,
                CombatTerminal::Win => {
                    return Err("prediction trace requires exact non-escape results".into())
                }
            };
            let features = features(&state);
            let predicted_return = actual_terminal_return
                .is_none()
                .then(|| self.predict_features(&features));
            frames.push(PlanPredictionFrame {
                after_inputs,
                features,
                predicted_return,
                actual_terminal_return,
            });
        }
        Ok(PlanPredictionTrace {
            objective: self.objective.clone(),
            feature_names: feature_names(&source),
            frames,
            replay_inputs: inputs.len(),
        })
    }

    pub fn validate(&self, state: &CombatSearchState) -> Result<(), String> {
        validate_position(state.position())?;
        let h = self.hidden_bias.len();
        let input_width = if let Some(encoder) = &self.hand_encoder {
            let start = feature_names(state)
                .iter()
                .position(|n| n == "hand/0/present")
                .unwrap();
            let width = (self.feature_names.len().saturating_sub(start)) / 10;
            if encoder.start != start
                || encoder.width != width
                || width == 0
                || self.feature_names.len() != start + 10 * width
                || !(1..=64).contains(&encoder.bias.len())
                || encoder.weights.len() != encoder.bias.len()
                || encoder.weights.iter().any(|w| w.len() != width)
                || encoder
                    .weights
                    .iter()
                    .flatten()
                    .chain(&encoder.bias)
                    .any(|v| !v.is_finite() || v.abs() > 1e6)
            {
                return Err("invalid resource hand encoder layout or weights".into());
            }
            start + 2 * encoder.bias.len()
        } else {
            self.feature_names.len()
        };
        if self.schema != SCHEMA
            || !matches!(
                self.objective.as_str(),
                OBJECTIVE
                    | CONTINUATION_OBJECTIVE
                    | SEARCH_CONTINUATION_OBJECTIVE
                    | WITNESS_CONTINUATION_OBJECTIVE
                    | RESOURCE_CURVE_OBJECTIVE
            )
            || (self.objective == RESOURCE_CURVE_OBJECTIVE) != self.resource_curve.is_some()
            || self.resource_curve.as_ref().is_some_and(|head| {
                head.slope_weights.len() != h
                    || head
                        .slope_weights
                        .iter()
                        .chain(std::iter::once(&head.slope_bias))
                        .any(|v| !v.is_finite() || v.abs() > 1e6)
            })
            || self.feature_names != feature_names(state)
            || !(1..=128).contains(&h)
            || self.hidden_weights.len() != h
            || self.output_weights.len() != h
            || self.hidden_weights.iter().any(|w| w.len() != input_width)
            || self
                .hidden_weights
                .iter()
                .flatten()
                .chain(&self.hidden_bias)
                .chain(&self.output_weights)
                .chain(std::iter::once(&self.output_bias))
                .any(|x| !x.is_finite() || x.abs() > 1e6)
        {
            return Err("invalid resource model schema, shape, vocabulary or weights".into());
        }
        Ok(())
    }
    fn components(&self, x: &[f32]) -> (f32, Option<f32>, f32) {
        let projected;
        let x = if let Some(encoder) = &self.hand_encoder {
            projected = encoder.project(x);
            &projected
        } else {
            x
        };
        let nonzero: Vec<_> = x
            .iter()
            .enumerate()
            .filter(|(_, value)| **value != 0.)
            .collect();
        let mut y = self.output_bias;
        let mut slope = self.resource_curve.as_ref().map(|head| head.slope_bias);
        for (j, ((w, bias), out)) in self
            .hidden_weights
            .iter()
            .zip(&self.hidden_bias)
            .zip(&self.output_weights)
            .enumerate()
        {
            let z = nonzero
                .iter()
                .fold(*bias, |s, (i, x)| s + w[*i] * **x)
                .max(0.);
            y += z * out;
            if let (Some(value), Some(head)) = (&mut slope, &self.resource_curve) {
                *value += z * head.slope_weights[j];
            }
        }
        (y, slope, x[7])
    }

    /// P(the training executor wins and retains this integer HP goal).
    /// No such probability can be extracted from legacy scalar regression.
    pub fn resource_goal_probability_features(&self, x: &[f32], hp_goal: u32) -> Option<f32> {
        let (a, raw_b, ceiling) = self.components(x);
        let b = softplus(raw_b? as f64);
        if hp_goal as f64 > (ceiling as f64 * 75.).round() {
            return Some(0.);
        }
        Some(sigmoid(a as f64 - b * hp_goal as f64 / 75.) as f32)
    }

    /// Already observed terminal facts bypass prediction. Feature-only callers
    /// are responsible for supplying a nonterminal continuation decision.
    pub fn resource_goal_probability(
        &self,
        state: &CombatSearchState,
        hp_goal: u32,
    ) -> Option<f32> {
        use sts_core::sim::combat::{CombatStepper, CombatTerminal, EngineCombatStepper};
        self.resource_curve.as_ref()?;
        let p = state.position();
        if p.combat.runtime.combat_smoked {
            return None;
        }
        match EngineCombatStepper.terminal(p) {
            CombatTerminal::Win if EngineCombatStepper.is_exact_victory(p) => Some(f32::from(
                hp_goal as i64 <= p.combat.entities.player.current_hp as i64,
            )),
            CombatTerminal::Win => None,
            CombatTerminal::Loss => Some(0.),
            CombatTerminal::Unresolved => {
                self.resource_goal_probability_features(&features(state), hp_goal)
            }
        }
    }

    pub fn predict_features(&self, x: &[f32]) -> f32 {
        let (y, slope, ceiling) = self.components(x);
        if let Some(raw_b) = slope {
            return resource_curve_return(y as f64, softplus(raw_b as f64), ceiling as f64) as f32;
        }
        // This scoped vocabulary has no combat healing; Burning Blood is the
        // only permitted post-combat heal. This ceiling is a mechanism fact.
        y.clamp(-1., ceiling)
    }
    pub fn predict(&self, state: &CombatSearchState) -> f32 {
        self.predict_features(&features(state))
    }
}

fn sigmoid(x: f64) -> f64 {
    if x >= 0. {
        1. / (1. + (-x).exp())
    } else {
        let e = x.exp();
        e / (1. + e)
    }
}
fn softplus(x: f64) -> f64 {
    x.max(0.) + (-x.abs()).exp().ln_1p()
}
fn resource_curve_return(a: f64, b: f64, ceiling: f64) -> f64 {
    let d = b * ceiling;
    let area = if d < 1e-6 {
        ceiling * sigmoid(a - d / 2.)
    } else if a - d > 0. {
        (d + (-a).exp().ln_1p() - (-(a - d)).exp().ln_1p()) / b
    } else {
        (softplus(a) - softplus(a - d)) / b
    };
    (sigmoid(a) - 1. + area).clamp(-1., ceiling)
}

#[cfg(test)]
mod tests;
