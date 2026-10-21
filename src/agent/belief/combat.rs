use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use crate::agent::information::action::project_public_combat_actions_v1;
use crate::agent::information::combat::{combat_public_observation_v2, ObservationEvidenceKindV1};
use crate::agent::information::draw_order::DrawOrderKnowledge;
use crate::agent::information::state::learning_card;
use crate::runtime::combat::{CombatState, DrawCardKind};
use crate::runtime::rng::StsRng;
use crate::sim::combat::CombatPosition;
use crate::state::core::EngineState;

use super::draw_order::DrawOrderDistribution;
use super::environment::{CombatPublicBoundaryV1, CombatPublicHistoryV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatBeliefConditioningV1 {
    /// Conditions on the current public boundary only. Earlier public history
    /// is accepted for identity and future replacement, but is not used to
    /// reconstruct a run-seed posterior.
    CurrentPublicBoundaryOnly,
    /// Uniform kind orders satisfying current facts and remembered order.
    /// Does not condition on history likelihood or historical RNG streams.
    PublicBoundaryAndRememberedDrawOrder,
}

pub struct CombatBeliefSamplingRequestV1<'a> {
    pub public_history: &'a CombatPublicHistoryV1,
    pub exact_source: &'a CombatPosition,
}

pub trait CombatBeliefSamplerV1 {
    fn conditioning(&self) -> CombatBeliefConditioningV1;

    fn sample(
        &self,
        request: CombatBeliefSamplingRequestV1<'_>,
    ) -> Result<Vec<CombatBeliefParticleV1>, CombatBeliefSamplingErrorV1>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndependentStreamsCombatBeliefSamplerV1 {
    particle_seeds: Vec<u64>,
    draw_order_state_budget: Option<usize>,
}

impl IndependentStreamsCombatBeliefSamplerV1 {
    pub fn new(particle_seeds: Vec<u64>) -> Self {
        Self {
            particle_seeds,
            draw_order_state_budget: None,
        }
    }

