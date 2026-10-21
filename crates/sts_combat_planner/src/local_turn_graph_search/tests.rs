use super::{
    selected_boundary_generation_work, update_max_guide, GuideRankMap, LocalServiceView,
    LocalTurnGraphWitnessConfig,
};
use crate::policy::{CombatGuideLaneId, CombatStateGuideRank};

#[test]
fn semantic_guide_backup_is_monotone_per_lane() {
    let lane = crate::policy::CombatGuideLaneId::new(4);
    let weak = CombatStateGuideRank::new(vec![1, 2]);
    let strong = CombatStateGuideRank::new(vec![1, 3]);
    let weaker_later = CombatStateGuideRank::new(vec![1, 1]);
    let mut backed = GuideRankMap::default();

    assert!(update_max_guide(&mut backed, lane, &weak));
    assert!(update_max_guide(&mut backed, lane, &strong));
    assert!(!update_max_guide(&mut backed, lane, &weaker_later));
    assert_eq!(backed.get(&lane), Some(&strong));
}

#[test]
fn fresh_guide_boundary_receives_the_coherent_guide_quantum() {
    let config = LocalTurnGraphWitnessConfig {
        backed_generation_quantum_work: 128,
        initial_expansion_work: 64,
        ..LocalTurnGraphWitnessConfig::default()
    };

    assert_eq!(
        selected_boundary_generation_work(
            &config,
            1,
            0,
            LocalServiceView::Guide(CombatGuideLaneId::new(7)),
        ),
        128
    );
    assert_eq!(
        selected_boundary_generation_work(&config, 1, 0, LocalServiceView::Anchor),
        64
    );
    assert_eq!(
        selected_boundary_generation_work(&config, 1, 0, LocalServiceView::ProposalRoot),
        128
    );
    assert_eq!(
        selected_boundary_generation_work(&config, 1, 0, LocalServiceView::ProposalContinuation,),
        128
    );
    assert_eq!(
        selected_boundary_generation_work(
            &config,
            1,
            1,
            LocalServiceView::Guide(CombatGuideLaneId::new(7)),
        ),
        config.generation_quantum_work
    );
}

#[test]
fn backed_quantum_may_be_smaller_than_the_fresh_anchor_quantum() {
    let config = LocalTurnGraphWitnessConfig {
        backed_generation_quantum_work: 48,
        initial_expansion_work: 64,
        ..LocalTurnGraphWitnessConfig::default()
    };

    assert_eq!(
        selected_boundary_generation_work(
            &config,
            1,
            0,
            LocalServiceView::Guide(CombatGuideLaneId::new(7)),
        ),
        48
    );
    assert_eq!(
        selected_boundary_generation_work(&config, 1, 0, LocalServiceView::Anchor),
        64
    );
}
