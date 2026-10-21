//! Extract action-conditioned, jointly observed turn consequences from frozen
//! continuations. No search, max backup, synthetic targets or new game rules.
use clap::Parser;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    time::Instant,
};
use sts_core::{
    content::powers::PowerId,
    runtime::combat::{CardLineageId, CombatCard},
    sim::combat::{CombatPosition, CombatStepper, CombatTerminal, EngineCombatStepper},
    state::core::{ClientInput, EngineState},
};
use sts_guided_search::{
    local::ConstructedCombat, resource_value as value, shuffle_order, CombatSearchState,
};

#[derive(Parser)]
struct Args {
    #[arg(long, num_args = 1..)]
    input: Vec<PathBuf>,
    #[arg(long)]
    output: PathBuf,
    /// Offline collection cap, not a gameplay threshold. Selection ignores targets.
    #[arg(long, default_value_t = 128)]
    max_rows_per_case: usize,
}

#[derive(Deserialize, Serialize)]
struct Case {
    label: String,
    group: String,
    spec: ConstructedCombat,
}
#[derive(Deserialize)]
struct Control {
    inputs: Vec<ClientInput>,
    endpoint: CombatPosition,
}
#[derive(Deserialize)]
struct Trace {
    prefix: Vec<ClientInput>,
    inputs: Vec<ClientInput>,
    endpoint: CombatPosition,
    tactical_control: Option<Control>,
}
#[derive(Deserialize)]
struct Document {
    schema: u32,
    feature_schema: u32,
    objective: String,
    information: String,
    case: Case,
    root: CombatPosition,
    traces: Vec<Trace>,
}
#[derive(Serialize)]
struct Row {
    source_sha256: String,
    endpoint_sha256: String,
    sample_sha256: String,
    trace: usize,
    executor: &'static str,
    source_offset: usize,
    source_turn: u32,
    horizon: u32,
    reached_turn_offset: u32,
    features: Vec<f32>,
    inputs: Vec<ClientInput>,
    plan: Vec<Vec<f32>>,
    targets: Vec<f32>,
}

fn digest<T: Serialize>(x: &T) -> Result<String, serde_json::Error> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(x)?)))
}

fn source_cards(p: &CombatPosition) -> Vec<(&CombatCard, usize, usize)> {
    [
        p.combat.zones.hand.iter().collect::<Vec<_>>(),
        p.combat.zones.draw_pile.iter().collect(),
        p.combat.zones.discard_pile.iter().collect(),
        p.combat.zones.exhaust_pile.iter().collect(),
        p.combat.zones.limbo.iter().collect(),
    ]
    .into_iter()
    .enumerate()
    .flat_map(|(zone, cards)| {
        cards
            .into_iter()
            .enumerate()
            .map(move |(index, c)| (c, zone, index))
    })
    .collect()
}

fn token_names() -> Vec<String> {
    let mut names = [
        "play",
        "selection",
        "selection_member",
        "end_turn",
        "enemy_target",
        "scope_hand",
        "scope_deck",
        "scope_grid",
        "source_upgrades",
        "source_cost",
    ]
    .into_iter()
    .map(String::from)
    .collect::<Vec<_>>();
    names.extend((0..5).map(|i| format!("source_zone/{i}")));
    names.push("source_position".into());
    names.extend(value::CARDS.iter().map(|id| format!("card/{id:?}")));
    names
}

/// Bind only identity from executed pre-action states. All numerical card inputs
/// are frozen at the source: later upgrades, damage, costs and HP never enter.
/// Generated cards absent at the source are an explicit coverage exclusion.
fn plan_tokens(
    source: &CombatPosition,
    states: &[CombatPosition],
    inputs: &[ClientInput],
) -> Result<Option<Vec<Vec<f32>>>, String> {
    let cards = source_cards(source);
    let mut tokens = vec![];
    let card_token = |lineage: CardLineageId, mut t: Vec<f32>| -> Option<Vec<f32>> {
        let (c, zone, index) = cards.iter().find(|(c, _, _)| c.lineage == lineage)?;
        t[8] = c.upgrades as f32 / 3.;
        t[9] = c.cost_for_turn_java() as f32 / 3.;
        t[10 + zone] = 1.;
        t[15] = *index as f32 / 32.;
        t[16 + value::CARDS.iter().position(|id| *id == c.id)?] = 1.;
        Some(t)
    };
    for (before, input) in states.iter().zip(inputs) {
        let mut t = vec![0.; token_names().len()];
        match input {
            ClientInput::PlayCard { card_index, target } => {
                let c = before
                    .combat
                    .zones
                    .hand
                    .get(*card_index)
                    .ok_or("missing played card")?;
                t[0] = 1.;
                if let Some(target) = target {
                    if *target != source.combat.entities.monsters[0].id {
                        return Err("pilot requires its exact Guardian target".into());
                    }
                    t[4] = 1.;
                }
                let Some(t) = card_token(c.lineage, t) else {
                    return Ok(None);
                };
                tokens.push(t);
            }
            ClientInput::SubmitSelection(r) => {
                t[1] = 1.;
                t[5 + match r.scope {
                    sts_core::state::selection::SelectionScope::Hand => 0,
                    sts_core::state::selection::SelectionScope::Deck => 1,
                    sts_core::state::selection::SelectionScope::Grid => 2,
                }] = 1.;
                tokens.push(t.clone());
                for lineage in r.selected_card_lineages() {
                    t[1] = 0.;
                    t[2] = 1.;
                    let Some(member) = card_token(lineage, t.clone()) else {
                        return Ok(None);
                    };
                    tokens.push(member);
                }
            }
            ClientInput::EndTurn => {
                t[3] = 1.;
                tokens.push(t);
            }
            _ => return Err(format!("unrepresented plan action: {}", input.kind_v1())),
        }
    }
    Ok(Some(tokens))
}

