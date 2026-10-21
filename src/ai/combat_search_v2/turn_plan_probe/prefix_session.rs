use crate::ai::combat_search_v2::frontier::SearchNode;
use crate::ai::combat_search_v2::turn_planner::{TurnPlanLayerSessionV1, TurnPlanPartialLayerV1};
use crate::ai::combat_search_v2::{
    CombatSearchPluginStack, CombatSearchV2Config, CombatSearchV2TurnPlanPartialRetentionAuditV1,
    CombatSearchV2TurnPlanProbeEnumeration,
};
use crate::runtime::combat::CombatState;
use crate::sim::combat::{CombatPosition, EngineCombatStepper};
use crate::state::core::{ClientInput, EngineState};

use super::{build_probe_enumeration, turn_planner_config};

/// One privileged exact partial-turn candidate. Downstream model adapters must
/// project `position` and `inputs` to public semantics before observation.
#[derive(Clone)]
pub struct CombatSearchV2TurnPlanPrefixCandidateV1 {
    pub inputs: Vec<ClientInput>,
    pub position: CombatPosition,
}

#[derive(Clone)]
pub struct CombatSearchV2TurnPlanPrefixLayerV1 {
    pub atomic_depth: usize,
    pub max_retained_states: usize,
    pub nodes_expanded: usize,
    pub nodes_generated: usize,
    pub candidates: Vec<CombatSearchV2TurnPlanPrefixCandidateV1>,
}

/// Resumable exact same-turn enumerator which pauses before each partial beam
/// decision. It owns exact transitions, action equivalence, deduplication, and
/// final complete-turn selection; callers own only the bounded set and order
/// of retained observation-local ordinals.
#[derive(Clone)]
pub struct CombatSearchV2TurnPlanPrefixSessionV1 {
    engine: EngineState,
    combat: CombatState,
    config: CombatSearchV2Config,
    root: SearchNode,
    turn_config: crate::ai::combat_search_v2::turn_planner::TurnPlannerConfigV1,
    inner: TurnPlanLayerSessionV1,
}

impl CombatSearchV2TurnPlanPrefixSessionV1 {
    pub fn new(engine: &EngineState, combat: &CombatState, config: &CombatSearchV2Config) -> Self {
        let root = SearchNode::root(engine.clone(), combat.clone());
        let plugins = CombatSearchPluginStack::from_config(config);
        let turn_config = turn_planner_config(config, &plugins);
        let inner = TurnPlanLayerSessionV1::new(&root, &turn_config, true);
        Self {
            engine: engine.clone(),
            combat: combat.clone(),
            config: config.clone(),
            root,
            turn_config,
            inner,
        }
    }

    /// Returns the current pending layer, expanding exact transitions only
    /// when no prior layer is waiting for a retain decision.
    pub fn observe_layer(&mut self) -> Option<CombatSearchV2TurnPlanPrefixLayerV1> {
        loop {
            if let Some(layer) = self.inner.pending_layer() {
                if layer.candidates.is_empty() {
                    self.inner
                        .retain_pending_ordinals(&[])
                        .expect("empty pending layer accepts an empty retain set");
                    continue;
                }
                return Some(public_layer(
                    layer,
                    self.inner.nodes_expanded(),
                    self.inner.nodes_generated(),
                ));
            }
            if !self.inner.expand_next_layer(&EngineCombatStepper, None) {
                return None;
            }
        }
    }

    pub fn retain_layer(&mut self, ordinals: &[usize]) -> Result<(), String> {
        self.inner.retain_pending_ordinals(ordinals)
    }

    pub fn retain_layer_with_heuristic_fill(&mut self, ordinals: &[usize]) -> Result<(), String> {
        self.inner
            .retain_pending_ordinals_with_heuristic_fill(ordinals)
    }

    /// Applies the maintained production selector to the currently observed
    /// layer with caller-owned opaque guide ranks. This is the resumable
    /// counterpart of guided one-shot enumeration; it exists so diagnostics
    /// can observe production retention without reimplementing it.
    pub fn retain_layer_with_guides(
        &mut self,
        partial_state_guides: &dyn Fn(
            &CombatPosition,
        )
            -> Vec<super::CombatSearchV2TurnPlanProbeGuideRankV1>,
    ) -> CombatSearchV2TurnPlanPartialRetentionAuditV1 {
        self.inner
            .retain_pending_with_guides_audited(Some(partial_state_guides))
    }

    pub fn finish(self) -> Result<CombatSearchV2TurnPlanProbeEnumeration, String> {
        if !self.inner.is_ready_to_finish() {
            return Err(
                "turn-plan prefix session still has an unobserved or unretained layer".to_owned(),
            );
        }
        let enumeration = self.inner.finish();
        Ok(build_probe_enumeration(
            &self.engine,
            &self.combat,
            &self.config,
            &self.root,
            &self.turn_config,
            enumeration,
            "turn_plan_v1_external_partial_frontier_selection",
        ))
    }
}

