//! Small exact continuations of score-tied pre-shuffle alternatives. Families
//! allocate work only: their members are never merged or declared equivalent.
use super::retention::SCORE_LANES;
use super::*;
use sts_core::content::cards::can_play_card;
use sts_core::state::core::EngineState;

#[derive(Clone, Debug, Serialize)]
pub struct OrderFamilyOrigin {
    pub admitted_at_work: usize,
    pub source_inputs: usize,
    pub turn: u32,
    pub lane: usize,
    pub scores: [i64; SCORE_LANES],
    pub members: usize,
    pub member: usize,
    pub child_work: usize,
}

pub(super) struct Candidate {
    pub node: Node,
    pub origin: SolverPlanOrigin,
}

#[derive(Default)]
pub(super) struct Queue {
    pub pending: VecDeque<Candidate>,
}

/// Completed child work is reusable across parent widths: every child receives
/// the same configuration and allowance. This remembers computations, not a
/// value estimate or proof that the root has no better continuation.
pub(super) struct CompletedRoot {
    state: CombatSearchState,
    expenditures: CombatInputExpendituresV1,
    input_count: usize,
}

impl CompletedRoot {
    fn covers(&self, node: &Node) -> bool {
        self.input_count <= node.input_count
            && self.expenditures == node.expenditures()
            && self.state == node.state
    }
}

fn ready(node: &Node) -> bool {
    let p = node.state.position();
    matches!(p.engine, EngineState::CombatPlayerTurn)
        && node.state.selected_members().is_none()
        && p.combat.zones.draw_pile.is_empty()
        && p.combat.zones.discard_pile.len() > 1
        && p.combat
            .zones
            .hand
            .iter()
            .all(|c| can_play_card(c, &p.combat).is_err())
}

fn same_family(a: &Node, b: &Node) -> bool {
    let a_c = &a.state.position().combat;
    let b_c = &b.state.position().combat;
    a_c.turn.turn_count == b_c.turn.turn_count
        && a.expenditures() == b.expenditures()
        && a_c.entities.player.current_hp == b_c.entities.player.current_hp
        && a_c.entities.player.block == b_c.entities.player.block
        && a_c
            .entities
            .monsters
            .iter()
            .map(|m| (m.current_hp, m.block))
            .eq(b_c
                .entities
                .monsters
                .iter()
                .map(|m| (m.current_hp, m.block)))
        && retention::same_retention_context(a, b, true, true)
}

impl Pass {
    pub(super) fn offer_order_families(
        &mut self,
        kept: &[Node],
        omitted: &[Node],
        stats: &mut SolverStats,
    ) {
        if !self.families_enabled || omitted.is_empty() {
            return;
        }
        let mut groups: Vec<(Vec<&Node>, [i64; SCORE_LANES])> = Vec::new();
        for node in kept.iter().filter(|n| ready(n)) {
            let scores = retention::scores(node);
            if groups
                .iter()
                .any(|(g, s)| *s == scores && same_family(g[0], node))
            {
                continue;
            }
            let variants: Vec<_> = omitted
                .iter()
                .filter(|n| ready(n) && retention::scores(n) == scores && same_family(node, n))
                .collect();
            if variants.is_empty() {
                continue;
            }
            let mut members: Vec<_> = kept
                .iter()
                .filter(|n| ready(n) && retention::scores(n) == scores && same_family(node, n))
                .collect();
            members.extend(variants);
            groups.push((members, scores));
        }
        let mut winners = Vec::new();
        for lane in 0..SCORE_LANES {
            let best = (0..groups.len()).min_by_key(|&i| {
                (
                    std::cmp::Reverse(groups[i].1[lane]),
                    groups[i].0[0].input_count,
                    i,
                )
            });
            if let Some(i) = best {
                if !winners.iter().any(|&(g, _)| g == i) {
                    winners.push((i, lane));
                }
            }
        }
        for (i, lane) in winners {
            let (members, scores) = &groups[i];
            let mut admitted = false;
            for (member, node) in members.iter().enumerate().take(self.width) {
                if self.order_families.pending.iter().any(|old| {
                    old.node.input_count <= node.input_count
                        && old.node.state == node.state
                        && old.node.expenditures() == node.expenditures()
                }) {
                    stats.order_family_duplicates += 1;
                    continue;
                }
                if self.order_families.pending.len() >= self.width * 8 {
                    stats.order_family_limit_dropped += 1;
                    continue;
                }
                admitted = true;
                self.order_families.pending.push_back(Candidate {
                    node: (*node).clone(),
                    origin: SolverPlanOrigin {
                        width: self.width,
                        card_only: self.card_only,
                        recovery: self.primary_done,
                        observed_at_work: stats.work(),
                        pass_search_work: self.search_work,
                        shuffle: None,
                        potion_edit: None,
                        order_edit: None,
                        order_family: Some(OrderFamilyOrigin {
                            admitted_at_work: stats.work(),
                            source_inputs: node.input_count,
                            turn: node.state.position().combat.turn.turn_count,
                            lane,
                            scores: *scores,
                            members: members.len(),
                            member,
                            child_work: 0,
                        }),
                    },
                });
                stats.order_family_candidates += 1;
            }
            stats.order_families_admitted += usize::from(admitted);
        }
    }
}

