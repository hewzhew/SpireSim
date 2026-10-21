//! Canonical combat belief environment over public history and exact particles.

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::agent::information::action::{
    project_public_combat_actions_v1, resolve_public_combat_action_v1,
    CombatActionResolutionErrorV1, PublicCombatActionChoiceV1, PublicCombatActionSurfaceV1,
};
use crate::agent::information::draw_order::{DrawKnowledgeError, DrawOrderKnowledge};
use crate::agent::information::run::PublicCombatRunContextV1;
use crate::agent::information::state::{public_combat_state_v1, PublicCombatStateV1};
use crate::runtime::combat::{DrawCardKind, DrawPileChange};
use crate::sim::combat::{CombatPosition, CombatStepLimits, CombatStepper, CombatTerminal};

use super::combat::{
    CombatBeliefParticleV1, CombatBeliefSamplerV1, CombatBeliefSamplingErrorV1,
    CombatBeliefSamplingRequestV1,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicDecisionV1 {
    pub run_context: PublicCombatRunContextV1,
    pub observation: PublicCombatStateV1,
    pub actions: PublicCombatActionSurfaceV1,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CombatPublicBoundaryV1 {
    Decision {
        decision: CombatPublicDecisionV1,
    },
    Terminal {
        run_context: PublicCombatRunContextV1,
        observation: PublicCombatStateV1,
        outcome: CombatTerminal,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicHistoryEntryV1 {
    pub chosen_action: Option<PublicCombatActionChoiceV1>,
    pub boundary: CombatPublicBoundaryV1,
    /// Intermediate visible changes, excluding internal invalidation markers.
    #[serde(default)]
    pub draw_pile_changes: Vec<DrawPileChange>,
    /// Missing/legacy observation coverage rebuilds knowledge from the public
    /// boundary. It never licenses retaining unverified order constraints.
    #[serde(default)]
    pub draw_order_trace_complete: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPublicHistoryV1 {
    entries: Vec<CombatPublicHistoryEntryV1>,
}

impl CombatPublicHistoryV1 {
    pub fn root(boundary: CombatPublicBoundaryV1) -> Self {
        Self {
            entries: vec![CombatPublicHistoryEntryV1 {
                chosen_action: None,
                boundary,
                draw_pile_changes: Vec::new(),
                draw_order_trace_complete: false,
            }],
        }
    }

    pub fn entries(&self) -> &[CombatPublicHistoryEntryV1] {
        &self.entries
    }

    pub fn current(&self) -> &CombatPublicBoundaryV1 {
        &self
            .entries
            .last()
            .expect("combat public history always contains its root")
            .boundary
    }

    pub fn draw_order_knowledge(&self) -> Result<DrawOrderKnowledge, DrawKnowledgeError> {
        let mut knowledge =
            DrawOrderKnowledge::from_observation(boundary_observation(&self.entries[0].boundary));
        for entry in &self.entries[1..] {
            let observation = boundary_observation(&entry.boundary);
            knowledge = if entry.draw_order_trace_complete {
                knowledge.after_changes(&entry.draw_pile_changes, observation)?
            } else {
                DrawOrderKnowledge::from_observation(observation)
            };
        }
        Ok(knowledge)
    }

    /// Advance a separately owned execution world using the same observation
    /// contract as the candidate population. No particle identity is consulted.
    pub fn advance_exact(
        &self,
        position: &CombatPosition,
        action: PublicCombatActionChoiceV1,
        stepper: &impl CombatStepper,
        max_engine_steps: usize,
    ) -> Result<(Self, CombatPosition), CombatBeliefEnvironmentErrorV1> {
        if public_boundary_for_position_v1(
            current_run_context_v1(self.current()).clone(),
            position,
        )? != *self.current()
        {
            return Err(CombatBeliefEnvironmentErrorV1::ParticleBoundaryMismatch);
        }
        let (entry, next) = observe_transition(
            self.current(),
            position,
            action.clone(),
            stepper,
            max_engine_steps,
        )?;
        Ok((
            self.successor(
                action,
                entry.boundary,
                entry.draw_pile_changes,
                entry.draw_order_trace_complete,
            ),
            next,
        ))
    }

    pub(super) fn successor(
        &self,
        chosen_action: PublicCombatActionChoiceV1,
        boundary: CombatPublicBoundaryV1,
        draw_pile_changes: Vec<DrawPileChange>,
        draw_order_trace_complete: bool,
    ) -> Self {
        let mut next = self.clone();
        next.entries.push(CombatPublicHistoryEntryV1 {
            chosen_action: Some(chosen_action),
            boundary,
            draw_pile_changes,
            draw_order_trace_complete,
        });
        next
    }
}

#[derive(Clone, Debug)]
pub struct CombatBeliefEnvironmentV1 {
    public_history: CombatPublicHistoryV1,
    particles: Vec<CombatBeliefParticleV1>,
}

struct ObservationGroup {
    boundary: CombatPublicBoundaryV1,
    changes: Vec<DrawPileChange>,
    complete: bool,
    particles: Vec<CombatBeliefParticleV1>,
}

impl CombatBeliefEnvironmentV1 {
    pub fn from_sampler(
        run_context: PublicCombatRunContextV1,
        exact_source: &CombatPosition,
        sampler: &impl CombatBeliefSamplerV1,
    ) -> Result<Self, CombatBeliefEnvironmentErrorV1> {
        let root = public_boundary_for_position_v1(run_context, exact_source)?;
        if !matches!(root, CombatPublicBoundaryV1::Decision { .. }) {
            return Err(CombatBeliefEnvironmentErrorV1::TerminalRoot);
        }
        let public_history = CombatPublicHistoryV1::root(root);
        Self::from_history_and_sampler(public_history, exact_source, sampler)
    }

    /// Rebuild a particle population at any represented public-history node.
    /// The sampler receives the complete prefix and declares how much of it
    /// actually conditions its distribution.
    pub fn from_history_and_sampler(
        public_history: CombatPublicHistoryV1,
        exact_source: &CombatPosition,
        sampler: &impl CombatBeliefSamplerV1,
    ) -> Result<Self, CombatBeliefEnvironmentErrorV1> {
        if !matches!(
            public_history.current(),
            CombatPublicBoundaryV1::Decision { .. }
        ) {
            return Err(CombatBeliefEnvironmentErrorV1::TerminalRoot);
        }
        let particles = sampler
            .sample(CombatBeliefSamplingRequestV1 {
                public_history: &public_history,
                exact_source,
            })
            .map_err(CombatBeliefEnvironmentErrorV1::Sampling)?;
        Self::from_particles(public_history, particles)
    }

    pub fn from_particles(
        public_history: CombatPublicHistoryV1,
        mut particles: Vec<CombatBeliefParticleV1>,
    ) -> Result<Self, CombatBeliefEnvironmentErrorV1> {
        if particles.is_empty() {
            return Err(CombatBeliefEnvironmentErrorV1::EmptyPopulation);
        }
        let knowledge = public_history
            .draw_order_knowledge()
            .map_err(CombatBeliefEnvironmentErrorV1::DrawKnowledge)?;
        let total_mass = particles.iter().try_fold(0.0, |total, particle| {
            let mass = particle.probability_mass();
            if !mass.is_finite() || mass <= 0.0 {
                Err(CombatBeliefEnvironmentErrorV1::InvalidProbabilityMass)
            } else {
                Ok(total + mass)
            }
        })?;
        for particle in &mut particles {
            particle.set_probability_mass(particle.probability_mass() / total_mass);
            let actual = public_boundary_for_position_v1(
                current_run_context_v1(public_history.current()).clone(),
                particle.private_position(),
            )?;
            if &actual != public_history.current() {
                return Err(CombatBeliefEnvironmentErrorV1::ParticleBoundaryMismatch);
            }
            let kind_order = particle
                .private_position()
                .combat
                .zones
                .draw_pile
                .iter()
                .map(DrawCardKind::from)
                .collect::<Vec<_>>();
            if !knowledge.permits_kind_order(&kind_order) {
                return Err(CombatBeliefEnvironmentErrorV1::ParticleKnowledgeMismatch);
            }
        }
        Ok(Self {
            public_history,
            particles,
        })
    }

    pub fn public_history(&self) -> &CombatPublicHistoryV1 {
        &self.public_history
    }

    pub fn particles(&self) -> &[CombatBeliefParticleV1] {
        &self.particles
    }

    pub fn step(
        &self,
        action: PublicCombatActionChoiceV1,
        stepper: &impl CombatStepper,
        max_engine_steps: usize,
    ) -> Result<Vec<CombatBeliefChanceBranchV1>, CombatBeliefEnvironmentErrorV1> {
        let CombatPublicBoundaryV1::Decision { .. } = self.public_history.current() else {
            return Err(CombatBeliefEnvironmentErrorV1::TerminalBoundary);
        };
        let mut grouped: Vec<ObservationGroup> = Vec::new();
        for particle in &self.particles {
            let position = particle.private_position();
            let (entry, next) = observe_transition(
                self.public_history.current(),
                position,
                action.clone(),
                stepper,
                max_engine_steps,
            )?;
            let boundary = entry.boundary;
            let complete = entry.draw_order_trace_complete;
            let visible_changes = entry.draw_pile_changes;
            let successor = particle.successor(next);
            if let Some(group) = grouped
                .iter_mut()
                .find(|group| group.boundary == boundary && group.changes == visible_changes)
            {
                // Internal coverage differences never split an information set.
                group.complete &= complete;
                group.particles.push(successor);
            } else {
                grouped.push(ObservationGroup {
                    boundary,
                    changes: visible_changes,
                    complete,
                    particles: vec![successor],
                });
            }
        }

        if grouped.iter().any(|group| !group.complete) {
            // An incomplete trace is not evidence that an event did not occur.
            // Use only final public observations for this whole expansion;
            // coverage of a private path must not become a branch signal.
            let mut coarse: Vec<ObservationGroup> = Vec::new();
            for group in grouped {
                if let Some(retained) = coarse.iter_mut().find(|g| g.boundary == group.boundary) {
                    retained.particles.extend(group.particles);
                } else {
                    coarse.push(ObservationGroup {
                        boundary: group.boundary,
                        changes: Vec::new(),
                        complete: false,
                        particles: group.particles,
                    });
                }
            }
            grouped = coarse;
        }

        grouped
            .into_iter()
            .map(
                |ObservationGroup {
                     boundary,
                     changes,
                     complete,
                     mut particles,
                 }| {
                    let probability = particles
                        .iter()
                        .map(CombatBeliefParticleV1::probability_mass)
                        .sum::<f64>();
                    for particle in &mut particles {
                        particle.set_probability_mass(particle.probability_mass() / probability);
                    }
                    let history =
                        self.public_history
                            .successor(action.clone(), boundary, changes, complete);
                    Ok(CombatBeliefChanceBranchV1 {
                        probability,
                        environment: CombatBeliefEnvironmentV1::from_particles(history, particles)?,
                    })
                },
            )
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct CombatBeliefChanceBranchV1 {
    probability: f64,
    environment: CombatBeliefEnvironmentV1,
}

impl CombatBeliefChanceBranchV1 {
    /// Match independently observed evidence, never private world identity.
    /// A coarse branch may deliberately ignore extra observed draw detail.
    pub fn matches_observation(&self, observation: &CombatPublicHistoryEntryV1) -> bool {
        let expected = self.environment.public_history.entries.last().unwrap();
        expected.boundary == observation.boundary
            && (!expected.draw_order_trace_complete
                || (observation.draw_order_trace_complete
                    && expected.draw_pile_changes == observation.draw_pile_changes))
    }

    pub fn probability(&self) -> f64 {
        self.probability
    }

    pub fn environment(&self) -> &CombatBeliefEnvironmentV1 {
        &self.environment
    }

    pub fn into_environment(self) -> CombatBeliefEnvironmentV1 {
        self.environment
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CombatBeliefEnvironmentErrorV1 {
    DrawKnowledge(DrawKnowledgeError),
    ParticleKnowledgeMismatch,
    ObservationUnsupported,
    EmptyPopulation,
    InvalidProbabilityMass,
    TerminalRoot,
    TerminalBoundary,
    ParticleBoundaryMismatch,
    ActionProjection(String),
    ActionResolution(CombatActionResolutionErrorV1),
    ResolvedActionIllegal,
    TransitionTruncated,
    Sampling(CombatBeliefSamplingErrorV1),
}

impl Display for CombatBeliefEnvironmentErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DrawKnowledge(error) => write!(formatter, "invalid draw knowledge: {error:?}"),
            Self::ParticleKnowledgeMismatch => {
                formatter.write_str("particle contradicts observed draw order knowledge")
            }
            Self::ObservationUnsupported => {
                formatter.write_str("stepper does not provide transition observations")
            }
            Self::EmptyPopulation => formatter.write_str("combat belief population is empty"),
            Self::InvalidProbabilityMass => {
                formatter.write_str("combat belief particle mass must be finite and positive")
            }
            Self::TerminalRoot => formatter.write_str("combat belief root is terminal"),
            Self::TerminalBoundary => formatter.write_str("combat belief boundary is terminal"),
            Self::ParticleBoundaryMismatch => {
                formatter.write_str("combat belief particle does not match its public history")
            }
            Self::ActionProjection(error) => {
                write!(formatter, "cannot project particle actions: {error}")
            }
            Self::ActionResolution(error) => write!(formatter, "cannot resolve action: {error}"),
            Self::ResolvedActionIllegal => {
                formatter.write_str("resolved public action is not exact-engine legal")
            }
            Self::TransitionTruncated => {
                formatter.write_str("combat belief transition did not reach a stable boundary")
            }
            Self::Sampling(error) => write!(formatter, "cannot sample belief particles: {error}"),
        }
    }
}

impl Error for CombatBeliefEnvironmentErrorV1 {}

pub(super) fn public_boundary_for_position_v1(
    run_context: PublicCombatRunContextV1,
    position: &CombatPosition,
) -> Result<CombatPublicBoundaryV1, CombatBeliefEnvironmentErrorV1> {
    let observation = public_combat_state_v1(&position.combat);
    let outcome = crate::sim::combat::combat_terminal(&position.engine, &position.combat);
    if outcome != CombatTerminal::Unresolved {
        return Ok(CombatPublicBoundaryV1::Terminal {
            run_context,
            observation,
            outcome,
        });
    }
    let actions = project_public_combat_actions_v1(&position.engine, &position.combat)
        .map_err(|error| CombatBeliefEnvironmentErrorV1::ActionProjection(error.to_string()))?
        .public;
    Ok(CombatPublicBoundaryV1::Decision {
        decision: CombatPublicDecisionV1 {
            run_context,
            observation,
            actions,
        },
    })
}

fn boundary_observation(boundary: &CombatPublicBoundaryV1) -> &PublicCombatStateV1 {
    match boundary {
        CombatPublicBoundaryV1::Decision { decision } => &decision.observation,
        CombatPublicBoundaryV1::Terminal { observation, .. } => observation,
    }
}

pub(super) fn current_run_context_v1(
    boundary: &CombatPublicBoundaryV1,
) -> &PublicCombatRunContextV1 {
    match boundary {
        CombatPublicBoundaryV1::Decision { decision } => &decision.run_context,
        CombatPublicBoundaryV1::Terminal { run_context, .. } => run_context,
    }
}

fn observe_transition(
    current: &CombatPublicBoundaryV1,
    position: &CombatPosition,
    action: PublicCombatActionChoiceV1,
    stepper: &impl CombatStepper,
    max_engine_steps: usize,
) -> Result<(CombatPublicHistoryEntryV1, CombatPosition), CombatBeliefEnvironmentErrorV1> {
    let CombatPublicBoundaryV1::Decision { decision } = current else {
        return Err(CombatBeliefEnvironmentErrorV1::TerminalBoundary);
    };
    let projection = project_public_combat_actions_v1(&position.engine, &position.combat)
        .map_err(|e| CombatBeliefEnvironmentErrorV1::ActionProjection(e.to_string()))?;
    if projection.public != decision.actions {
        return Err(CombatBeliefEnvironmentErrorV1::ParticleBoundaryMismatch);
    }
    let input = resolve_public_combat_action_v1(&projection, &action)
        .map_err(CombatBeliefEnvironmentErrorV1::ActionResolution)?;
    if !stepper.is_legal_action(position, &input) {
        return Err(CombatBeliefEnvironmentErrorV1::ResolvedActionIllegal);
    }
    let observed = stepper
        .apply_to_stable_observed(
            position,
            input,
            CombatStepLimits {
                max_engine_steps,
                deadline: None,
            },
        )
        .ok_or(CombatBeliefEnvironmentErrorV1::ObservationUnsupported)?;
    if observed.step.truncated || observed.step.timed_out {
        return Err(CombatBeliefEnvironmentErrorV1::TransitionTruncated);
    }
    let complete = !observed
        .draw_pile_changes
        .iter()
        .any(|c| matches!(c, DrawPileChange::OrderUnknown { .. }));
    let boundary =
        public_boundary_for_position_v1(decision.run_context.clone(), &observed.step.position)?;
    Ok((
        CombatPublicHistoryEntryV1 {
            chosen_action: Some(action),
            boundary,
            draw_pile_changes: if complete {
                observed.draw_pile_changes
            } else {
                vec![]
            },
            draw_order_trace_complete: complete,
        },
        observed.step.position,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::belief::CombatBeliefParticleOriginV1;
    use crate::agent::belief::{
        CombatBeliefConditioningV1, CombatBeliefSamplerV1, IndependentStreamsCombatBeliefSamplerV1,
    };
    use crate::agent::information::action::PublicCombatAtomicActionV1;
    use crate::agent::information::run::PublicCombatRunContextGapV1;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::potions::{Potion, PotionId};
    use crate::runtime::combat::{CombatCard, Intent};
    use crate::sim::combat::EngineCombatStepper;
    use crate::state::core::EngineState;

    fn two_orders(hand: CardId, draw: &[CardId]) -> CombatBeliefEnvironmentV1 {
        let mut left = decision_combat();
        left.zones.hand = vec![CombatCard::for_scenario(hand, 10)];
        left.zones.draw_pile = draw
            .iter()
            .enumerate()
            .map(|(index, id)| CombatCard::for_scenario(*id, index as u32 + 20))
            .collect();
        left.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Carnage, 40)].into();
        let mut right = left.clone();
        right.zones.draw_pile.swap(0, 1);
        let left = CombatPosition::new(EngineState::CombatPlayerTurn, left);
        let right = CombatPosition::new(EngineState::CombatPlayerTurn, right);
        let history = CombatPublicHistoryV1::root(
            public_boundary_for_position_v1(detached_context(), &left).unwrap(),
        );
        CombatBeliefEnvironmentV1::from_particles(
            history,
            vec![particle(1, left), particle(2, right)],
        )
        .unwrap()
    }

    fn play_first(environment: &CombatBeliefEnvironmentV1) -> PublicCombatActionChoiceV1 {
        let CombatPublicBoundaryV1::Decision { decision } = environment.public_history().current()
        else {
            panic!()
        };
        let action_ordinal = decision
            .actions
            .atomic_actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    PublicCombatAtomicActionV1::PlayCard { hand_index: 0, .. }
                )
            })
            .unwrap();
        PublicCombatActionChoiceV1::Atomic { action_ordinal }
    }

    #[test]
    fn headbutt_knowledge_constrains_futures_and_cannot_be_forgotten_by_resampling() {
        let environment = two_orders(CardId::Headbutt, &[CardId::Strike, CardId::Defend]);
        let mut branches = environment
            .step(play_first(&environment), &EngineCombatStepper, 512)
            .unwrap();
        assert_eq!(
            branches.len(),
            1,
            "unobserved remaining draw order does not split the decision"
        );
        let branch = branches.pop().unwrap().into_environment();
        assert_eq!(
            branch
                .public_history
                .draw_order_knowledge()
                .unwrap()
                .known_prefix(),
            [DrawCardKind {
                card_id: CardId::Carnage,
                upgrades: 0
            }]
        );
        let mut impossible = branch.particles[0].private_position().clone();
        impossible.combat.zones.draw_pile.swap(0, 1);
        assert_eq!(
            CombatBeliefEnvironmentV1::from_particles(
                branch.public_history.clone(),
                vec![particle(3, impossible)]
            )
            .unwrap_err(),
            CombatBeliefEnvironmentErrorV1::ParticleKnowledgeMismatch
        );
        let sampler = IndependentStreamsCombatBeliefSamplerV1::new(vec![101, 202]);
        assert_eq!(
            CombatBeliefEnvironmentV1::from_history_and_sampler(
                branch.public_history.clone(),
                branch.particles[0].private_position(),
                &sampler
            )
            .unwrap_err(),
            CombatBeliefEnvironmentErrorV1::Sampling(
                CombatBeliefSamplingErrorV1::RememberedDrawOrderUnsupported
            )
        );
    }

    #[test]
    fn remembered_sampler_supports_continued_draws_without_revealing_unknown_order() {
        let initial = two_orders(CardId::Headbutt, &[CardId::Strike, CardId::Defend]);
        let particles = initial
            .particles
            .into_iter()
            .map(|particle| {
                let origin = particle.origin();
                let mut position = particle.into_private_position();
                position.combat.zones.hand.extend([
                    CombatCard::for_scenario(CardId::ShrugItOff, 50),
                    CombatCard::for_scenario(CardId::ShrugItOff, 51),
                ]);
                CombatBeliefParticleV1::from_private_position(origin, 0.5, position)
            })
            .collect::<Vec<_>>();
        let history = CombatPublicHistoryV1::root(
            public_boundary_for_position_v1(detached_context(), particles[0].private_position())
                .unwrap(),
        );
        let initial = CombatBeliefEnvironmentV1::from_particles(history, particles).unwrap();
        let mut branches = initial
            .step(play_first(&initial), &EngineCombatStepper, 512)
            .unwrap();
        assert_eq!(branches.len(), 1);
        let after_headbutt = branches.pop().unwrap().into_environment();
        let source = after_headbutt.particles[0].private_position();
        let source_before = source.clone();
        let sampler = IndependentStreamsCombatBeliefSamplerV1::new((1..=64).collect())
            .with_draw_order_knowledge(1_000);
        assert_eq!(
            sampler.conditioning(),
            CombatBeliefConditioningV1::PublicBoundaryAndRememberedDrawOrder
        );
        let replenished = CombatBeliefEnvironmentV1::from_history_and_sampler(
            after_headbutt.public_history.clone(),
            source,
            &sampler,
        )
        .unwrap();
        assert_eq!(
            *source, source_before,
            "sampling never changes the actual combat"
        );
        let mut alternate_source = source.clone();
        alternate_source.combat.zones.draw_pile.swap(1, 2);
        let alternate = CombatBeliefEnvironmentV1::from_history_and_sampler(
            after_headbutt.public_history.clone(),
            &alternate_source,
            &sampler,
        )
        .unwrap();
        for (left, right) in replenished.particles.iter().zip(&alternate.particles) {
            assert!(matches!(
                left.origin(),
                CombatBeliefParticleOriginV1::DrawOrderConditionedIndependentStreams { .. }
            ));
            assert_eq!(
                left.private_position().combat.zones.draw_pile,
                right.private_position().combat.zones.draw_pile,
                "the source's hidden permutation must not control sampled futures"
            );
        }
        // The first Shrug draws the known Headbutt card. No new order fact is
        // revealed, so this remains one decision shared by all particles.
        let mut branches = replenished
            .step(play_first(&replenished), &EngineCombatStepper, 512)
            .unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].probability(), 1.0);
        let after_known_draw = branches.pop().unwrap().into_environment();
        assert_eq!(
            after_known_draw.particles[0]
                .private_position()
                .combat
                .zones
                .hand
                .last()
                .unwrap()
                .id,
            CardId::Carnage
        );
        // The next Shrug now reveals an unknown card; only here may decisions
        // diverge. Both branches retain consistent histories and live states.
        let branches = after_known_draw
            .step(play_first(&after_known_draw), &EngineCombatStepper, 512)
            .unwrap();
        assert_eq!(branches.len(), 2);
        assert!((branches.iter().map(|b| b.probability()).sum::<f64>() - 1.0).abs() < 1e-12);
        for branch in branches {
            let environment = branch.into_environment();
            let knowledge = environment.public_history.draw_order_knowledge().unwrap();
            assert_eq!(knowledge.composition().len(), 1);
            let continued = CombatBeliefEnvironmentV1::from_history_and_sampler(
                environment.public_history.clone(),
                environment.particles[0].private_position(),
                &sampler,
            )
            .unwrap();
            assert_eq!(continued.particles.len(), 64);
        }
    }

    #[test]
    fn kind_only_memory_does_not_guess_which_card_variant_was_put_back() {
        let initial = two_orders(CardId::Headbutt, &[CardId::Carnage, CardId::Defend]);
        let mut source = initial.particles[0].private_position().clone();
        source.combat.zones.discard_pile[0].cost_modifier = -1;
        let sampler =
            IndependentStreamsCombatBeliefSamplerV1::new(vec![1, 2]).with_draw_order_knowledge(100);
        let initial =
            CombatBeliefEnvironmentV1::from_sampler(detached_context(), &source, &sampler).unwrap();
        let mut branches = initial
            .step(play_first(&initial), &EngineCombatStepper, 512)
            .unwrap();
        assert_eq!(branches.len(), 1);
        let after = branches.pop().unwrap().into_environment();
        assert_eq!(
            CombatBeliefEnvironmentV1::from_history_and_sampler(
                after.public_history.clone(),
                after.particles[0].private_position(),
                &sampler
            )
            .unwrap_err(),
            CombatBeliefEnvironmentErrorV1::Sampling(
                CombatBeliefSamplingErrorV1::RememberedCardVariantsUnsupported
            )
        );
    }

    #[test]
    fn conditioned_sampler_preserves_full_frozen_eye_information() {
        use crate::content::relics::{RelicId, RelicState};
        let initial = two_orders(CardId::ShrugItOff, &[CardId::Strike, CardId::Strike]);
        let mut source = initial.particles[0].private_position().clone();
        source
            .combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::FrozenEye));
        source
            .combat
            .zones
            .draw_pile
            .iter_mut()
            .next()
            .unwrap()
            .cost_modifier = -1;
        let sampler = IndependentStreamsCombatBeliefSamplerV1::new(vec![1, 2, 3])
            .with_draw_order_knowledge(0);
        let environment =
            CombatBeliefEnvironmentV1::from_sampler(detached_context(), &source, &sampler).unwrap();
        for particle in environment.particles() {
            assert_eq!(
                particle.private_position().combat.zones.draw_pile,
                source.combat.zones.draw_pile
            );
        }
        let branches = environment
            .step(play_first(&environment), &EngineCombatStepper, 512)
            .unwrap();
        assert_eq!(branches.len(), 1);
        assert_eq!(
            branches[0].environment.particles[0]
                .private_position()
                .combat
                .zones
                .hand[0]
                .cost_modifier,
            -1
        );
    }

    #[test]
    fn intermediate_visible_draws_split_equal_final_snapshots() {
        let environment = two_orders(
            CardId::BattleTrance,
            &[CardId::DeusExMachina, CardId::Defend, CardId::Bash],
        );
        let branches = environment
            .step(play_first(&environment), &EngineCombatStepper, 512)
            .unwrap();
        assert_eq!(branches.len(), 2);
        assert_eq!(
            branches[0].environment.public_history.current(),
            branches[1].environment.public_history.current()
        );
        let left = branches[0]
            .environment
            .public_history
            .entries
            .last()
            .unwrap();
        let right = branches[1]
            .environment
            .public_history
            .entries
            .last()
            .unwrap();
        assert_ne!(left.draw_pile_changes, right.draw_pile_changes);
        assert!(left.draw_order_trace_complete && right.draw_order_trace_complete);
    }

    #[test]
    fn internal_coverage_markers_never_identify_a_hidden_world() {
        struct PartialObserver;
        impl CombatStepper for PartialObserver {
            fn atomic_actions(&self, p: &CombatPosition) -> Vec<crate::state::core::ClientInput> {
                EngineCombatStepper.atomic_actions(p)
            }
            fn apply_to_stable(
                &self,
                p: &CombatPosition,
                a: crate::state::core::ClientInput,
                l: CombatStepLimits,
            ) -> crate::sim::combat::CombatStepResult {
                EngineCombatStepper.apply_to_stable(p, a, l)
            }
            fn terminal(&self, p: &CombatPosition) -> CombatTerminal {
                EngineCombatStepper.terminal(p)
            }
            fn apply_to_stable_observed(
                &self,
                p: &CombatPosition,
                a: crate::state::core::ClientInput,
                l: CombatStepLimits,
            ) -> Option<crate::sim::combat::CombatObservedStepResultV1> {
                let mut result = EngineCombatStepper.apply_to_stable_observed(p, a, l)?;
                if p.combat.zones.draw_pile[0].id == CardId::Strike {
                    result.draw_pile_changes.push(DrawPileChange::OrderUnknown {
                        count: result.step.position.combat.zones.draw_pile.len(),
                    });
                }
                Some(result)
            }
        }
        let environment = two_orders(CardId::Headbutt, &[CardId::Strike, CardId::Defend]);
        let branches = environment
            .step(play_first(&environment), &PartialObserver, 512)
            .unwrap();
        assert_eq!(branches.len(), 1);
        let branch = branches[0].environment();
        assert_eq!(branch.particles.len(), 2);
        assert!(branch
            .public_history
            .draw_order_knowledge()
            .unwrap()
            .known_prefix()
            .is_empty());
        assert!(branch
            .public_history
            .entries
            .last()
            .unwrap()
            .draw_pile_changes
            .is_empty());
    }

    #[test]
    fn one_public_action_resolves_against_each_particles_private_handles() {
        let left = potion_combat(7, 71);
        let mut right = potion_combat(700, 7100);
        right.entities.monsters[0].current_hp = left.entities.monsters[0].current_hp;
        right.entities.monsters[0].max_hp = left.entities.monsters[0].max_hp;
        let left_position = CombatPosition::new(EngineState::CombatPlayerTurn, left);
        let right_position = CombatPosition::new(EngineState::CombatPlayerTurn, right);
        let history = CombatPublicHistoryV1::root(
            public_boundary_for_position_v1(detached_context(), &left_position).unwrap(),
        );
        let environment = CombatBeliefEnvironmentV1::from_particles(
            history,
            vec![particle(1, left_position), particle(2, right_position)],
        )
        .unwrap();
        let CombatPublicBoundaryV1::Decision { decision } = environment.public_history().current()
        else {
            panic!("root must be a decision");
        };
        let action_ordinal = decision
            .actions
            .atomic_actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    PublicCombatAtomicActionV1::UsePotion {
                        potion_index: 0,
                        target_monster_index: Some(0)
                    }
                )
            })
            .expect("public fire-potion action");

        let branches = environment
            .step(
                PublicCombatActionChoiceV1::Atomic { action_ordinal },
                &EngineCombatStepper,
                100,
            )
            .expect("step every particle");

        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].probability(), 1.0);
        assert_eq!(branches[0].environment().particles().len(), 2);
        for particle in branches[0].environment().particles() {
            assert!(particle.private_position().combat.entities.potions[0].is_none());
            assert!(particle.private_position().combat.entities.monsters[0].current_hp <= 0);
        }
    }

    #[test]
    fn visible_successors_split_particles_into_conditional_chance_branches() {
        let mut left = decision_combat();
        left.zones.hand = vec![CombatCard::for_scenario(CardId::BattleTrance, 10)];
        left.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Strike, 21),
            CombatCard::for_scenario(CardId::Defend, 22),
            CombatCard::for_scenario(CardId::Bash, 23),
        ]
        .into();
        let mut right = left.clone();
        right.zones.hand[0].lineage = crate::test_support::scenario_lineage(100);
        right.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Bash, 230),
            CombatCard::for_scenario(CardId::Strike, 210),
            CombatCard::for_scenario(CardId::Defend, 220),
        ]
        .into();
        let left_position = CombatPosition::new(EngineState::CombatPlayerTurn, left);
        let right_position = CombatPosition::new(EngineState::CombatPlayerTurn, right);
        let history = CombatPublicHistoryV1::root(
            public_boundary_for_position_v1(detached_context(), &left_position).unwrap(),
        );
        let environment = CombatBeliefEnvironmentV1::from_particles(
            history,
            vec![particle(11, left_position), particle(22, right_position)],
        )
        .unwrap();
        let CombatPublicBoundaryV1::Decision { decision } = environment.public_history().current()
        else {
            panic!("root must be a decision");
        };
        let action_ordinal = decision
            .actions
            .atomic_actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    PublicCombatAtomicActionV1::PlayCard {
                        hand_index: 0,
                        target_monster_index: None
                    }
                )
            })
            .expect("public Battle Trance action");

        let branches = environment
            .step(
                PublicCombatActionChoiceV1::Atomic { action_ordinal },
                &EngineCombatStepper,
                100,
            )
            .expect("step every particle");

        assert_eq!(branches.len(), 2);
        assert!(branches.iter().all(|branch| branch.probability() == 0.5));
        assert!(branches
            .iter()
            .all(|branch| branch.environment().particles().len() == 1));
        assert!(branches
            .iter()
            .all(|branch| { branch.environment().public_history().entries().len() == 2 }));
        let hands = branches
            .iter()
            .map(
                |branch| match branch.environment().public_history().current() {
                    CombatPublicBoundaryV1::Decision { decision } => decision
                        .observation
                        .cards
                        .hand
                        .cards
                        .iter()
                        .map(|card| card.card_id)
                        .collect::<Vec<_>>(),
                    CombatPublicBoundaryV1::Terminal { .. } => Vec::new(),
                },
            )
            .collect::<Vec<_>>();
        assert_ne!(hands[0], hands[1]);
    }

    #[test]
    fn declared_sampler_builds_a_normalized_environment_at_the_public_root() {
        let mut combat = decision_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 10)];
        combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Strike, 21),
            CombatCard::for_scenario(CardId::Defend, 22),
        ]
        .into();
        let source = CombatPosition::new(EngineState::CombatPlayerTurn, combat);
        let sampler = IndependentStreamsCombatBeliefSamplerV1::new(vec![101, 202]);

        let environment =
            CombatBeliefEnvironmentV1::from_sampler(detached_context(), &source, &sampler)
                .expect("build belief environment");

        assert_eq!(
            sampler.conditioning(),
            CombatBeliefConditioningV1::CurrentPublicBoundaryOnly
        );
        assert_eq!(environment.public_history().entries().len(), 1);
        assert_eq!(environment.particles().len(), 2);
        assert_eq!(
            environment
                .particles()
                .iter()
                .map(CombatBeliefParticleV1::probability_mass)
                .sum::<f64>(),
            1.0
        );
    }

    fn particle(seed: u64, position: CombatPosition) -> CombatBeliefParticleV1 {
        CombatBeliefParticleV1::from_private_position(
            CombatBeliefParticleOriginV1::IndependentStreams {
                particle_seed: seed,
            },
            0.5,
            position,
        )
    }

    fn detached_context() -> PublicCombatRunContextV1 {
        PublicCombatRunContextV1::Unavailable {
            reason: PublicCombatRunContextGapV1::DetachedExactCombatPosition,
        }
    }

    fn potion_combat(monster_id: usize, potion_uuid: u32) -> crate::runtime::combat::CombatState {
        let mut combat = decision_combat();
        combat.entities.monsters[0].id = monster_id;
        combat.set_monster_protocol_visible_intent(
            monster_id,
            Intent::Attack {
                damage: 11,
                hits: 1,
            },
        );
        combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, potion_uuid))];
        combat
    }

    fn decision_combat() -> crate::runtime::combat::CombatState {
        let mut combat = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.slot = 0;
        combat.entities.monsters.push(monster);
        combat.set_monster_protocol_visible_intent(
            7,
            Intent::Attack {
                damage: 11,
                hits: 1,
            },
        );
        combat
    }
}