    /// Opt into a uniform-order assumption constrained by remembered facts.
    /// The shared conditioning calculation is bounded; exhaustion returns an
    /// error, never unconstrained samples or a claim of impossible futures.
    pub fn with_draw_order_knowledge(mut self, max_states: usize) -> Self {
        self.draw_order_state_budget = Some(max_states);
        self
    }
}

/// Provenance for one sampled private combat future.
///
/// `IndependentStreams` is deliberately not called a posterior: it preserves
/// the public boundary while independently replacing hidden stream state. A
/// seed-consistent run-history sampler can add another origin without changing
/// the particle consumer. `DrawOrderConditionedIndependentStreams` adds only
/// order constraints, not the likelihood of obtaining the observed history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CombatBeliefParticleOriginV1 {
    IndependentStreams {
        particle_seed: u64,
    },
    DrawOrderConditionedIndependentStreams {
        particle_seed: u64,
    },
    /// A correlated, history-compatible proposal; not a new posterior sample.
    HistoryReplayedMutation {
        proposal_seed: u64,
    },
    /// A complete fixed proposal schedule was checked against a new public
    /// observation; its individual accepted moves are recorded by the run
    /// receipt, not represented as one root-mutation seed.
    ObservedSuccessorRescue {
        schedule_seed: u64,
        donor_index: usize,
        attempt_index: usize,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CombatBeliefParticleV1 {
    origin: CombatBeliefParticleOriginV1,
    probability_mass: f64,
    private_position: CombatPosition,
    // Present only for samples made at the recorded root, then propagated by
    // real transitions. A sample at a later boundary is not a replay root.
    pub(super) reconstruction_root: Option<Arc<CombatPosition>>,
}

impl CombatBeliefParticleV1 {
    pub fn origin(&self) -> CombatBeliefParticleOriginV1 {
        self.origin
    }

    pub fn probability_mass(&self) -> f64 {
        self.probability_mass
    }

    pub fn private_position(&self) -> &CombatPosition {
        &self.private_position
    }

    pub fn into_private_position(self) -> CombatPosition {
        self.private_position
    }

    pub(super) fn from_private_position(
        origin: CombatBeliefParticleOriginV1,
        probability_mass: f64,
        private_position: CombatPosition,
    ) -> Self {
        Self {
            origin,
            probability_mass,
            private_position,
            reconstruction_root: None,
        }
    }

    pub(super) fn successor(&self, position: CombatPosition) -> Self {
        Self {
            origin: self.origin,
            probability_mass: self.probability_mass,
            private_position: position,
            reconstruction_root: self.reconstruction_root.clone(),
        }
    }

    pub(super) fn set_probability_mass(&mut self, probability_mass: f64) {
        self.probability_mass = probability_mass;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CombatBeliefSamplingErrorV1 {
    EmptyPopulation,
    DuplicateParticleSeed(u64),
    HiddenCurrentIntent,
    HiddenDrawMultiset,
    DrawPileTooLarge,
    PublicBoundaryChanged { particle_seed: u64 },
    HistoryBoundaryMismatch,
    RememberedDrawOrderUnsupported,
    RememberedCardVariantsUnsupported,
    DrawOrderConditioningBudgetExceeded { max_states: usize },
    DrawOrderConditioningFailed,
    ActionProjection(String),
}

impl Display for CombatBeliefSamplingErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RememberedCardVariantsUnsupported => formatter.write_str(
                "kind-only draw memory cannot condition distinct variants of the same card kind",
            ),
            Self::DrawOrderConditioningBudgetExceeded { max_states } => write!(
                formatter,
                "draw-order conditioning exceeded {max_states} states; possible futures remain unknown"
            ),
            Self::DrawOrderConditioningFailed => formatter.write_str(
                "draw-order conditioning could not construct a finite positive distribution",
            ),
            Self::RememberedDrawOrderUnsupported => {
                formatter.write_str("current-boundary sampler cannot discard remembered draw order")
            }
            Self::EmptyPopulation => {
                formatter.write_str("combat belief population requires at least one seed")
            }
            Self::DuplicateParticleSeed(seed) => {
                write!(
                    formatter,
                    "combat belief population repeats particle seed {seed}"
                )
            }
            Self::HiddenCurrentIntent => formatter
                .write_str("combat belief sampling does not yet support a hidden current intent"),
            Self::HiddenDrawMultiset => formatter
                .write_str("combat belief sampling cannot condition an unobserved draw multiset"),
            Self::DrawPileTooLarge => {
                formatter.write_str("combat draw pile is too large for belief sampling")
            }
            Self::PublicBoundaryChanged { particle_seed } => write!(
                formatter,
                "combat belief particle {particle_seed} changed public observation or legal actions"
            ),
            Self::HistoryBoundaryMismatch => formatter.write_str(
                "combat belief sampling source does not match the current public history boundary",
            ),
            Self::ActionProjection(error) => {
                write!(formatter, "cannot project public combat actions: {error}")
            }
        }
    }
}

impl Error for CombatBeliefSamplingErrorV1 {}

impl CombatBeliefSamplerV1 for IndependentStreamsCombatBeliefSamplerV1 {
    fn conditioning(&self) -> CombatBeliefConditioningV1 {
        if self.draw_order_state_budget.is_some() {
            CombatBeliefConditioningV1::PublicBoundaryAndRememberedDrawOrder
        } else {
            CombatBeliefConditioningV1::CurrentPublicBoundaryOnly
        }
    }