pub(super) struct Job {
    pub candidate: Candidate,
    child: Box<CombatSolver>,
    published_work: usize,
    allowance: usize,
}

pub(super) enum Event {
    Progress,
    TimedOut,
    Done,
    Win {
        inputs: Vec<ClientInput>,
        expected: CombatPosition,
        origin: SolverPlanOrigin,
    },
}

impl Job {
    fn new(candidate: Candidate, config: &SolverConfig) -> Result<Self, String> {
        let mut child = CombatSolver::new(
            candidate.node.state.position().clone(),
            SolverConfig {
                max_potion_uses: Some(0),
                order_family_work: 0,
                shuffle_continuation_work: 0,
                refine_potion_plans: false,
                refine_card_order: false,
                ..config.clone()
            },
        )?;
        // One source-bound tactical entry, with every draw and play charged to
        // this child's existing allowance. Later hands use ordinary retention.
        child.beams[0].passes[0].first_draw_attack_turn =
            Some(candidate.node.state.position().combat.turn.turn_count + 1);
        Ok(Self {
            candidate,
            child: Box::new(child),
            published_work: 0,
            allowance: config.order_family_work,
        })
    }

    pub(super) fn advance(
        &mut self,
        limits: CombatStepLimits,
        stats: &mut SolverStats,
    ) -> Result<Event, String> {
        if let Some(plan) = self
            .child
            .plans()
            .iter()
            .filter(|p| p.found_at_work > self.published_work)
            .min_by_key(|p| p.found_at_work)
        {
            self.published_work = plan.found_at_work;
            let mut inputs = self.candidate.node.inputs();
            inputs.extend_from_slice(&plan.inputs);
            let mut origin = self.candidate.origin.clone();
            origin.order_family.as_mut().unwrap().child_work = self.child.stats.work();
            stats.order_family_observed_wins += 1;
            return Ok(Event::Win {
                inputs,
                expected: plan.successor.clone(),
                origin,
            });
        }
        if self.child.is_complete() || self.child.stats.work() >= self.allowance {
            stats.order_family_completed += 1;
            return Ok(Event::Done);
        }
        let work = self.child.stats.work();
        let steps = self.child.stats.engine_steps;
        let remaining = limits.deadline.map_or(Duration::from_secs(3600), |d| {
            d.saturating_duration_since(Instant::now())
        });
        let result = self.child.advance(1, remaining);
        stats.order_family_work += self.child.stats.work() - work;
        stats.engine_steps += self.child.stats.engine_steps - steps;
        Ok(if result? == SolverStop::TimeBudget {
            Event::TimedOut
        } else {
            Event::Progress
        })
    }
}

impl CombatSolver {
    pub(super) fn finish_order_family(&mut self) {
        let job = self.family_job.take().unwrap();
        debug_assert!(job.child.is_complete() || job.child.stats.work() >= job.allowance);
        if self.family_completed.len() == self.config.max_width * 8 {
            self.family_completed.pop_front();
        }
        self.family_completed.push_back(CompletedRoot {
            expenditures: job.candidate.node.expenditures(),
            state: job.candidate.node.state,
            input_count: job.candidate.node.input_count,
        });
    }