fn target_spec() -> (Vec<String>, Vec<f32>) {
    let mut names = vec![
        "hp_delta".into(),
        "strength_delta".into(),
        "enemy_hp_delta".into(),
    ];
    let mut scales = vec![75., 10., 250.];
    names.extend(
        value::CARDS
            .iter()
            .map(|id| format!("exhaust_delta/{id:?}")),
    );
    scales.extend(vec![1.; value::CARDS.len()]);
    names.extend(["victory".into(), "loss".into()]);
    scales.extend([1., 1.]);
    (names, scales)
}

fn joint_targets(source: &CombatPosition, end: &CombatPosition) -> Vec<f32> {
    let a = &source.combat;
    let b = &end.combat;
    let mut result = vec![
        (b.entities.player.current_hp - a.entities.player.current_hp) as f32 / 75.,
        (b.get_power(b.entities.player.id, PowerId::Strength)
            - a.get_power(a.entities.player.id, PowerId::Strength)) as f32
            / 10.,
        (b.entities.monsters[0].current_hp - a.entities.monsters[0].current_hp) as f32 / 250.,
    ];
    for id in value::CARDS {
        result.push(
            b.zones.exhaust_pile.iter().filter(|c| c.id == *id).count() as f32
                - a.zones.exhaust_pile.iter().filter(|c| c.id == *id).count() as f32,
        );
    }
    let terminal = EngineCombatStepper.terminal(end);
    result.extend([
        f32::from(terminal == CombatTerminal::Win),
        f32::from(terminal == CombatTerminal::Loss),
    ]);
    result
}

fn endpoint(states: &[CombatPosition], offset: usize, horizon: u32) -> Option<usize> {
    let turn = states[offset].combat.turn.turn_count;
    ((offset + 1)..states.len()).find(|&i| {
        EngineCombatStepper.terminal(&states[i]) != CombatTerminal::Unresolved
            || (states[i].engine == EngineState::CombatPlayerTurn
                && states[i].combat.turn.turn_count >= turn + horizon)
    })
}

