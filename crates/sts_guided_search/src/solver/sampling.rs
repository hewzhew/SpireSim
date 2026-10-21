//! Bounded, observational sampling at actual primary-pass retention boundaries.
use super::*;
use std::sync::Mutex;

#[derive(Clone, Debug, Serialize)]
pub struct CandidateSample {
    pub kept: bool,
    /// Index within the kept or omitted side, not a gameplay action ID.
    pub side_index: usize,
    pub position: CombatPosition,
    /// Exact root-relative materialized inputs; no virtual selection prefix.
    pub inputs: Vec<ClientInput>,
    pub potion_uses: usize,
    pub expenditures: CombatInputExpendituresV1,
}

#[derive(Clone, Debug, Serialize)]
pub struct CandidateSampleGroup {
    pub ordinal: usize,
    pub at_work: usize,
    pub pass_width: usize,
    pub card_only: bool,
    pub capacity: usize,
    pub pool: String,
    pub kept_count: usize,
    pub omitted_count: usize,
    pub materialized_kept_count: usize,
    pub materialized_omitted_count: usize,
    pub candidates: Vec<CandidateSample>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CandidateSamples {
    pub max_groups: usize,
    pub candidates_per_side: usize,
    pub contested_groups_seen: usize,
    /// A genuine selection boundary is retained; only virtual assembly is excluded.
    pub virtual_candidates_skipped: usize,
    pub groups: Vec<CandidateSampleGroup>,
}

pub(super) struct Sampler {
    report: CandidateSamples,
}

// Counter-based diagnostic randomness never reads or advances gameplay RNG.
fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

impl Sampler {
    pub(super) fn observe(
        &mut self,
        kept: &[Node],
        omitted: &[Node],
        pass_width: usize,
        card_only: bool,
        capacity: usize,
        pool: &str,
        at_work: usize,
    ) {
        if omitted.is_empty() {
            return;
        }
        let materialized = |nodes: &[Node]| {
            nodes
                .iter()
                .filter(|n| n.state.selected_members().is_none())
                .count()
        };
        let kept_count = materialized(kept);
        let omitted_count = materialized(omitted);
        self.report.virtual_candidates_skipped +=
            kept.len() + omitted.len() - kept_count - omitted_count;
        if kept_count == 0 || omitted_count == 0 {
            return;
        }
        let ordinal = self.report.contested_groups_seen;
        self.report.contested_groups_seen += 1;
        let slot = if self.report.groups.len() < self.report.max_groups {
            self.report.groups.len()
        } else {
            let slot = mix(ordinal as u64) % self.report.contested_groups_seen as u64;
            if slot >= self.report.max_groups as u64 {
                return;
            }
            slot as usize
        };
        let mut candidates = vec![];
        for (retained, nodes) in [(true, kept), (false, omitted)] {
            let mut indices = nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.state.selected_members().is_none())
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            indices.sort_by_key(|&i| {
                mix((ordinal as u64).wrapping_mul(65537)
                    ^ (i as u64).wrapping_mul(2)
                    ^ u64::from(retained))
            });
            indices.truncate(self.report.candidates_per_side);
            indices.sort_unstable();
            for i in indices {
                let node = &nodes[i];
                candidates.push(CandidateSample {
                    kept: retained,
                    side_index: i,
                    position: node.state.position().clone(),
                    inputs: node.inputs(),
                    potion_uses: node.state.potion_uses(),
                    expenditures: node.expenditures(),
                });
            }
        }
        let group = CandidateSampleGroup {
            ordinal,
            at_work,
            pass_width,
            card_only,
            capacity,
            pool: pool.into(),
            kept_count: kept.len(),
            omitted_count: omitted.len(),
            materialized_kept_count: kept_count,
            materialized_omitted_count: omitted_count,
            candidates,
        };
        if slot == self.report.groups.len() {
            self.report.groups.push(group);
        } else {
            self.report.groups[slot] = group;
        }
    }
}

impl CombatSolver {
    /// Sample contested primary-pass pools without a reference or extra engine work.
    /// Deterministic reservoir over groups; at most N materialized states per side.
    /// This diagnostic storage is opt-in, separate from search budgets/configuration.
    pub fn sample_candidates(
        &mut self,
        max_groups: usize,
        candidates_per_side: usize,
    ) -> Result<(), String> {
        if self.stats.work() != 0 || self.beams[0].passes[0].samples.is_some() {
            return Err("candidate sampling must be enabled once before search".into());
        }
        if !(1..=128).contains(&max_groups) || !(1..=16).contains(&candidates_per_side) {
            return Err(
                "candidate sampling requires 1..128 groups and 1..16 candidates per side".into(),
            );
        }
        let shared = Arc::new(Mutex::new(Sampler {
            report: CandidateSamples {
                max_groups,
                candidates_per_side,
                contested_groups_seen: 0,
                virtual_candidates_skipped: 0,
                groups: vec![],
            },
        }));
        for pass in self.beams.iter_mut().flat_map(|b| &mut b.passes) {
            pass.samples = Some(shared.clone());
        }
        Ok(())
    }

