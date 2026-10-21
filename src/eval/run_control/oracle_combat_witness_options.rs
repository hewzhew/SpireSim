use std::time::Duration;

use crate::ai::combat_witness_contract::{
    CombatWitnessPotionPolicyV1, CombatWitnessSatisfactionV1,
};
use crate::sim::combat::CombatPosition;

use super::session::RunControlSession;

/// Request surface owned by the production complete-turn portfolio.
///
/// Atomic-v2 rollout, frontier, turn-plan, plugin, and report controls are
/// intentionally absent: the portfolio cannot silently ignore them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OracleCombatWitnessOptionsV1 {
    /// Complete-turn generation work, not atomic-v2 expanded nodes.
    pub max_generation_work: Option<usize>,
    pub max_engine_steps_per_transition: Option<usize>,
    pub wall_ms: Option<u64>,
    pub satisfaction: Option<CombatWitnessSatisfactionV1>,
    pub potion_policy: Option<CombatWitnessPotionPolicyV1>,
    pub max_potions_used: Option<u32>,
    pub allowed_potion_slots: Option<u64>,
    pub allow_potion_discard: Option<bool>,
}

pub(super) struct PreparedOracleCombatWitnessV1 {
    pub(super) start: CombatPosition,
    pub(super) max_generation_work: usize,
    pub(super) max_engine_steps_per_transition: usize,
    pub(super) wall_time: Option<Duration>,
    pub(super) satisfaction: CombatWitnessSatisfactionV1,
    pub(super) max_potions_used: Option<u32>,
    pub(super) allowed_potion_slots: Option<u64>,
    pub(super) allow_potion_discard: bool,
}

pub(super) fn prepare_oracle_combat_witness_v1(
    session: &RunControlSession,
    options: OracleCombatWitnessOptionsV1,
) -> Result<PreparedOracleCombatWitnessV1, String> {
    let start = session.current_active_combat_position()?;
    let satisfaction = options
        .satisfaction
        .unwrap_or(CombatWitnessSatisfactionV1::ZeroLossOrBudget);
    let potion_policy = options
        .potion_policy
        .unwrap_or(CombatWitnessPotionPolicyV1::Never);
    let allow_potion_discard = options
        .allow_potion_discard
        .unwrap_or(matches!(potion_policy, CombatWitnessPotionPolicyV1::All));

    Ok(PreparedOracleCombatWitnessV1 {
        start,
        max_generation_work: options.max_generation_work.unwrap_or(50_000),
        max_engine_steps_per_transition: options
            .max_engine_steps_per_transition
            .unwrap_or(250)
            .max(1),
        wall_time: options.wall_ms.map(Duration::from_millis),
        satisfaction,
        max_potions_used: options.max_potions_used,
        allowed_potion_slots: options.allowed_potion_slots,
        allow_potion_discard,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::map::node::RoomType;

    fn active_session() -> RunControlSession {
        let mut session = RunControlSession::new(Default::default());
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            crate::test_support::blank_test_combat(),
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        session
    }

    #[test]
    fn production_request_has_no_atomic_search_tuning_surface() {
        let prepared = prepare_oracle_combat_witness_v1(
            &active_session(),
            OracleCombatWitnessOptionsV1 {
                max_generation_work: Some(17),
                max_engine_steps_per_transition: Some(31),
                potion_policy: Some(CombatWitnessPotionPolicyV1::SemanticBudgeted),
                ..OracleCombatWitnessOptionsV1::default()
            },
        )
        .expect("prepare production planner");

        assert_eq!(prepared.max_generation_work, 17);
        assert_eq!(prepared.max_engine_steps_per_transition, 31);
        assert!(!prepared.allow_potion_discard);
    }

    #[test]
    fn production_request_does_not_inherit_atomic_session_tuning() {
        let config = crate::eval::run_control::RunControlConfig {
            search_max_nodes: Some(17),
            search_wall_ms: Some(31),
            search_max_hp_loss: Some(4),
            search_potion_policy: Some(CombatWitnessPotionPolicyV1::All),
            search_max_potions_used: Some(2),
            ..Default::default()
        };
        let mut session = RunControlSession::new(config);
        session.active_combat = active_session().active_combat;

        let prepared =
            prepare_oracle_combat_witness_v1(&session, OracleCombatWitnessOptionsV1::default())
                .expect("prepare production planner");

        assert_eq!(prepared.max_generation_work, 50_000);
        assert_eq!(prepared.wall_time, None);
        assert_eq!(
            prepared.satisfaction,
            CombatWitnessSatisfactionV1::ZeroLossOrBudget
        );
        assert_eq!(prepared.max_potions_used, None);
        assert!(!prepared.allow_potion_discard);
    }
}