/// Preserve available within-source alternatives under the case cap. Both
/// family order and member order use provenance only, never observed outcomes.
fn retain_families(rows: Vec<Row>, cap: usize) -> Vec<Row> {
    let mut families = BTreeMap::<(String, u32), Vec<Row>>::new();
    for row in rows {
        families
            .entry((row.source_sha256.clone(), row.horizon))
            .or_default()
            .push(row);
    }
    let mut families = families.into_iter().collect::<Vec<_>>();
    families.sort_by(|(ak, a), (bk, b)| (a.len() < 2, ak).cmp(&(b.len() < 2, bk)));
    let mut selected = vec![];
    for (_, mut family) in families {
        family.sort_by(|a, b| a.sample_sha256.cmp(&b.sample_sha256));
        selected.extend(family.into_iter().take(cap - selected.len()));
        if selected.len() == cap {
            break;
        }
    }
    selected
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.output.exists() || args.max_rows_per_case == 0 {
        return Err("fresh output and positive cap required".into());
    }
    let producer = format!("{:x}", Sha256::digest(fs::read(std::env::current_exe()?)?));
    fs::create_dir_all(&args.output)?;
    for (case_index, path) in args.input.iter().enumerate() {
        let started = Instant::now();
        let bytes = fs::read(path)?;
        let d: Document = serde_json::from_slice(&bytes)?;
        if d.schema != 1
            || d.feature_schema != value::SCHEMA
            || d.information != "full_state_oracle"
            || d.objective != value::SEARCH_CONTINUATION_OBJECTIVE
            || d.case.group.trim().is_empty()
            || d.case.spec.build()? != d.root
        {
            return Err("inconsistent frozen continuation source".into());
        }
        value::validate_position(&d.root)?;
        let feature_names = value::feature_names(&CombatSearchState::new(d.root.clone())?);
        let mut seen = BTreeSet::new();
        let mut rows = vec![];
        let mut censored = 0;
        let mut generated = 0;
        let mut replay_inputs = 0;
        for (trace_index, trace) in d.traces.iter().enumerate() {
            let mut variants = vec![("adopted", &trace.inputs, &trace.endpoint)];
            if let Some(control) = &trace.tactical_control {
                variants.push(("tactical_control", &control.inputs, &control.endpoint));
            }
            for (executor, suffix, recorded_endpoint) in variants {
                let inputs = trace
                    .prefix
                    .iter()
                    .chain(suffix.iter())
                    .cloned()
                    .collect::<Vec<_>>();
                let states = shuffle_order::replay(&d.root, &inputs)?;
                replay_inputs += inputs.len();
                if states.last() != Some(recorded_endpoint) {
                    return Err("frozen continuation replay mismatch".into());
                }
                for offset in trace.prefix.len()..inputs.len() {
                    let source = &states[offset];
                    if source.engine != EngineState::CombatPlayerTurn
                        || EngineCombatStepper.terminal(source) != CombatTerminal::Unresolved
                    {
                        continue;
                    }
                    for horizon in [1, 2] {
                        let Some(end) = endpoint(&states, offset, horizon) else {
                            censored += 1;
                            continue;
                        };
                        let plan_inputs = inputs[offset..end].to_vec();
                        let sample_sha256 = digest(&(source, &plan_inputs, horizon))?;
                        if !seen.insert(sample_sha256.clone()) {
                            continue;
                        }
                        let Some(plan) = plan_tokens(source, &states[offset..=end], &plan_inputs)?
                        else {
                            generated += 1;
                            continue;
                        };
                        rows.push(Row {
                            source_sha256: digest(source)?,
                            endpoint_sha256: digest(&states[end])?,
                            sample_sha256,
                            trace: trace_index,
                            executor,
                            source_offset: offset,
                            source_turn: source.combat.turn.turn_count + 1,
                            horizon,
                            reached_turn_offset: states[end].combat.turn.turn_count
                                - source.combat.turn.turn_count,
                            features: value::features(&CombatSearchState::new(source.clone())?),
                            inputs: plan_inputs,
                            plan,
                            targets: joint_targets(source, &states[end]),
                        });
                    }
                }
            }
        }
        let eligible = rows.len();
        let rows = retain_families(rows, args.max_rows_per_case);
        // Independently replay every retained local plan. Timing is the exact
        // simulator baseline, without feature encoding or model inference.
        let mut source_reconstruction_ms = 0.;
        let mut exact_replay_ms = 0.;
        for row in &rows {
            let trace = &d.traces[row.trace];
            let suffix = if row.executor == "adopted" {
                &trace.inputs
            } else {
                &trace.tactical_control.as_ref().unwrap().inputs
            };
            let full = trace
                .prefix
                .iter()
                .chain(suffix.iter())
                .cloned()
                .collect::<Vec<_>>();
            let now = Instant::now();
            let source = shuffle_order::replay(&d.root, &full[..row.source_offset])?
                .pop()
                .unwrap();
            source_reconstruction_ms += now.elapsed().as_secs_f64() * 1000.;
            if digest(&source)? != row.source_sha256 {
                return Err("local source mismatch".into());
            }
            let now = Instant::now();
            let end = shuffle_order::replay(&source, &row.inputs)?.pop().unwrap();
            exact_replay_ms += now.elapsed().as_secs_f64() * 1000.;
            replay_inputs += row.source_offset + row.inputs.len();
            if digest(&end)? != row.endpoint_sha256 || joint_targets(&source, &end) != row.targets {
                return Err("joint consequence replay mismatch".into());
            }
        }
        let (target_names, target_scales) = target_spec();
        let output = serde_json::json!({"schema":1, "objective":"guardian_concrete_plan_joint_consequences_v1",
            "information":"full_state_oracle_concrete_plan", "feature_schema":value::SCHEMA,
            "feature_names":feature_names, "token_names":token_names(), "target_names":target_names, "target_scales":target_scales,
            "case":d.case, "source_file":path, "source_sha256":format!("{:x}", Sha256::digest(bytes)),
            "producer_sha256":producer, "rows":rows,
            "collection":{"eligible_rows":eligible,"retained_rows":rows.len(),"censored_windows":censored,
                "excluded_generated_card_windows":generated,"replay_inputs":replay_inputs,
                "source_reconstruction_ms":source_reconstruction_ms,"exact_local_replay_ms":exact_replay_ms,
                "sampling":"multiple-plan source families first, then provenance order; targets unused",
                "elapsed_ms":started.elapsed().as_secs_f64()*1000.,"max_rows_per_case":args.max_rows_per_case}});
        fs::write(
            args.output.join(format!("case-{case_index:03}.json")),
            serde_json::to_vec(&output)?,
        )?;
        println!(
            "{} rows={}/{} exact_local_replay_ms={:.2}",
            output["case"]["label"],
            rows.len(),
            eligible,
            exact_replay_ms
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::{
        content::cards::CardId,
        state::selection::{SelectionResolution, SelectionScope},
    };
    fn root() -> CombatPosition {
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.monsters = vec![sts_core::test_support::planned_monster(
            sts_core::content::monsters::EnemyId::TheGuardian,
            1,
        )];
        c.zones.hand = vec![
            CombatCard::for_scenario(CardId::Inflame, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        CombatPosition::new(EngineState::CombatPlayerTurn, c)
    }
    #[test]
    fn tokens_keep_order_and_source_values_even_after_upgrades() {
        let source = root();
        let mut later = source.clone();
        later.combat.zones.hand[0].upgrades = 1;
        later.combat.zones.hand[0].cost_for_turn = Some(0);
        let play = ClientInput::PlayCard {
            card_index: 0,
            target: None,
        };
        let end = ClientInput::EndTurn;
        let tokens = plan_tokens(
            &source,
            &[later.clone(), later.clone()],
            &[play.clone(), end.clone()],
        )
        .unwrap()
        .unwrap();
        assert_eq!(tokens[0][8], 0.);
        assert_eq!(tokens[0][9], 1. / 3.);
        assert_eq!(tokens[0][0], 1.);
        assert_eq!(tokens[1][3], 1.);
        let reversed = plan_tokens(&source, &[later.clone(), later], &[end, play])
            .unwrap()
            .unwrap();
        assert_ne!(tokens, reversed);
    }
    #[test]
    fn generated_choices_are_excluded_instead_of_guessing_source_card() {
        let source = root();
        let generated = CombatCard::for_scenario(CardId::Wound, 99);
        let choose = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
            SelectionScope::Hand,
            [generated.lineage],
        ));
        assert!(plan_tokens(&source, &[source.clone()], &[choose])
            .unwrap()
            .is_none());
    }
    #[test]
    fn targets_are_one_endpoint_and_pending_does_not_end_window() {
        let source = root();
        let mut end = source.clone();
        end.combat.entities.player.current_hp -= 7;
        end.combat
            .entities
            .power_db
            .entry(0)
            .or_default()
            .push(sts_core::runtime::combat::Power {
                power_type: PowerId::Strength,
                amount: 4,
                extra_data: 0,
                instance_id: None,
                payload: sts_core::runtime::combat::PowerPayload::None,
                just_applied: false,
            });
        end.combat
            .zones
            .exhaust_pile
            .push(CombatCard::for_scenario(CardId::Carnage, 9));
        let y = joint_targets(&source, &end);
        assert_eq!(&y[..2], &[-7. / 75., 0.4]);
        assert_eq!(
            y[3 + value::CARDS
                .iter()
                .position(|id| *id == CardId::Carnage)
                .unwrap()],
            1.
        );
        end.combat.turn.turn_count += 1;
        let mut pending = end.clone();
        pending.engine =
            EngineState::PendingChoice(sts_core::state::core::PendingChoice::HandSelect {
                candidate_lineages: vec![],
                min_cards: 0,
                max_cards: 0,
                can_cancel: true,
                reason: sts_core::state::core::HandSelectReason::Exhaust,
            });
        assert_eq!(endpoint(&[source, pending, end.clone()], 0, 1), Some(2));
        assert_eq!(endpoint(&[root(), end], 0, 2), None);
    }

    #[test]
    fn sampling_preserves_alternatives_without_sorting_on_targets() {
        let row = |source: &str, sample: &str, value: f32| Row {
            source_sha256: source.into(),
            endpoint_sha256: sample.into(),
            sample_sha256: sample.into(),
            trace: 0,
            executor: "adopted",
            source_offset: 0,
            source_turn: 1,
            horizon: 1,
            reached_turn_offset: 1,
            features: vec![],
            inputs: vec![],
            plan: vec![],
            targets: vec![value],
        };
        let selected = retain_families(
            vec![row("a", "a", 100.), row("b", "b1", -1.), row("b", "b2", 0.)],
            2,
        );
        assert_eq!(
            selected
                .iter()
                .map(|r| r.sample_sha256.as_str())
                .collect::<Vec<_>>(),
            ["b1", "b2"]
        );
    }
}
