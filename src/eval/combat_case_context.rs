use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ai::combat_state_key::combat_exact_state_hash_v2;
use crate::ai::combat_witness_contract::{
    CombatWitnessPotionPolicyV1, CombatWitnessSatisfactionV1,
};
use crate::content::monsters::factory::EncounterId;
use crate::eval::combat_case_core::{combat_summary, CombatCaseCoreV2, CombatCaseRngSummary};
use crate::eval::combat_guidance_bundle::CombatGuidanceBundleV1;
use crate::eval::run_control::{
    run_control_session_fingerprint_v3, OracleCombatWitnessOptionsV1,
    OracleRunCombatWitnessBudgetsV1, OracleRunCombatWitnessQualityPolicyV1, RunControlSession,
    RunControlSessionCheckpointV1,
};
use crate::state::core::{ActiveCombat, CombatContext};

pub const COMBAT_CASE_PRODUCTION_CONTEXT_SCHEMA_NAME: &str = "CombatCaseProductionContextV2";
pub const COMBAT_CASE_PRODUCTION_CONTEXT_SCHEMA_VERSION: u32 = 2;
pub const COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_NAME: &str = "CombatCaseReplayIdentityV2";
pub const COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatCaseReplayCapabilityV2 {
    IsolatedProjection,
    ExactProductionState,
    ExactProductionOwner,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseReplayIdentityV2 {
    pub schema_name: String,
    pub schema_version: u32,
    pub capability: CombatCaseReplayCapabilityV2,
    pub root_exact_state_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_session_fingerprint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_policy_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CombatCaseProductionOwnerV1 {
    OracleAnalysis {
        policy_fingerprint: String,
        budgets: CombatCaseOracleCombatWitnessBudgetsV1,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseOracleCombatWitnessBudgetsV1 {
    pub hallway: CombatCaseOracleWitnessOptionsV1,
    pub elite: CombatCaseOracleWitnessOptionsV1,
    pub boss: CombatCaseOracleWitnessOptionsV1,
    pub quality_policy: OracleRunCombatWitnessQualityPolicyV1,
    pub initial_divisor: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidance_bundle: Option<CombatGuidanceBundleV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseOracleWitnessOptionsV1 {
    pub max_generation_work: Option<usize>,
    pub max_engine_steps_per_transition: Option<usize>,
    pub wall_ms: Option<u64>,
    pub satisfaction: Option<CombatWitnessSatisfactionV1>,
    pub potion_policy: Option<CombatWitnessPotionPolicyV1>,
    pub max_potions_used: Option<u32>,
    pub allowed_potion_slots: Option<u64>,
    pub allow_potion_discard: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaseProductionContextV2 {
    pub schema_name: String,
    pub schema_version: u32,
    pub root_exact_state_hash: String,
    pub run_session_fingerprint: String,
    pub combat_context: CombatContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter_id: Option<EncounterId>,
    pub run_session_checkpoint: RunControlSessionCheckpointV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_owner: Option<CombatCaseProductionOwnerV1>,
}

pub fn capture_combat_case_production_context_v2(
    core: &CombatCaseCoreV2,
    session: &RunControlSession,
) -> Result<CombatCaseProductionContextV2, String> {
    let root_exact_state_hash =
        combat_exact_state_hash_v2(&core.position.engine, &core.position.combat);
    validate_core_against_session(core, session, &root_exact_state_hash)?;

    let mut run_session_checkpoint = RunControlSessionCheckpointV1::from_session(session);
    run_session_checkpoint.clear_combat_diagnostics_for_external_checkpoint();
    let active_combat = run_session_checkpoint
        .take_active_combat_for_external_ref()
        .ok_or_else(|| "production combat case requires an active combat checkpoint".to_string())?;
    let context = CombatCaseProductionContextV2 {
        schema_name: COMBAT_CASE_PRODUCTION_CONTEXT_SCHEMA_NAME.to_string(),
        schema_version: COMBAT_CASE_PRODUCTION_CONTEXT_SCHEMA_VERSION,
        root_exact_state_hash,
        run_session_fingerprint: run_control_session_fingerprint_v3(session),
        combat_context: active_combat.context,
        encounter_id: active_combat.encounter_id,
        run_session_checkpoint,
        production_owner: None,
    };
    validate_context_payload(core, &context)?;
    Ok(context)
}

pub fn capture_oracle_analysis_combat_case_production_context_v2(
    core: &CombatCaseCoreV2,
    session: &RunControlSession,
    budgets: &OracleRunCombatWitnessBudgetsV1,
) -> Result<CombatCaseProductionContextV2, String> {
    let mut context = capture_combat_case_production_context_v2(core, session)?;
    let budgets = CombatCaseOracleCombatWitnessBudgetsV1::capture(budgets)?;
    context.production_owner = Some(CombatCaseProductionOwnerV1::OracleAnalysis {
        policy_fingerprint: owner_policy_fingerprint(&budgets),
        budgets,
    });
    Ok(context)
}

pub fn restore_combat_case_oracle_analysis_owner_v2(
    core: &CombatCaseCoreV2,
    context: Option<&CombatCaseProductionContextV2>,
) -> Result<(RunControlSession, OracleRunCombatWitnessBudgetsV1), String> {
    let session = restore_combat_case_production_session_v2(core, context)?;
    let owner = context
        .and_then(|context| context.production_owner.as_ref())
        .ok_or_else(|| {
            "combat case has exact production state but no production owner".to_string()
        })?;
    validate_production_owner(owner)?;
    let CombatCaseProductionOwnerV1::OracleAnalysis { budgets, .. } = owner;
    Ok((session, budgets.restore()))
}

pub fn combat_case_replay_identity_v2(
    core: &CombatCaseCoreV2,
    context: Option<&CombatCaseProductionContextV2>,
) -> Result<CombatCaseReplayIdentityV2, String> {
    let root_exact_state_hash =
        combat_exact_state_hash_v2(&core.position.engine, &core.position.combat);
    let Some(context) = context else {
        return Ok(CombatCaseReplayIdentityV2 {
            schema_name: COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_NAME.to_string(),
            schema_version: COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_VERSION,
            capability: CombatCaseReplayCapabilityV2::IsolatedProjection,
            root_exact_state_hash,
            run_session_fingerprint: None,
            owner_policy_fingerprint: None,
        });
    };
    validate_context_payload(core, context)?;
    let owner_policy_fingerprint = context.production_owner.as_ref().map(|owner| {
        let CombatCaseProductionOwnerV1::OracleAnalysis {
            policy_fingerprint, ..
        } = owner;
        policy_fingerprint.clone()
    });
    Ok(CombatCaseReplayIdentityV2 {
        schema_name: COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_NAME.to_string(),
        schema_version: COMBAT_CASE_REPLAY_IDENTITY_SCHEMA_VERSION,
        capability: if owner_policy_fingerprint.is_some() {
            CombatCaseReplayCapabilityV2::ExactProductionOwner
        } else {
            CombatCaseReplayCapabilityV2::ExactProductionState
        },
        root_exact_state_hash,
        run_session_fingerprint: Some(context.run_session_fingerprint.clone()),
        owner_policy_fingerprint,
    })
}

impl CombatCaseOracleCombatWitnessBudgetsV1 {
    fn capture(budgets: &OracleRunCombatWitnessBudgetsV1) -> Result<Self, String> {
        if budgets.initial_divisor == 0 {
            return Err("oracle combat initial divisor must be positive".to_string());
        }
        Ok(Self {
            hallway: CombatCaseOracleWitnessOptionsV1::capture(&budgets.hallway),
            elite: CombatCaseOracleWitnessOptionsV1::capture(&budgets.elite),
            boss: CombatCaseOracleWitnessOptionsV1::capture(&budgets.boss),
            quality_policy: budgets.quality_policy,
            initial_divisor: budgets.initial_divisor,
            guidance_bundle: budgets.guidance_bundle.as_deref().cloned(),
        })
    }

    fn restore(&self) -> OracleRunCombatWitnessBudgetsV1 {
        OracleRunCombatWitnessBudgetsV1 {
            hallway: self.hallway.restore(),
            elite: self.elite.restore(),
            boss: self.boss.restore(),
            quality_policy: self.quality_policy,
            initial_divisor: self.initial_divisor,
            guidance_bundle: self.guidance_bundle.clone().map(Arc::new),
        }
    }
}

impl CombatCaseOracleWitnessOptionsV1 {
    fn capture(options: &OracleCombatWitnessOptionsV1) -> Self {
        Self {
            max_generation_work: options.max_generation_work,
            max_engine_steps_per_transition: options.max_engine_steps_per_transition,
            wall_ms: options.wall_ms,
            satisfaction: options.satisfaction,
            potion_policy: options.potion_policy,
            max_potions_used: options.max_potions_used,
            allowed_potion_slots: options.allowed_potion_slots,
            allow_potion_discard: options.allow_potion_discard,
        }
    }

    fn restore(&self) -> OracleCombatWitnessOptionsV1 {
        OracleCombatWitnessOptionsV1 {
            max_generation_work: self.max_generation_work,
            max_engine_steps_per_transition: self.max_engine_steps_per_transition,
            wall_ms: self.wall_ms,
            satisfaction: self.satisfaction,
            potion_policy: self.potion_policy,
            max_potions_used: self.max_potions_used,
            allowed_potion_slots: self.allowed_potion_slots,
            allow_potion_discard: self.allow_potion_discard,
        }
    }
}

pub fn validate_combat_case_production_context_v2(
    core: &CombatCaseCoreV2,
    context: Option<&CombatCaseProductionContextV2>,
) -> Result<(), String> {
    let context =
        context.ok_or_else(|| "combat case has no exact production context".to_string())?;
    validate_context_payload(core, context).map(|_| ())
}

pub fn restore_combat_case_production_session_v2(
    core: &CombatCaseCoreV2,
    context: Option<&CombatCaseProductionContextV2>,
) -> Result<RunControlSession, String> {
    let context = context.ok_or_else(|| "combat case supports isolated replay only".to_string())?;
    validate_context_payload(core, context)
}

fn validate_context_payload(
    core: &CombatCaseCoreV2,
    context: &CombatCaseProductionContextV2,
) -> Result<RunControlSession, String> {
    validate_context_header(context)?;
    if let Some(owner) = context.production_owner.as_ref() {
        validate_production_owner(owner)?;
    }
    let case_root = combat_exact_state_hash_v2(&core.position.engine, &core.position.combat);
    if context.root_exact_state_hash != case_root {
        return Err(format!(
            "combat case root hash mismatch: context {}, case {case_root}",
            context.root_exact_state_hash
        ));
    }
    let session = restore_context_session(core, context)?;
    validate_core_against_session(core, &session, &case_root)?;
    let restored_fingerprint = run_control_session_fingerprint_v3(&session);
    if context.run_session_fingerprint != restored_fingerprint {
        return Err(format!(
            "combat case run-context fingerprint mismatch: context {}, checkpoint {restored_fingerprint}",
            context.run_session_fingerprint
        ));
    }
    Ok(session)
}

fn validate_production_owner(owner: &CombatCaseProductionOwnerV1) -> Result<(), String> {
    let CombatCaseProductionOwnerV1::OracleAnalysis {
        policy_fingerprint,
        budgets,
    } = owner;
    if budgets.initial_divisor == 0 {
        return Err("combat case owner initial divisor must be positive".to_string());
    }
    let actual = owner_policy_fingerprint(budgets);
    if policy_fingerprint != &actual {
        return Err(format!(
            "combat case owner-policy fingerprint mismatch: context {policy_fingerprint}, payload {actual}"
        ));
    }
    Ok(())
}

fn owner_policy_fingerprint(budgets: &CombatCaseOracleCombatWitnessBudgetsV1) -> String {
    crate::eval::fingerprint::hash_serializable(budgets)
}

fn restore_context_session(
    core: &CombatCaseCoreV2,
    context: &CombatCaseProductionContextV2,
) -> Result<RunControlSession, String> {
    let mut checkpoint = context.run_session_checkpoint.clone();
    if checkpoint.take_active_combat_for_external_ref().is_some() {
        return Err(
            "combat case production checkpoint must externalize its active combat".to_string(),
        );
    }
    let active_combat = match context.encounter_id {
        Some(encounter_id) => ActiveCombat::new_for_encounter(
            core.position.engine.clone(),
            core.position.combat.clone(),
            encounter_id,
            context.combat_context.clone(),
        ),
        None => ActiveCombat::new(
            core.position.engine.clone(),
            core.position.combat.clone(),
            context.combat_context.clone(),
        ),
    };
    checkpoint.restore_active_combat_from_external_ref(active_combat);
    checkpoint.into_session()
}

fn validate_context_header(context: &CombatCaseProductionContextV2) -> Result<(), String> {
    if context.schema_name != COMBAT_CASE_PRODUCTION_CONTEXT_SCHEMA_NAME
        || context.schema_version != COMBAT_CASE_PRODUCTION_CONTEXT_SCHEMA_VERSION
    {
        return Err(format!(
            "unsupported combat case production context {} version {}",
            context.schema_name, context.schema_version
        ));
    }
    Ok(())
}

fn validate_core_against_session(
    core: &CombatCaseCoreV2,
    session: &RunControlSession,
    expected_root: &str,
) -> Result<(), String> {
    let session_position = session.current_active_combat_position()?;
    let session_root =
        combat_exact_state_hash_v2(&session_position.engine, &session_position.combat);
    if session_root != expected_root {
        return Err(format!(
            "combat case active-combat hash mismatch: case {expected_root}, checkpoint {session_root}"
        ));
    }
    if core.source.seed != session.run_state.seed
        || core.source.ascension != session.run_state.ascension_level
    {
        return Err(format!(
            "combat case source mismatch: case seed {} A{}, checkpoint seed {} A{}",
            core.source.seed,
            core.source.ascension,
            session.run_state.seed,
            session.run_state.ascension_level
        ));
    }

    let expected_run = (
        session.run_state.act_num,
        session.run_state.floor_num,
        session.run_state.current_hp,
        session.run_state.max_hp,
        session.run_state.gold,
        session.run_state.master_deck.len(),
        session.run_state.relics.len(),
        session.run_state.potions.len(),
    );
    let case_run = (
        core.run.act,
        core.run.floor,
        core.run.hp,
        core.run.max_hp,
        core.run.gold,
        core.run.deck_size,
        core.run.relic_count,
        core.run.potion_slots,
    );
    if case_run != expected_run {
        return Err("combat case run summary does not match production checkpoint".to_string());
    }
    if core.combat != combat_summary(&core.position) {
        return Err("combat case combat summary does not match its exact root".to_string());
    }
    if core.run_rng != CombatCaseRngSummary::from_pool(&session.run_state.rng_pool) {
        return Err("combat case run RNG summary does not match production checkpoint".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::combat_case::{
        CombatCase, CombatCaseGap, CombatCaseRunSummary, CombatCaseSource,
        CombatCaseWitnessBudgetV1,
    };
    use crate::eval::run_control::{RunControlConfig, RunControlSession};
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::map::node::RoomType;

    fn exact_fixture() -> (CombatCase, RunControlSession) {
        let mut session = RunControlSession::new(RunControlConfig {
            seed: 71,
            ascension_level: 4,
            player_class: "Ironclad",
            ..RunControlConfig::default()
        });
        let combat = crate::test_support::blank_test_combat();
        session.run_state.act_num = 2;
        session.run_state.floor_num = 23;
        session.run_state.current_hp = combat.entities.player.current_hp;
        session.run_state.max_hp = combat.entities.player.max_hp;
        session.run_state.gold = combat.entities.player.gold;
        session.run_state.master_deck = combat.meta.master_deck_snapshot.to_vec();
        session.run_state.relics = combat.entities.player.relics.clone();
        session.run_state.potions = combat.entities.potions.clone();
        session.run_state.rng_pool = combat.rng.pool.clone();
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new_for_encounter(
            EngineState::CombatPlayerTurn,
            combat,
            EncounterId::JawWorm,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        let position = session.current_active_combat_position().unwrap();
        let case = CombatCase::new(
            CombatCaseSource {
                seed: 71,
                ascension: 4,
                generation: 3,
                branch_id: 9,
                parent_id: Some(8),
            },
            CombatCaseGap {
                boundary: "fixture".to_string(),
                reason: "contract".to_string(),
                witness_budget: CombatCaseWitnessBudgetV1::AtomicExactV2 {
                    primary_nodes: 10,
                    primary_wall_ms: 20,
                    rescue_nodes: 0,
                    rescue_wall_ms: 0,
                },
            },
            CombatCaseRunSummary {
                act: session.run_state.act_num,
                floor: session.run_state.floor_num,
                hp: session.run_state.current_hp,
                max_hp: session.run_state.max_hp,
                gold: session.run_state.gold,
                deck_size: session.run_state.master_deck.len(),
                relic_count: session.run_state.relics.len(),
                potion_slots: session.run_state.potions.len(),
            },
            Vec::new(),
            None,
            Vec::new(),
            CombatCaseRngSummary::from_pool(&session.run_state.rng_pool),
            position,
        );
        (case, session)
    }

    #[test]
    fn exact_context_capture_round_trips_the_production_state() {
        let (mut case, session) = exact_fixture();
        case.production_context =
            Some(capture_combat_case_production_context_v2(&case.core, &session).unwrap());

        let payload = serde_json::to_value(&case).unwrap();
        assert!(payload["production_context"]["run_session_checkpoint"][2].is_null());
        assert_eq!(payload["production_context"]["encounter_id"], "JawWorm");
        case = serde_json::from_value(payload).expect("decode flat production combat case");

        assert_eq!(
            case.replay_capability_v2().unwrap(),
            CombatCaseReplayCapabilityV2::ExactProductionState
        );
        let identity = case.replay_identity_v2().unwrap();
        assert_eq!(
            identity.root_exact_state_hash,
            combat_exact_state_hash_v2(&case.core.position.engine, &case.core.position.combat)
        );
        assert_eq!(
            identity.run_session_fingerprint.as_deref(),
            case.production_context
                .as_ref()
                .map(|context| context.run_session_fingerprint.as_str())
        );
        assert!(identity.owner_policy_fingerprint.is_none());
        let restored =
            restore_combat_case_production_session_v2(&case.core, case.production_context.as_ref())
                .unwrap();
        assert_eq!(
            restored
                .active_combat
                .as_ref()
                .and_then(|combat| combat.encounter_id),
            Some(EncounterId::JawWorm)
        );
        assert_eq!(
            run_control_session_fingerprint_v3(&restored),
            run_control_session_fingerprint_v3(&session)
        );
    }

    #[test]
    fn exact_context_rejects_a_changed_combat_root() {
        let (mut case, session) = exact_fixture();
        case.production_context =
            Some(capture_combat_case_production_context_v2(&case.core, &session).unwrap());
        case.core.position.combat.entities.player.current_hp -= 1;
        case.core.combat = combat_summary(&case.core.position);

        let error =
            restore_combat_case_production_session_v2(&case.core, case.production_context.as_ref())
                .unwrap_err();
        assert!(error.contains("root hash mismatch"), "{error}");
    }

    #[test]
    fn oracle_owner_context_round_trips_policy_without_default_inference() {
        let (mut case, session) = exact_fixture();
        let options = OracleCombatWitnessOptionsV1 {
            max_generation_work: Some(12_345),
            wall_ms: Some(678),
            satisfaction: Some(CombatWitnessSatisfactionV1::FirstCompleteWin),
            potion_policy: Some(CombatWitnessPotionPolicyV1::Never),
            max_potions_used: Some(0),
            ..OracleCombatWitnessOptionsV1::default()
        };
        let mut budgets = OracleRunCombatWitnessBudgetsV1::uniform(options);
        budgets.quality_policy = OracleRunCombatWitnessQualityPolicyV1::StrategicRun;
        budgets.initial_divisor = 3;
        case.production_context = Some(
            capture_oracle_analysis_combat_case_production_context_v2(
                &case.core, &session, &budgets,
            )
            .unwrap(),
        );

        assert_eq!(
            case.replay_capability_v2().unwrap(),
            CombatCaseReplayCapabilityV2::ExactProductionOwner
        );
        let identity = case.replay_identity_v2().unwrap();
        assert!(identity.run_session_fingerprint.is_some());
        assert!(identity.owner_policy_fingerprint.is_some());
        let (_, restored) = restore_combat_case_oracle_analysis_owner_v2(
            &case.core,
            case.production_context.as_ref(),
        )
        .unwrap();
        assert_eq!(restored.hallway.max_generation_work, Some(12_345));
        assert_eq!(restored.hallway.wall_ms, Some(678));
        assert_eq!(
            restored.hallway.satisfaction,
            Some(CombatWitnessSatisfactionV1::FirstCompleteWin)
        );
        assert_eq!(restored.initial_divisor, 3);
        assert_eq!(
            restored.quality_policy,
            OracleRunCombatWitnessQualityPolicyV1::StrategicRun
        );
    }

    #[test]
    fn oracle_owner_context_rejects_a_tampered_policy_payload() {
        let (mut case, session) = exact_fixture();
        let budgets =
            OracleRunCombatWitnessBudgetsV1::uniform(OracleCombatWitnessOptionsV1::default());
        case.production_context = Some(
            capture_oracle_analysis_combat_case_production_context_v2(
                &case.core, &session, &budgets,
            )
            .unwrap(),
        );
        let owner = case
            .production_context
            .as_mut()
            .unwrap()
            .production_owner
            .as_mut()
            .unwrap();
        let CombatCaseProductionOwnerV1::OracleAnalysis { budgets, .. } = owner;
        budgets.hallway.max_generation_work = Some(123_456);

        let error = case.replay_capability_v2().unwrap_err();
        assert!(
            error.contains("owner-policy fingerprint mismatch"),
            "{error}"
        );
    }

    #[test]
    fn exact_context_rejects_a_foreign_run_fingerprint() {
        let (mut case, session) = exact_fixture();
        case.production_context =
            Some(capture_combat_case_production_context_v2(&case.core, &session).unwrap());
        case.production_context
            .as_mut()
            .unwrap()
            .run_session_fingerprint = "foreign".to_string();

        let error =
            restore_combat_case_production_session_v2(&case.core, case.production_context.as_ref())
                .unwrap_err();
        assert!(
            error.contains("run-context fingerprint mismatch"),
            "{error}"
        );
    }

    #[test]
    fn case_without_production_context_is_explicitly_isolated_only() {
        let (case, _) = exact_fixture();

        assert_eq!(
            case.replay_capability_v2().unwrap(),
            CombatCaseReplayCapabilityV2::IsolatedProjection
        );
        let identity = case.replay_identity_v2().unwrap();
        assert_eq!(
            identity.root_exact_state_hash,
            combat_exact_state_hash_v2(&case.core.position.engine, &case.core.position.combat)
        );
        assert!(identity.run_session_fingerprint.is_none());
        assert!(identity.owner_policy_fingerprint.is_none());
        assert!(restore_combat_case_production_session_v2(&case.core, None)
            .unwrap_err()
            .contains("isolated replay only"));
    }

    #[test]
    fn derived_position_refresh_clears_exact_production_context() {
        let (mut case, session) = exact_fixture();
        case.production_context =
            Some(capture_combat_case_production_context_v2(&case.core, &session).unwrap());
        case.core.position.combat.entities.player.current_hp -= 1;

        case.refresh_derived_summaries_and_clear_production_context();

        assert!(case.production_context.is_none());
        assert_eq!(case.core.combat, combat_summary(&case.core.position));
        assert_eq!(
            case.core.run.hp,
            case.core.position.combat.entities.player.current_hp
        );
    }
}
