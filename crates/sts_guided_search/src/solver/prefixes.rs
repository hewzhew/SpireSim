//! Read-only bounded sampling of prefixes still held by the actual search.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SolverPrefixPool {
    Expanding,
    Active,
    AttackPlan,
    EffectPlan,
    OrderFamily,
    PendingChoice,
    NextAction,
    NextTurn,
    Deferred,
}

/// A searched prefix, not a solution, checkpoint or proof of future quality.
#[derive(Clone, Debug, Serialize)]
pub struct SolverPrefix {
    pub inputs: Vec<ClientInput>,
    pub successor: CombatPosition,
    pub width: usize,
    pub card_only: bool,
    pub pool: SolverPrefixPool,
}

impl CombatSolver {
    /// Visit the stored queues round-robin, retaining at most `limit` distinct
    /// materialized input prefixes. This does not advance, rank or remove work.
    /// Equal input chains from this same root are duplicates; equal positions
    /// reached by different input chains remain separate candidates.
    ///
    /// Queue order and earlier solver pruning bias this sample. It is not a
    /// sample of all legal states. Selection assembly is omitted because it has
    /// no materialized input yet; an actual PendingChoice boundary is admitted.
    pub fn sample_prefixes(&self, limit: usize) -> Vec<SolverPrefix> {
        if limit == 0 {
            return Vec::new();
        }
        struct Queue<'a> {
            width: usize,
            card_only: bool,
            pool: SolverPrefixPool,
            nodes: Box<dyn Iterator<Item = &'a Node> + 'a>,
        }
        let mut queues = Vec::new();
        for beam in &self.beams {
            for pass in &beam.passes {
                let pools: [(SolverPrefixPool, Box<dyn Iterator<Item = &Node>>); 9] = [
                    (
                        SolverPrefixPool::Expanding,
                        Box::new(
                            pass.expansion
                                .iter()
                                .chain(pass.ordinary.iter())
                                .map(|e| &e.node),
                        ),
                    ),
                    (SolverPrefixPool::Active, Box::new(pass.active.iter())),
                    (
                        SolverPrefixPool::OrderFamily,
                        Box::new(
                            pass.order_families.pending.iter().map(|c| &c.node).chain(
                                self.family_job
                                    .iter()
                                    .filter(|j| {
                                        j.candidate.origin.width == pass.width
                                            && j.candidate.origin.card_only == pass.card_only
                                    })
                                    .map(|j| &j.candidate.node),
                            ),
                        ),
                    ),
                    (
                        SolverPrefixPool::AttackPlan,
                        Box::new(
                            pass.tactical
                                .iter()
                                .filter(|job| job.plan.is_attack())
                                .map(|job| &job.node),
                        ),
                    ),
                    (
                        SolverPrefixPool::EffectPlan,
                        Box::new(
                            pass.tactical
                                .iter()
                                .filter(|job| !job.plan.is_attack())
                                .map(|job| &job.node),
                        ),
                    ),
                    (
                        SolverPrefixPool::PendingChoice,
                        Box::new(pass.pending_nodes()),
                    ),
                    (SolverPrefixPool::NextAction, Box::new(pass.next.iter())),
                    (SolverPrefixPool::NextTurn, Box::new(pass.ended.iter())),
                    (
                        SolverPrefixPool::Deferred,
                        Box::new(
                            pass.deferred.iter().chain(
                                pass.deferred_cursors
                                    .iter()
                                    .flat_map(actions::DeferredCursor::nodes),
                            ),
                        ),
                    ),
                ];
                queues.extend(pools.into_iter().map(|(pool, nodes)| Queue {
                    width: pass.width,
                    card_only: pass.card_only,
                    pool,
                    nodes,
                }));
            }
        }
        let mut sampled: Vec<SolverPrefix> = Vec::new();
        while sampled.len() < limit {
            let mut visited = false;
            for queue in &mut queues {
                while let Some(node) = queue.nodes.next() {
                    visited = true;
                    if node.input_count == 0
                        || node.state.selected_members().is_some()
                        || self.space.terminal(&node.state).is_some()
                    {
                        continue;
                    }
                    let inputs = node.inputs();
                    if sampled.iter().any(|other| other.inputs == inputs) {
                        continue;
                    }
                    sampled.push(SolverPrefix {
                        inputs,
                        successor: node.state.position().clone(),
                        width: queue.width,
                        card_only: queue.card_only,
                        pool: queue.pool,
                    });
                    break;
                }
                if sampled.len() == limit {
                    break;
                }
            }
            if !visited {
                break;
            }
        }
        sampled
    }
}
