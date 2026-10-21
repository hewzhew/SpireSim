use super::*;
use crate::eval::run_control::{
    LearningDecisionPathV3, LearningEnvPoolV3, LearningModelChoiceV1, LearningModelDecisionV1,
    LearningModelObservationV1, LearningSelectionStepV1,
};
use std::time::{Duration, Instant};

const EPISODE_STEP_LIMIT: usize = 20_000;

#[derive(Default)]
struct SmokeStats {
    total_steps: usize,
    strategic_steps: usize,
    combat_steps: usize,
    victories: usize,
}

struct SmokeRng(u64);

impl SmokeRng {
    fn pick(&mut self, len: usize) -> usize {
        debug_assert!(len > 0);
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 32) as usize) % len
    }
}

#[test]
fn seeded_legal_action_walk_reaches_terminal_without_unsupported_boundary() {
    let started = Instant::now();
    let mut stats = SmokeStats::default();
    let seeds = (1..=5).collect::<Vec<_>>();
    let mut pool = LearningEnvPoolV3::from_configs(seeds.iter().map(|seed| RunControlConfig {
        seed: *seed,
        ..RunControlConfig::default()
    }))
    .expect("create learning environment pool");
    let mut rngs = seeds
        .iter()
        .map(|seed| SmokeRng(seed ^ 0x9e37_79b9_7f4a_7c15))
        .collect::<Vec<_>>();

    while !pool.all_terminal() {
        assert!(
            stats.total_steps < EPISODE_STEP_LIMIT * seeds.len(),
            "legal action pool exceeded the aggregate step cap"
        );
        let batch = pool.active_model_batch().expect("build active model batch");
        let paths = batch
            .active_slot_indices
            .iter()
            .copied()
            .zip(&batch.decision_tokens)
            .zip(&batch.model_batch.decisions)
            .map(|((slot_index, token), decision)| {
                let ordinals = smoke_path(decision, &mut rngs[slot_index], &mut stats)
                    .unwrap_or_else(|error| panic!("slot {slot_index}: {error}"));
                LearningDecisionPathV3 {
                    source: token.clone(),
                    ordinals,
                }
            })
            .collect::<Vec<_>>();
        let transitions = pool.step_active_paths_v3(paths).expect("step active pool");
        stats.total_steps += transitions.len();
        for transition in transitions {
            if transition.next.is_none() {
                stats.victories += usize::from(transition.reward > 0);
            }
        }
    }

    print_summary(seeds.len(), &stats, started.elapsed());
}

fn smoke_path(
    decision: &LearningModelDecisionV1<'_>,
    rng: &mut SmokeRng,
    stats: &mut SmokeStats,
) -> Result<Vec<usize>, String> {
    match decision.observation {
        LearningModelObservationV1::Strategic(_) => stats.strategic_steps += 1,
        LearningModelObservationV1::Combat(_) => stats.combat_steps += 1,
    }
    let root = rng.pick(decision.candidates.len());
    match decision
        .choose(root)
        .map_err(|error| format!("root choice: {error}"))?
    {
        LearningModelChoiceV1::Apply(_) => Ok(vec![root]),
        LearningModelChoiceV1::DecodeSelection(mut draft) => {
            let mut path = vec![root];
            loop {
                let selection = draft.decision();
                let ordinal = rng.pick(selection.candidates.len());
                path.push(ordinal);
                match draft
                    .choose(ordinal)
                    .map_err(|error| format!("selection choice: {error}"))?
                {
                    LearningSelectionStepV1::Continue => {}
                    LearningSelectionStepV1::Apply(_) => break Ok(path),
                }
            }
        }
    }
}

fn print_summary(episodes: usize, stats: &SmokeStats, elapsed: Duration) {
    eprintln!(
        "learning_env_smoke episodes={} steps={} strategic_steps={} combat_steps={} victories={} elapsed_ms={} steps_per_second={:.0}",
        episodes,
        stats.total_steps,
        stats.strategic_steps,
        stats.combat_steps,
        stats.victories,
        elapsed.as_millis(),
        stats.total_steps as f64 / elapsed.as_secs_f64()
    );
}