    fn sample(
        &self,
        request: CombatBeliefSamplingRequestV1<'_>,
    ) -> Result<Vec<CombatBeliefParticleV1>, CombatBeliefSamplingErrorV1> {
        let CombatPublicBoundaryV1::Decision { decision } = request.public_history.current() else {
            return Err(CombatBeliefSamplingErrorV1::HistoryBoundaryMismatch);
        };
        let remembered = request
            .public_history
            .draw_order_knowledge()
            .map_err(|_| CombatBeliefSamplingErrorV1::HistoryBoundaryMismatch)?;
        let snapshot_only =
            crate::agent::information::draw_order::DrawOrderKnowledge::from_observation(
                &decision.observation,
            );
        if self.draw_order_state_budget.is_none() && remembered != snapshot_only {
            return Err(CombatBeliefSamplingErrorV1::RememberedDrawOrderUnsupported);
        }
        let exact_observation =
            crate::agent::information::state::public_combat_state_v1(&request.exact_source.combat);
        let exact_actions = project_public_combat_actions_v1(
            &request.exact_source.engine,
            &request.exact_source.combat,
        )
        .map_err(|error| CombatBeliefSamplingErrorV1::ActionProjection(error.to_string()))?
        .public;
        if decision.observation != exact_observation || decision.actions != exact_actions {
            return Err(CombatBeliefSamplingErrorV1::HistoryBoundaryMismatch);
        }
        let mut distribution = if let Some(max_states) = self.draw_order_state_budget {
            if decision.observation.cards.draw.evidence
                == ObservationEvidenceKindV1::PublicUnorderedCollection
            {
                validate_remembered_card_variants(&request.exact_source.combat, &remembered)?;
            }
            Some(DrawOrderDistribution::new(&remembered, max_states)?)
        } else {
            None
        };
        let mut particles = sample_combat_futures(
            &request.exact_source.engine,
            &request.exact_source.combat,
            &self.particle_seeds,
            distribution.as_mut(),
        )?;
        if request.public_history.entries().len() == 1 {
            for particle in &mut particles {
                particle.reconstruction_root = Some(Arc::new(particle.private_position.clone()));
            }
        }
        Ok(particles)
    }
}

/// Sample exact private combat futures that share one current public boundary.
///
/// This is the mechanics-level independent-stream sampler. It is useful for
/// exercising information-set consumers, but it is not a posterior over
/// complete run seeds: hidden draw order and future RNG streams are replaced
/// independently. The current intent must already be public. Potion inventory
/// and potion actions are preserved from the source boundary.
pub fn sample_independent_combat_futures_v1(
    engine: &EngineState,
    source: &CombatState,
    particle_seeds: &[u64],
) -> Result<Vec<CombatBeliefParticleV1>, CombatBeliefSamplingErrorV1> {
    sample_combat_futures(engine, source, particle_seeds, None)
}

fn sample_combat_futures(
    engine: &EngineState,
    source: &CombatState,
    particle_seeds: &[u64],
    mut distribution: Option<&mut DrawOrderDistribution>,
) -> Result<Vec<CombatBeliefParticleV1>, CombatBeliefSamplingErrorV1> {
    if particle_seeds.is_empty() {
        return Err(CombatBeliefSamplingErrorV1::EmptyPopulation);
    }
    let mut distinct = BTreeSet::new();
    if let Some(seed) = particle_seeds
        .iter()
        .copied()
        .find(|seed| !distinct.insert(*seed))
    {
        return Err(CombatBeliefSamplingErrorV1::DuplicateParticleSeed(seed));
    }

    let source_observation = combat_public_observation_v2(source);
    if source_observation
        .monsters
        .iter()
        .any(|monster| monster.intent.evidence != ObservationEvidenceKindV1::VisibleExact)
    {
        return Err(CombatBeliefSamplingErrorV1::HiddenCurrentIntent);
    }
    let source_actions = project_public_combat_actions_v1(engine, source)
        .map_err(|error| CombatBeliefSamplingErrorV1::ActionProjection(error.to_string()))?
        .public;
    let draw_evidence = source_observation.piles.draw.evidence;
    let probability_mass = 1.0 / particle_seeds.len() as f64;

    particle_seeds
        .iter()
        .copied()
        .map(|particle_seed| {
            let mut private_combat = source.clone();
            match draw_evidence {
                ObservationEvidenceKindV1::PublicUnorderedCollection => {
                    if let Some(distribution) = distribution.as_deref_mut() {
                        let mut rng = StsRng::new(chance_stream_seed_v1(particle_seed, 0));
                        let order = distribution.sample(&mut rng)?;
                        materialize_draw_order(&mut private_combat, &order, particle_seed)?;
                    } else {
                        shuffle_hidden_draw_order_v1(&mut private_combat, particle_seed)?;
                    }
                }
                ObservationEvidenceKindV1::PublicOrderedCollection
                | ObservationEvidenceKindV1::VisibleExact => {}
                ObservationEvidenceKindV1::Hidden => {
                    return Err(CombatBeliefSamplingErrorV1::HiddenDrawMultiset);
                }
            }
            reseed_hidden_combat_futures_v1(&mut private_combat, particle_seed);

            let sampled_observation = combat_public_observation_v2(&private_combat);
            let sampled_actions = project_public_combat_actions_v1(engine, &private_combat)
                .map_err(|error| CombatBeliefSamplingErrorV1::ActionProjection(error.to_string()))?
                .public;
            if sampled_observation != source_observation || sampled_actions != source_actions {
                return Err(CombatBeliefSamplingErrorV1::PublicBoundaryChanged { particle_seed });
            }

            Ok(CombatBeliefParticleV1 {
                origin: if distribution.is_some() {
                    CombatBeliefParticleOriginV1::DrawOrderConditionedIndependentStreams {
                        particle_seed,
                    }
                } else {
                    CombatBeliefParticleOriginV1::IndependentStreams { particle_seed }
                },
                probability_mass,
                private_position: CombatPosition::new(engine.clone(), private_combat),
                reconstruction_root: None,
            })
        })
        .collect()
}

fn validate_remembered_card_variants(
    source: &CombatState,
    knowledge: &DrawOrderKnowledge,
) -> Result<(), CombatBeliefSamplingErrorV1> {
    for kind in knowledge.ordered_subsequence() {
        let mut variants = source
            .zones
            .draw_pile
            .iter()
            .filter(|card| DrawCardKind::from(*card) == *kind)
            .map(|card| learning_card(source, card, false));
        if let Some(first) = variants.next() {
            if variants.any(|variant| variant != first) {
                return Err(CombatBeliefSamplingErrorV1::RememberedCardVariantsUnsupported);
            }
        }
    }
    Ok(())
}

fn materialize_draw_order(
    combat: &mut CombatState,
    order: &[DrawCardKind],
    particle_seed: u64,
) -> Result<(), CombatBeliefSamplingErrorV1> {
    // Canonicalize by public properties, never by hidden position or lineage.
    // Actual instances remain intact; equal-kind assignments are independently
    // randomized and do not become identity facts in the public knowledge.
    let mut cards = combat
        .zones
        .draw_pile
        .iter()
        .map(|card| (learning_card(combat, card, false), card.clone()))
        .collect::<Vec<_>>();
    cards.sort_by(|left, right| left.0.cmp(&right.0));
    let mut rng = StsRng::new(chance_stream_seed_v1(particle_seed, 9));
    let mut drawn = Vec::with_capacity(order.len());
    for kind in order {
        let matches = cards
            .iter()
            .enumerate()
            .filter_map(|(index, (_, card))| (DrawCardKind::from(card) == *kind).then_some(index))
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return Err(CombatBeliefSamplingErrorV1::HistoryBoundaryMismatch);
        }
        let index = matches[rng.random(matches.len() as i32 - 1) as usize];
        drawn.push(cards.swap_remove(index).1);
    }
    if !cards.is_empty() {
        return Err(CombatBeliefSamplingErrorV1::HistoryBoundaryMismatch);
    }
    combat.zones.draw_pile = drawn.into();
    Ok(())
}

