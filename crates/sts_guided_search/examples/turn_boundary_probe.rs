//! Paired, constructed first-turn diagnostic. This is not a combat policy benchmark.
use clap::Parser;
use serde::Serialize;
use std::{fs, path::PathBuf};
use sts_combat_outcome::agent::{
    belief::{
        CombatBeliefEnvironmentV1, CombatPublicBoundaryV1, CombatPublicHistoryV1,
        IndependentStreamsCombatBeliefSamplerV1,
    },
    information::{
        action::{PublicCombatActionChoiceV1 as Choice, PublicCombatAtomicActionV1 as Atomic},
        run::{PublicCombatRunContextGapV1, PublicCombatRunContextV1},
        state::PublicCombatStateV1,
    },
};
use sts_core::{
    content::cards::CardId,
    runtime::combat::DrawPileChange,
    sim::combat::{CombatPosition, EngineCombatStepper},
};
use sts_guided_search::local::ConstructedCombat;

#[derive(Parser)]
struct Args {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 8)]
    particles: usize,
    /// Controlled intervention: enumerate physical orders of the current draw pile.
    #[arg(long)]
    all_draw_orders: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum Line {
    End,
    DefendEnd,
    ShrugEnd,
    ShrugSlamIfSeenEnd,
}

#[derive(Serialize)]
struct Row {
    candidate: usize,
    candidate_mass: f64,
    line: Line,
    first_draw: Option<CardId>,
    first_observation_hand: Vec<CardId>,
    reached_next_turn: bool,
    turn: u32,
    hp: i32,
    block: i32,
    energy: u8,
    enemy_hp: Vec<i32>,
    enemy_block: Vec<i32>,
    hand: Vec<CardId>,
    draw_composition: Vec<CardId>,
    discard_composition: Vec<CardId>,
    cards_played_this_combat: Vec<CardId>,
    end_trace_complete: bool,
    public_equal_to_end: bool,
    /// Diagnostic only. Private equality never selects an action.
    exact_equal_to_end: bool,
}

fn observation(boundary: &CombatPublicBoundaryV1) -> &PublicCombatStateV1 {
    match boundary {
        CombatPublicBoundaryV1::Decision { decision } => &decision.observation,
        CombatPublicBoundaryV1::Terminal { observation, .. } => observation,
    }
}

fn choose(history: &CombatPublicHistoryV1, card: Option<CardId>) -> Result<Choice, String> {
    let CombatPublicBoundaryV1::Decision { decision } = history.current() else {
        return Err("combat ended before the next requested action".into());
    };
    let ordinal = decision
        .actions
        .atomic_actions
        .iter()
        .position(|action| match (card, action) {
            (None, Atomic::EndTurn) => true,
            (Some(id), Atomic::PlayCard { hand_index, .. }) => {
                decision.observation.cards.hand.cards[*hand_index].card_id == id
            }
            _ => false,
        })
        .ok_or_else(|| format!("requested public action {card:?} is unavailable"))?;
    Ok(Choice::Atomic {
        action_ordinal: ordinal,
    })
}

fn step(
    history: &mut CombatPublicHistoryV1,
    position: &mut CombatPosition,
    action: Choice,
) -> Result<(), Box<dyn std::error::Error>> {
    let (next_history, next_position) =
        history.advance_exact(position, action, &EngineCombatStepper, 512)?;
    *history = next_history;
    *position = next_position;
    Ok(())
}