    /// Read only: no replay, label production, inference or resumed search.
    pub fn candidate_samples(&self) -> Option<CandidateSamples> {
        self.beams[0].passes[0].samples.as_ref().map(|s| {
            let mut report = s.lock().unwrap().report.clone();
            report.groups.sort_by_key(|g| g.ordinal);
            report
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::cards::CardId;

    #[test]
    fn candidate_sampling_preserves_search_and_exports_replayable_states() {
        let source = super::super::tests::root(
            &[
                CardId::Strike,
                CardId::Defend,
                CardId::Bash,
                CardId::ShrugItOff,
                CardId::PommelStrike,
                CardId::TrueGrit,
            ],
            80,
        );
        let config = SolverConfig {
            max_width: 8,
            ..Default::default()
        };
        let mut plain = CombatSolver::new(source.clone(), config.clone()).unwrap();
        let mut sampled = CombatSolver::new(source.clone(), config).unwrap();
        sampled.sample_candidates(3, 2).unwrap();
        plain.advance(2000, Duration::from_secs(30)).unwrap();
        sampled.advance(2000, Duration::from_secs(30)).unwrap();
        assert_eq!(
            serde_json::to_value(plain.stats()).unwrap(),
            serde_json::to_value(sampled.stats()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(plain.plans()).unwrap(),
            serde_json::to_value(sampled.plans()).unwrap()
        );
        let report = sampled.candidate_samples().unwrap();
        assert!(report.contested_groups_seen > 3);
        assert_eq!(report.groups.len(), 3);
        for group in report.groups {
            assert!(group.candidates.iter().any(|n| n.kept));
            assert!(group.candidates.iter().any(|n| !n.kept));
            assert!(group.candidates.len() <= 4);
            for sample in group.candidates {
                let next = crate::shuffle_order::replay(&source, &sample.inputs)
                    .unwrap()
                    .pop()
                    .unwrap();
                assert_eq!(next, sample.position);
                assert_eq!(
                    sample.potion_uses,
                    sample
                        .inputs
                        .iter()
                        .filter(|i| matches!(i, ClientInput::UsePotion { .. }))
                        .count()
                );
            }
        }
        assert!(sampled.sample_candidates(3, 2).is_err());
    }

    #[test]
    fn candidate_sampling_rejects_unbounded_storage() {
        let source = super::super::tests::root(&[CardId::Strike], 20);
        let mut solver = CombatSolver::new(source, SolverConfig::default()).unwrap();
        for (groups, side) in [(0, 2), (129, 2), (3, 0), (3, 17)] {
            assert!(solver.sample_candidates(groups, side).is_err());
        }
        assert!(solver.candidate_samples().is_none());
    }

    #[test]
    fn candidate_sampling_keeps_real_pending_choice_but_excludes_virtual_assembly() {
        use sts_core::state::core::{HandSelectReason, PendingChoice};
        let mut source = super::super::tests::root(&[CardId::Strike, CardId::Defend], 20);
        source.engine = EngineState::PendingChoice(PendingChoice::HandSelect {
            candidate_lineages: sts_core::test_support::scenario_lineages([1, 2]),
            min_cards: 1,
            max_cards: 2,
            can_cancel: false,
            reason: HandSelectReason::Exhaust,
        });
        let state = CombatSearchState::new(source).unwrap();
        let space = CombatSearchSpace::default();
        let prepared = space.prepare_choices(&state).unwrap();
        let branch = prepared
            .branches()
            .iter()
            .position(|b| *b == CombatBranch::BeginSelection(0))
            .unwrap();
        let Transition::Settled {
            state: virtual_state,
            ..
        } = prepared
            .transition(
                branch,
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            )
            .unwrap()
        else {
            panic!("selection assembly did not settle");
        };
        assert!(virtual_state.selected_members().is_some());
        let real = Node::root(state);
        let mut sampler = Sampler {
            report: CandidateSamples {
                max_groups: 1,
                candidates_per_side: 2,
                contested_groups_seen: 0,
                virtual_candidates_skipped: 0,
                groups: vec![],
            },
        };
        sampler.observe(
            &[real.clone(), Node::root(virtual_state)],
            &[real],
            1,
            false,
            1,
            "test",
            0,
        );
        assert_eq!(sampler.report.virtual_candidates_skipped, 1);
        assert_eq!(sampler.report.groups[0].candidates.len(), 2);
        assert!(sampler.report.groups[0]
            .candidates
            .iter()
            .all(|c| matches!(c.position.engine, EngineState::PendingChoice(_))));
    }
}