    pub(super) fn prepare_order_family(&mut self) -> Result<(), String> {
        if self.family_job.is_some() {
            return Ok(());
        }
        let count: usize = self.beams.iter().map(|b| b.passes.len()).sum();
        for offset in 0..count {
            let i = (self.family_cursor + offset) % count;
            let pass = self
                .beams
                .iter_mut()
                .flat_map(|b| &mut b.passes)
                .nth(i)
                .unwrap();
            if let Some(candidate) = pass.order_families.pending.pop_front() {
                if self
                    .family_completed
                    .iter()
                    .any(|old| old.covers(&candidate.node))
                {
                    self.stats.order_family_cached += 1;
                    continue;
                }
                self.family_job = Some(Job::new(candidate, &self.config)?);
                self.stats.order_family_jobs += 1;
                self.family_cursor = (i + 1) % count;
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::content::{
        cards::CardId,
        monsters::EnemyId,
        potions::{Potion, PotionId},
    };
    use sts_core::runtime::combat::CombatCard;

    fn root() -> CombatPosition {
        let mut c = sts_core::test_support::blank_test_combat();
        let mut enemy = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        enemy.current_hp = 50;
        enemy.max_hp = 50;
        c.entities.monsters = vec![enemy];
        c.zones.hand = [
            CardId::Strike,
            CardId::Defend,
            CardId::Anger,
            CardId::Strike,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 1))
        .collect();
        c.entities.potions = vec![Some(Potion::new(PotionId::BlockPotion, 1))];
        CombatPosition::new(EngineState::CombatPlayerTurn, c)
    }

    #[test]
    fn families_are_bounded_full_state_jobs_not_equivalence_merges() {
        let mut p = root();
        p.combat.turn.energy = 0;
        p.combat.zones.discard_pile = std::mem::take(&mut p.combat.zones.hand).into();
        let make = |p| Node::root(CombatSearchState::new(p).unwrap());
        let kept = make(p.clone());
        let mut swapped = p.combat.zones.discard_pile.clone().into_vec();
        swapped.swap(0, 1);
        p.combat.zones.discard_pile = swapped.into();
        let omitted = make(p.clone());
        assert!(same_family(&kept, &omitted));
        assert!(kept.state != omitted.state);
        let mut pass = Pass::new(kept.clone(), 4, true);
        pass.families_enabled = true;
        let mut stats = SolverStats::default();
        pass.offer_order_families(&[kept.clone()], &[omitted.clone()], &mut stats);
        assert_eq!(pass.order_families.pending.len(), 2);
        pass.offer_order_families(&[kept.clone()], &[omitted], &mut stats);
        assert_eq!(pass.order_families.pending.len(), 2);
        assert_eq!(stats.order_family_duplicates, 2);
        p.combat
            .zones
            .hand
            .push(CombatCard::for_scenario(CardId::Anger, 20));
        assert!(
            !ready(&make(p.clone())),
            "zero energy does not imply no playable cards"
        );
        p.combat.zones.hand.clear();
        p.combat.entities.player.block += 1;
        assert!(!same_family(&kept, &make(p.clone())));
        p.combat.entities.player.block -= 1;
        for i in 100..160 {
            p.combat.zones.discard_pile[0].lineage =
                CombatCard::for_scenario(CardId::Strike, i).lineage;
            let variant = make(p.clone());
            pass.offer_order_families(&[kept.clone()], &[variant], &mut stats);
        }
        assert_eq!(pass.order_families.pending.len(), pass.width * 8);
        assert!(stats.order_family_limit_dropped > 0);
    }

    #[test]
    fn completed_work_is_shared_across_widths_but_orders_rng_and_spending_stay_distinct() {
        let mut source = root();
        source.combat.turn.energy = 0;
        source.combat.zones.discard_pile = std::mem::take(&mut source.combat.zones.hand).into();
        let make = |p| {
            let mut node = Node::root(CombatSearchState::new(p).unwrap());
            node.input_count = 3;
            node
        };
        let mut node = make(source.clone());
        node.input_count = 3;
        let cached = CompletedRoot {
            state: node.state.clone(),
            expenditures: node.expenditures(),
            input_count: 3,
        };
        assert!(cached.covers(&node));
        node.input_count = 4;
        assert!(cached.covers(&node));
        node.input_count = 2;
        assert!(
            !cached.covers(&node),
            "a shorter prefix can improve equal-resource tie breaking"
        );
        node.input_count = 3;
        let mut changed = source.clone();
        let mut cards = changed.combat.zones.discard_pile.clone().into_vec();
        cards.swap(0, 1);
        changed.combat.zones.discard_pile = cards.into();
        assert!(!cached.covers(&make(changed)));
        let mut changed = source.clone();
        changed.combat.rng.shuffle_rng.random_long();
        assert!(!cached.covers(&make(changed)));
        let mut spent = node.clone();
        spent.trace = Some(Arc::new(Trace {
            previous: None,
            input: ClientInput::DiscardPotion(0),
            expenditures: CombatInputExpendituresV1::from_inputs(&[ClientInput::DiscardPotion(0)]),
        }));
        assert!(!cached.covers(&spent));

        let mut solver = CombatSolver::new(
            source,
            SolverConfig {
                max_width: 4,
                max_potion_uses: Some(0),
                order_family_work: 1,
                ..Default::default()
            },
        )
        .unwrap();
        solver.family_completed.push_back(cached);
        // Manufacture a second width's pending exact source. Its metadata has
        // no role in deciding whether the same child computation was completed.
        let origin = SolverPlanOrigin {
            width: 8,
            card_only: true,
            recovery: false,
            observed_at_work: 0,
            pass_search_work: 0,
            shuffle: None,
            potion_edit: None,
            order_edit: None,
            order_family: Some(OrderFamilyOrigin {
                admitted_at_work: 0,
                source_inputs: 3,
                turn: 0,
                lane: 0,
                scores: [0; SCORE_LANES],
                members: 2,
                member: 0,
                child_work: 0,
            }),
        };
        solver.beams[0].passes[0]
            .order_families
            .pending
            .push_back(Candidate {
                node: node.clone(),
                origin: origin.clone(),
            });
        solver.prepare_order_family().unwrap();
        assert!(solver.family_job.is_none());
        assert_eq!(solver.stats.order_family_cached, 1);
        assert_eq!(solver.stats.work(), 0);
        node.input_count = 2;
        solver.beams[0].passes[0]
            .order_families
            .pending
            .push_back(Candidate { node, origin });
        solver.prepare_order_family().unwrap();
        assert!(solver.family_job.is_some());
        assert_eq!(solver.family_completed.len(), 1);
        // A cache contains completed work only, and its storage is bounded.
        let candidate_node = solver.family_job.as_ref().unwrap().candidate.node.clone();
        let candidate_origin = solver.family_job.as_ref().unwrap().candidate.origin.clone();
        for _ in 0..40 {
            if solver.family_job.is_none() {
                solver.family_job = Some(
                    Job::new(
                        Candidate {
                            node: candidate_node.clone(),
                            origin: candidate_origin.clone(),
                        },
                        &solver.config,
                    )
                    .unwrap(),
                );
            }
            let limits = CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            };
            while !matches!(
                solver
                    .family_job
                    .as_mut()
                    .unwrap()
                    .advance(limits, &mut solver.stats)
                    .unwrap(),
                Event::Done
            ) {}
            solver.finish_order_family();
        }
        assert_eq!(solver.family_completed.len(), solver.config.max_width * 8);
    }

    #[test]
    fn automatic_families_resume_and_replay_within_the_original_budget() {
        let root = root();
        let config = SolverConfig {
            max_width: 8,
            max_potion_uses: Some(0),
            order_family_work: 256,
            ..Default::default()
        };
        let mut whole = CombatSolver::new(root.clone(), config.clone()).unwrap();
        let mut chunks = CombatSolver::new(root.clone(), config.clone()).unwrap();
        whole.advance(6000, Duration::from_secs(30)).unwrap();
        let mut saw_child = false;
        for _ in 0..6000 {
            let before = chunks.stats.work();
            chunks.advance(1, Duration::from_secs(30)).unwrap();
            assert!(chunks.stats.work() <= before + 1);
            if let Some(job) = &chunks.family_job {
                saw_child = true;
                assert_eq!(job.child.config.order_family_work, 0);
                assert_eq!(
                    job.child.stats.plan_edit_work + job.child.stats.shuffle_work,
                    0
                );
                assert!(job.child.stats.work() <= 256);
                let before = serde_json::to_value(chunks.stats()).unwrap();
                let progress = serde_json::to_value(chunks.progress()).unwrap();
                let _ = chunks.sample_prefixes(10);
                chunks.advance(0, Duration::from_secs(30)).unwrap();
                chunks.advance(1, Duration::ZERO).unwrap();
                assert_eq!(serde_json::to_value(chunks.stats()).unwrap(), before);
                assert_eq!(serde_json::to_value(chunks.progress()).unwrap(), progress);
            }
        }
        assert!(saw_child);
        assert!(whole.stats.order_family_work > 0);
        assert!(whole.stats.order_family_observed_wins > 0);
        assert_eq!(
            serde_json::to_value(whole.stats()).unwrap(),
            serde_json::to_value(chunks.stats()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(whole.plans()).unwrap(),
            serde_json::to_value(chunks.plans()).unwrap()
        );
        assert_eq!(whole.root(), &root);
        let reference = &whole.plans()[0];
        let mut watched = CombatSolver::new(root.clone(), config).unwrap();
        watched
            .audit_reference(&reference.inputs, &reference.successor)
            .unwrap();
        watched.advance(6000, Duration::from_secs(30)).unwrap();
        assert_eq!(
            serde_json::to_value(whole.stats()).unwrap(),
            serde_json::to_value(watched.stats()).unwrap()
        );
        assert_eq!(
            serde_json::to_value(whole.plans()).unwrap(),
            serde_json::to_value(watched.plans()).unwrap()
        );
        for plan in whole.plans() {
            assert_eq!(
                crate::shuffle_order::replay(&root, &plan.inputs)
                    .unwrap()
                    .last(),
                Some(&plan.successor)
            );
            assert_eq!(plan.outcome.total_potion_expenditures, 0);
            assert_eq!(
                plan.successor.combat.entities.potions,
                root.combat.entities.potions
            );
        }
    }

    #[test]
    fn queued_child_finishes_after_primary_and_original_prefix_spending_still_counts() {
        let mut root = root();
        root.combat.entities.monsters[0].current_hp = 6;
        root.combat.turn.energy = 0;
        root.combat.zones.discard_pile = std::mem::take(&mut root.combat.zones.hand).into();
        let input = ClientInput::UsePotion {
            potion_index: 0,
            target: None,
        };
        let next = EngineCombatStepper.apply_to_stable(
            &root,
            input.clone(),
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(!next.timed_out && !next.truncated);
        let mut node = Node::root(CombatSearchState::new(next.position).unwrap());
        node.input_count = 1;
        node.trace = Some(Arc::new(Trace {
            previous: None,
            expenditures: CombatInputExpendituresV1::from_inputs(std::slice::from_ref(&input)),
            input,
        }));
        let origin = SolverPlanOrigin {
            width: 4,
            card_only: false,
            recovery: false,
            observed_at_work: 0,
            pass_search_work: 0,
            shuffle: None,
            potion_edit: None,
            order_edit: None,
            order_family: Some(OrderFamilyOrigin {
                admitted_at_work: 0,
                source_inputs: 1,
                turn: 0,
                lane: 0,
                scores: [0; SCORE_LANES],
                members: 1,
                member: 0,
                child_work: 0,
            }),
        };
        for max in [0, 1] {
            let mut solver = CombatSolver::new(
                root.clone(),
                SolverConfig {
                    max_width: 4,
                    max_potion_uses: Some(max),
                    order_family_work: 64,
                    ..Default::default()
                },
            )
            .unwrap();
            solver.done = true;
            solver.beams[0].passes[0]
                .order_families
                .pending
                .push_back(Candidate {
                    node: node.clone(),
                    origin: origin.clone(),
                });
            assert!(!solver.is_complete());
            assert_eq!(
                solver.advance(500, Duration::from_secs(30)).unwrap(),
                SolverStop::ScheduleComplete
            );
            assert!(solver.stats.order_family_work > 0);
            assert_eq!(solver.stats.order_family_completed, 1);
            if max == 0 {
                assert!(solver.plans().is_empty());
            } else {
                let plan = &solver.plans()[0];
                assert_eq!(plan.outcome.total_potion_expenditures, 1);
                assert_eq!(plan.outcome.starting_potion_expenditure_slots, 1);
                assert!(plan.origin.order_family.is_some());
                assert_eq!(
                    crate::shuffle_order::replay(&root, &plan.inputs)
                        .unwrap()
                        .last(),
                    Some(&plan.successor)
                );
                assert_eq!(solver.stats.plan_edit_jobs + solver.stats.shuffle_jobs, 0);
            }
        }
    }
}