fn shuffle_hidden_draw_order_v1(
    combat: &mut CombatState,
    particle_seed: u64,
) -> Result<(), CombatBeliefSamplingErrorV1> {
    // A seeded permutation of the source pile would keep the source's hidden
    // order as its input. Start from the public kind multiset instead.
    let mut order = combat
        .zones
        .draw_pile
        .iter()
        .map(DrawCardKind::from)
        .collect::<Vec<_>>();
    order.sort_by_key(|kind| (kind.card_id.stable_ordinal(), kind.upgrades));
    let mut rng = StsRng::new(chance_stream_seed_v1(particle_seed, 0));
    for right in (1..order.len()).rev() {
        let bound =
            i32::try_from(right).map_err(|_| CombatBeliefSamplingErrorV1::DrawPileTooLarge)?;
        let left = rng.random(bound) as usize;
        order.swap(left, right);
    }
    materialize_draw_order(combat, &order, particle_seed)
}

fn reseed_hidden_combat_futures_v1(combat: &mut CombatState, particle_seed: u64) {
    let pool = &mut combat.rng.pool;
    pool.ai_rng = resampled_combat_rng_v1(&pool.ai_rng, particle_seed, 1);
    pool.shuffle_rng = resampled_combat_rng_v1(&pool.shuffle_rng, particle_seed, 2);
    pool.card_random_rng = resampled_combat_rng_v1(&pool.card_random_rng, particle_seed, 3);
    pool.misc_rng = resampled_combat_rng_v1(&pool.misc_rng, particle_seed, 4);
    pool.math_rng = resampled_combat_rng_v1(&pool.math_rng, particle_seed, 5);
    pool.monster_hp_rng = resampled_combat_rng_v1(&pool.monster_hp_rng, particle_seed, 6);
    pool.card_rng = resampled_combat_rng_v1(&pool.card_rng, particle_seed, 7);
    pool.potion_rng = resampled_combat_rng_v1(&pool.potion_rng, particle_seed, 8);
}

