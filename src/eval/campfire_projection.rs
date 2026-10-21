use crate::content::relics::RelicId;
use crate::engine::campfire_candidates::{
    resolve_campfire_candidate, CampfireCandidate, CampfireCandidateResolutionError,
};
use crate::state::core::{ClientInput, EngineState};
use crate::state::run::{with_suppressed_obtain_logs, RunState};

#[derive(Clone, Debug, PartialEq)]
pub enum CampfireProjection {
    Exact(CampfireExactProjection),
    Chance(CampfireChanceProjection),
    ChanceThenDecision(CampfireRecourseProjection),
}

#[derive(Clone, Debug, PartialEq)]
pub struct CampfireExactProjection {
    pub candidate: CampfireCandidate,
    pub engine_state: EngineState,
    pub run_state: RunState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampfireChanceProjection {
    pub candidate: CampfireCandidate,
    pub exact_prefix: CampfireExactPrefix,
    pub chance: CampfireChanceKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampfireRecourseProjection {
    pub candidate: CampfireCandidate,
    pub exact_prefix: CampfireExactPrefix,
    pub chance: CampfireChanceKind,
    pub recourse: CampfireRecourseKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CampfireExactPrefix {
    pub hp_before: i32,
    pub hp_after: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampfireChanceKind {
    DigRelicReward,
    DreamCatcherCardReward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampfireRecourseKind {
    ExistingCardRewardOwner,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampfireProjectionError {
    Candidate(CampfireCandidateResolutionError),
    EngineDidNotAdvance { candidate: CampfireCandidate },
    UnexpectedRngMutation { candidate: CampfireCandidate },
}

impl From<CampfireCandidateResolutionError> for CampfireProjectionError {
    fn from(value: CampfireCandidateResolutionError) -> Self {
        Self::Candidate(value)
    }
}

pub fn project_campfire_candidate(
    root: &RunState,
    candidate: CampfireCandidate,
) -> Result<CampfireProjection, CampfireProjectionError> {
    let choice = resolve_campfire_candidate(root, candidate)?;
    if candidate == CampfireCandidate::Dig {
        return Ok(CampfireProjection::Chance(CampfireChanceProjection {
            candidate,
            exact_prefix: unchanged_hp_prefix(root),
            chance: CampfireChanceKind::DigRelicReward,
        }));
    }
    if candidate == CampfireCandidate::Rest && has_dream_catcher(root) {
        let mut prefix = root.clone();
        crate::engine::campfire_handler::apply_campfire_rest_healing(&mut prefix);
        return Ok(CampfireProjection::ChanceThenDecision(
            CampfireRecourseProjection {
                candidate,
                exact_prefix: CampfireExactPrefix {
                    hp_before: root.current_hp,
                    hp_after: prefix.current_hp,
                },
                chance: CampfireChanceKind::DreamCatcherCardReward,
                recourse: CampfireRecourseKind::ExistingCardRewardOwner,
            },
        ));
    }

    let mut engine_state = EngineState::Campfire;
    let mut run_state = root.clone();
    let original_rng = run_state.rng_pool.clone();
    with_suppressed_obtain_logs(|| {
        crate::engine::campfire_handler::handle(
            &mut engine_state,
            &mut run_state,
            Some(ClientInput::CampfireOption(choice)),
        )
    });
    if matches!(engine_state, EngineState::Campfire) {
        return Err(CampfireProjectionError::EngineDidNotAdvance { candidate });
    }
    if run_state.rng_pool != original_rng {
        return Err(CampfireProjectionError::UnexpectedRngMutation { candidate });
    }
    Ok(CampfireProjection::Exact(CampfireExactProjection {
        candidate,
        engine_state,
        run_state,
    }))
}

fn unchanged_hp_prefix(root: &RunState) -> CampfireExactPrefix {
    CampfireExactPrefix {
        hp_before: root.current_hp,
        hp_after: root.current_hp,
    }
}

fn has_dream_catcher(root: &RunState) -> bool {
    root.relics
        .iter()
        .any(|relic| relic.id == RelicId::DreamCatcher)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::relics::{RelicId, RelicState};
    use crate::runtime::combat::CombatCard;
    use crate::state::run::RunState;

    #[test]
    fn smith_projection_uses_real_engine_on_a_clone_without_mutating_root() {
        let mut root = RunState::new(31, 0, false, "Ironclad");
        root.master_deck = vec![CombatCard::for_scenario(CardId::Strike, 501)];
        let before = root.clone();

        let projection = project_campfire_candidate(
            &root,
            CampfireCandidate::Smith {
                card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(501),
            },
        )
        .unwrap();

        let CampfireProjection::Exact(exact) = projection else {
            panic!("Smith must be exact");
        };
        assert_eq!(root, before);
        assert_eq!(exact.run_state.master_deck[0].upgrades, 1);
        assert_eq!(exact.run_state.rng_pool, root.rng_pool);
        assert!(matches!(
            exact.engine_state,
            crate::state::core::EngineState::MapNavigation
        ));
    }

    #[test]
    fn dig_projection_is_invariant_to_hidden_relic_rng_and_pool_order() {
        let mut root = RunState::new(37, 0, false, "Ironclad");
        root.relics.push(RelicState::new(RelicId::Shovel));
        let mut hidden_variant = root.clone();
        hidden_variant.rng_pool.relic_rng.random(999);
        hidden_variant.common_relic_pool.reverse();
        hidden_variant.uncommon_relic_pool.reverse();

        let first = project_campfire_candidate(&root, CampfireCandidate::Dig).unwrap();
        let second = project_campfire_candidate(&hidden_variant, CampfireCandidate::Dig).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first,
            CampfireProjection::Chance(CampfireChanceProjection {
                candidate: CampfireCandidate::Dig,
                exact_prefix: CampfireExactPrefix {
                    hp_before: root.current_hp,
                    hp_after: root.current_hp,
                },
                chance: CampfireChanceKind::DigRelicReward,
            })
        );
    }

    #[test]
    fn dream_catcher_projection_is_exact_heal_then_post_reveal_recourse() {
        let mut root = RunState::new(41, 0, false, "Ironclad");
        root.current_hp = 20;
        root.relics = vec![RelicState::new(RelicId::DreamCatcher)];
        let mut hidden_variant = root.clone();
        hidden_variant.rng_pool.card_rng.random(999);

        let first = project_campfire_candidate(&root, CampfireCandidate::Rest).unwrap();
        let second = project_campfire_candidate(&hidden_variant, CampfireCandidate::Rest).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first,
            CampfireProjection::ChanceThenDecision(CampfireRecourseProjection {
                candidate: CampfireCandidate::Rest,
                exact_prefix: CampfireExactPrefix {
                    hp_before: 20,
                    hp_after: 44,
                },
                chance: CampfireChanceKind::DreamCatcherCardReward,
                recourse: CampfireRecourseKind::ExistingCardRewardOwner,
            })
        );
    }
}