fn public_layer(
    layer: TurnPlanPartialLayerV1,
    nodes_expanded: usize,
    nodes_generated: usize,
) -> CombatSearchV2TurnPlanPrefixLayerV1 {
    CombatSearchV2TurnPlanPrefixLayerV1 {
        atomic_depth: layer.atomic_depth,
        max_retained_states: layer.max_retained_states,
        nodes_expanded,
        nodes_generated,
        candidates: layer
            .candidates
            .into_iter()
            .map(|candidate| CombatSearchV2TurnPlanPrefixCandidateV1 {
                inputs: candidate.inputs,
                position: candidate.position,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::combat_state_key::combat_exact_state_hash_v2;
    use crate::content::{cards::CardId, monsters::EnemyId};
    use crate::runtime::combat::CombatCard;
    use crate::test_support::{blank_test_combat, planned_monster};

    #[test]
    fn retaining_every_small_layer_matches_one_shot_enumeration() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let engine = EngineState::CombatPlayerTurn;
        let config = CombatSearchV2Config {
            turn_plan_probe_max_inner_nodes: Some(64),
            turn_plan_probe_max_partial_frontier_states: Some(64),
            turn_plan_probe_max_end_states: Some(64),
            turn_plan_probe_per_bucket_limit: Some(64),
            ..Default::default()
        };

        let expected = super::super::enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices(
            &engine,
            &combat,
            &config,
        );
        let mut session = CombatSearchV2TurnPlanPrefixSessionV1::new(&engine, &combat, &config);
        let mut observed_depths = Vec::new();
        while let Some(layer) = session.observe_layer() {
            let repeated = session
                .observe_layer()
                .expect("an unretained layer remains cached");
            assert_eq!(repeated.atomic_depth, layer.atomic_depth);
            assert_eq!(repeated.candidates.len(), layer.candidates.len());
            assert!(layer.candidates.len() <= layer.max_retained_states);
            observed_depths.push(layer.atomic_depth);
            session
                .retain_layer(&(0..layer.candidates.len()).collect::<Vec<_>>())
                .unwrap();
        }
        let actual = session.finish().unwrap();

        assert!(!observed_depths.is_empty());
        let mut expected_hashes = expected
            .candidates
            .iter()
            .map(|candidate| {
                combat_exact_state_hash_v2(&candidate.position.engine, &candidate.position.combat)
            })
            .collect::<Vec<_>>();
        let mut actual_hashes = actual
            .candidates
            .iter()
            .map(|candidate| {
                combat_exact_state_hash_v2(&candidate.position.engine, &candidate.position.combat)
            })
            .collect::<Vec<_>>();
        expected_hashes.sort();
        actual_hashes.sort();
        assert_eq!(actual_hashes, expected_hashes);
        assert_eq!(
            actual.report.enumeration.nodes_expanded,
            expected.report.enumeration.nodes_expanded
        );
        assert_eq!(
            actual.report.enumeration.nodes_generated,
            expected.report.enumeration.nodes_generated
        );
    }

    #[test]
    fn resumable_guided_retention_matches_guided_one_shot_enumeration() {
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Strike, 10),
            CombatCard::for_scenario(CardId::Defend, 11),
        ];
        let engine = EngineState::CombatPlayerTurn;
        let config = CombatSearchV2Config {
            turn_plan_probe_max_inner_nodes: Some(64),
            turn_plan_probe_max_partial_frontier_states: Some(1),
            turn_plan_probe_max_end_states: Some(64),
            turn_plan_probe_per_bucket_limit: Some(64),
            ..Default::default()
        };
        let guides = |position: &CombatPosition| {
            vec![
                crate::ai::combat_search_v2::CombatSearchV2TurnPlanProbeGuideRankV1 {
                    lane: 7,
                    components: vec![position.combat.entities.player.block],
                },
            ]
        };

        let expected = super::super::
            enumerate_combat_search_v2_turn_plan_probe_candidates_across_pending_choices_guided(
                &engine, &combat, &config, &guides,
            );
        let mut session = CombatSearchV2TurnPlanPrefixSessionV1::new(&engine, &combat, &config);
        while let Some(layer) = session.observe_layer() {
            let candidate_count = layer.candidates.len();
            let audit = session.retain_layer_with_guides(&guides);
            assert_eq!(audit.candidates.len(), candidate_count);
            assert_eq!(audit.max_retained_states, 1);
            assert_eq!(
                audit
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.selected_by.is_some())
                    .count(),
                1
            );
        }
        let actual = session.finish().unwrap();

        let hashes = |enumeration: &CombatSearchV2TurnPlanProbeEnumeration| {
            let mut hashes = enumeration
                .candidates
                .iter()
                .map(|candidate| {
                    combat_exact_state_hash_v2(
                        &candidate.position.engine,
                        &candidate.position.combat,
                    )
                })
                .collect::<Vec<_>>();
            hashes.sort();
            hashes
        };
        assert_eq!(hashes(&actual), hashes(&expected));
        assert_eq!(
            actual.report.enumeration.nodes_expanded,
            expected.report.enumeration.nodes_expanded
        );
        assert_eq!(
            actual.report.enumeration.nodes_generated,
            expected.report.enumeration.nodes_generated
        );
    }
}