fn resampled_combat_rng_v1(source: &StsRng, particle_seed: u64, stream: u64) -> StsRng {
    StsRng::new_with_counter(chance_stream_seed_v1(particle_seed, stream), source.counter)
}

fn chance_stream_seed_v1(particle_seed: u64, stream: u64) -> u64 {
    let mut value =
        particle_seed.wrapping_add(0x9e37_79b9_7f4a_7c15_u64.wrapping_mul(stream.wrapping_add(1)));
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::monsters::EnemyId;
    use crate::content::potions::{Potion, PotionId};
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::{CombatCard, Intent};

    #[test]
    fn particles_preserve_public_boundary_and_potion_actions() {
        let mut combat = visible_combat();
        combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Bash, 61),
            CombatCard::for_scenario(CardId::Defend, 62),
            CombatCard::for_scenario(CardId::Strike, 63),
        ]
        .into();
        combat.entities.potions = vec![Some(Potion::new(PotionId::FirePotion, 91)), None, None];
        let _ = combat.rng.ai_rng.random(99);
        let _ = combat.rng.potion_rng.random(99);
        let engine = EngineState::CombatPlayerTurn;
        let source_observation = combat_public_observation_v2(&combat);
        let source_surface = project_public_combat_actions_v1(&engine, &combat)
            .unwrap()
            .public;

        let particles = sample_independent_combat_futures_v1(&engine, &combat, &[101, 202])
            .expect("sample public-equivalent private futures");

        assert_eq!(
            particles[0].origin(),
            CombatBeliefParticleOriginV1::IndependentStreams { particle_seed: 101 }
        );
        assert!(source_surface.atomic_actions.iter().any(|action| matches!(
            action,
            crate::agent::information::action::PublicCombatAtomicActionV1::UsePotion {
                potion_index: 0,
                ..
            }
        )));
        for particle in &particles {
            assert_eq!(
                combat_public_observation_v2(&particle.private_position.combat),
                source_observation
            );
            assert_eq!(
                project_public_combat_actions_v1(
                    &particle.private_position.engine,
                    &particle.private_position.combat,
                )
                .unwrap()
                .public,
                source_surface
            );
            assert_eq!(particle.private_position.combat.rng.ai_rng.counter, 1);
            assert_eq!(particle.private_position.combat.rng.potion_rng.counter, 1);
            assert_eq!(particle.probability_mass(), 0.5);
        }
        assert_ne!(
            particles[0].private_position.combat.rng.ai_rng,
            particles[1].private_position.combat.rng.ai_rng
        );
    }

    #[test]
    fn ordered_draw_information_is_not_resampled() {
        let mut combat = visible_combat();
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::FrozenEye));
        combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Bash, 61),
            CombatCard::for_scenario(CardId::Defend, 62),
            CombatCard::for_scenario(CardId::Strike, 63),
        ]
        .into();
        let source_order = combat
            .zones
            .draw_pile
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>();

        let particle =
            sample_independent_combat_futures_v1(&EngineState::CombatPlayerTurn, &combat, &[303])
                .expect("sample an ordered-information particle")
                .pop()
                .unwrap();

        assert_eq!(
            particle
                .private_position
                .combat
                .zones
                .draw_pile
                .iter()
                .map(|card| card.lineage)
                .collect::<Vec<_>>(),
            source_order
        );
    }

    #[test]
    fn unordered_independent_samples_keep_public_card_orders_source_invariant() {
        let mut source = visible_combat();
        let mut discounted = CombatCard::for_scenario(CardId::Strike, 61);
        discounted.cost_modifier = -1;
        source.zones.draw_pile = vec![
            discounted,
            CombatCard::for_scenario(CardId::Defend, 62),
            CombatCard::for_scenario(CardId::Strike, 63),
        ]
        .into();
        let mut alternate = source.clone();
        alternate.zones.draw_pile.reverse();
        assert_eq!(
            combat_public_observation_v2(&source),
            combat_public_observation_v2(&alternate)
        );
        let sampled_orders = |combat: &CombatState| {
            sample_independent_combat_futures_v1(
                &EngineState::CombatPlayerTurn,
                combat,
                &[101, 202, 303],
            )
            .unwrap()
            .into_iter()
            .map(|particle| {
                particle
                    .private_position()
                    .combat
                    .zones
                    .draw_pile
                    .iter()
                    .map(|card| learning_card(&particle.private_position().combat, card, false))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
        };
        assert_eq!(sampled_orders(&source), sampled_orders(&alternate));
    }

    #[test]
    fn conditioned_materialization_preserves_cards_without_using_private_order_or_labels() {
        let mut source = visible_combat();
        let mut discounted = CombatCard::for_scenario(CardId::Strike, 61);
        discounted.cost_modifier = -1;
        discounted.free_to_play_once = true;
        source.zones.draw_pile = vec![
            discounted,
            CombatCard::for_scenario(CardId::Strike, 62),
            CombatCard::for_scenario(CardId::Defend, 63),
        ]
        .into();
        let mut alternate = source.clone();
        alternate.zones.draw_pile.reverse();
        for (i, card) in alternate.zones.draw_pile.iter_mut().enumerate() {
            card.lineage = CombatCard::for_scenario(card.id, i as u32 + 100).lineage;
        }
        let knowledge = DrawOrderKnowledge::from_observation(
            &crate::agent::information::state::public_combat_state_v1(&source),
        );
        let mut distribution = DrawOrderDistribution::new(&knowledge, 0).unwrap();
        let same_kind = vec![
            DrawCardKind::from(&source.zones.draw_pile[0]),
            DrawCardKind::from(&source.zones.draw_pile[1]),
        ];
        assert_eq!(same_kind[0], same_kind[1]);
        for seed in 1..=32 {
            let order = distribution.sample(&mut StsRng::new(seed)).unwrap();
            let mut left = source.clone();
            let mut right = alternate.clone();
            materialize_draw_order(&mut left, &order, seed).unwrap();
            materialize_draw_order(&mut right, &order, seed).unwrap();
            let properties = |combat: &CombatState| {
                combat
                    .zones
                    .draw_pile
                    .iter()
                    .map(|card| learning_card(combat, card, false))
                    .collect::<Vec<_>>()
            };
            assert_eq!(properties(&left), properties(&right));
            for card in source.zones.draw_pile.iter() {
                assert!(left.zones.draw_pile.iter().any(|sample| sample == card));
            }
        }
    }

    #[test]
    fn hidden_current_intent_is_an_explicit_sampling_gap() {
        let mut combat = visible_combat();
        combat
            .entities
            .player
            .add_relic(RelicState::new(RelicId::RunicDome));

        assert_eq!(
            sample_independent_combat_futures_v1(&EngineState::CombatPlayerTurn, &combat, &[404]),
            Err(CombatBeliefSamplingErrorV1::HiddenCurrentIntent)
        );
    }

    fn visible_combat() -> CombatState {
        let mut combat = crate::test_support::blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 51)];
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