fn run_line(
    root: &CombatPublicHistoryV1,
    source: &CombatPosition,
    candidate: usize,
    candidate_mass: f64,
    line: Line,
) -> Result<(Row, CombatPublicBoundaryV1, CombatPosition), Box<dyn std::error::Error>> {
    let mut history = root.clone();
    let mut position = source.clone();
    let root_turn = observation(history.current()).turn.turn_count;

    match line {
        Line::End => {}
        Line::DefendEnd => {
            let action = choose(&history, Some(CardId::Defend))?;
            step(&mut history, &mut position, action)?;
        }
        Line::ShrugEnd | Line::ShrugSlamIfSeenEnd => {
            let action = choose(&history, Some(CardId::ShrugItOff))?;
            step(&mut history, &mut position, action)?;
        }
    }

    let first_observation_hand = observation(history.current())
        .cards
        .hand
        .cards
        .iter()
        .map(|card| card.card_id)
        .collect();
    let first_draw = history.entries().last().and_then(|entry| {
        entry
            .draw_pile_changes
            .iter()
            .find_map(|change| match change {
                DrawPileChange::TakeTop { card } => Some(card.card_id),
                _ => None,
            })
    });
    if matches!(line, Line::ShrugSlamIfSeenEnd) && first_draw == Some(CardId::BodySlam) {
        let action = choose(&history, Some(CardId::BodySlam))?;
        step(&mut history, &mut position, action)?;
    }
    let action = choose(&history, None)?;
    step(&mut history, &mut position, action)?;

    let state = observation(history.current());
    let reached_next_turn = state.turn.turn_count == root_turn + 1;
    if !reached_next_turn && !matches!(history.current(), CombatPublicBoundaryV1::Terminal { .. }) {
        return Err("line did not reach next player control or a terminal state".into());
    }
    let row = Row {
        candidate,
        candidate_mass,
        line,
        first_draw,
        first_observation_hand,
        reached_next_turn,
        turn: state.turn.turn_count,
        hp: state.player.hp,
        block: state.player.block,
        energy: state.turn.energy,
        enemy_hp: state.monsters.iter().map(|enemy| enemy.hp).collect(),
        enemy_block: state.monsters.iter().map(|enemy| enemy.block).collect(),
        hand: state
            .cards
            .hand
            .cards
            .iter()
            .map(|card| card.card_id)
            .collect(),
        draw_composition: state
            .cards
            .draw
            .cards
            .iter()
            .map(|card| card.card_id)
            .collect(),
        discard_composition: state
            .cards
            .discard
            .cards
            .iter()
            .map(|card| card.card_id)
            .collect(),
        cards_played_this_combat: state.turn.counters.card_ids_played_this_combat.clone(),
        end_trace_complete: history
            .entries()
            .last()
            .is_some_and(|entry| entry.draw_order_trace_complete),
        public_equal_to_end: false,
        exact_equal_to_end: false,
    };
    Ok((row, history.current().clone(), position))
}

fn append_world(
    rows: &mut Vec<Row>,
    root: &CombatPublicHistoryV1,
    source: &CombatPosition,
    candidate: usize,
    candidate_mass: f64,
) -> Result<(), Box<dyn std::error::Error>> {
    let lines = [
        Line::End,
        Line::DefendEnd,
        Line::ShrugEnd,
        Line::ShrugSlamIfSeenEnd,
    ];
    let mut compared = Vec::new();
    for line in lines {
        compared.push(run_line(root, source, candidate, candidate_mass, line)?);
    }
    let end_boundary = compared[0].1.clone();
    let end_position = compared[0].2.clone();
    for (mut row, boundary, position) in compared {
        row.public_equal_to_end = boundary == end_boundary;
        row.exact_equal_to_end = position == end_position;
        rows.push(row);
    }
    Ok(())
}

fn next_permutation(indices: &mut [usize]) -> bool {
    let Some(pivot) = (0..indices.len().saturating_sub(1))
        .rev()
        .find(|&i| indices[i] < indices[i + 1])
    else {
        return false;
    };
    let successor = (pivot + 1..indices.len())
        .rev()
        .find(|&i| indices[i] > indices[pivot])
        .unwrap();
    indices.swap(pivot, successor);
    indices[pivot + 1..].reverse();
    true
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !(1..=256).contains(&args.particles) {
        return Err("particles must be between 1 and 256".into());
    }
    let cases: Vec<ConstructedCombat> = serde_json::from_slice(&fs::read(&args.input)?)?;
    let source = cases
        .first()
        .ok_or("input needs one constructed combat recipe")?
        .build()?;
    let seeds = (10_001
        ..10_001
            + if args.all_draw_orders {
                1
            } else {
                args.particles
            } as u64)
        .collect();
    let environment = CombatBeliefEnvironmentV1::from_sampler(
        PublicCombatRunContextV1::Unavailable {
            reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
        },
        &source,
        &IndependentStreamsCombatBeliefSamplerV1::new(seeds).with_draw_order_knowledge(10_000),
    )?;
    let mut rows = Vec::new();
    if args.all_draw_orders {
        let cards = source
            .combat
            .zones
            .draw_pile
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        if cards.len() > 6 {
            return Err("all-draw-orders is limited to at most six cards".into());
        }
        let count = (1..=cards.len()).product::<usize>();
        let mut order = (0..cards.len()).collect::<Vec<_>>();
        for index in 0..count {
            let mut world = source.clone();
            world.combat.zones.draw_pile = order.iter().map(|&i| cards[i].clone()).collect();
            append_world(
                &mut rows,
                environment.public_history(),
                &world,
                index,
                1.0 / count as f64,
            )?;
            if index + 1 < count && !next_permutation(&mut order) {
                return Err("permutation enumeration ended early".into());
            }
        }
    } else {
        for (index, particle) in environment.particles().iter().enumerate() {
            append_world(
                &mut rows,
                environment.public_history(),
                particle.private_position(),
                index,
                particle.probability_mass(),
            )?;
        }
    }
    fs::write(&args.output, serde_json::to_vec_pretty(&rows)?)?;
    println!(
        "wrote {} paired rows to {}",
        rows.len(),
        args.output.display()
    );
    Ok(())
}
