use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ai::combat_state_key::{
    combat_exact_state_key, combat_exact_state_key_hash_v2, stable_dominance_bucket_key,
    stable_outcome_key,
};
use crate::eval::artifact::{
    ArtifactHeaderV1, ArtifactProvenanceV1, ArtifactTrustLevel, ARTIFACT_PRODUCER,
};
use crate::eval::fingerprint::{
    combat_fingerprint_bundle_v3, CombatLegalActionSurfaceFingerprintV3, StateFingerprintV3,
};
use crate::runtime::combat::{CardLineageId, Intent};
use crate::sim::combat::{combat_terminal, stable_boundary, CombatPosition, CombatTerminal};
use crate::sim::combat_identity::validate_combat_card_identity_for_capture;
use crate::state::core::EngineState;
use crate::state::run::RunState;

pub const COMBAT_CAPTURE_SCHEMA_NAME: &str = "CombatCaptureV3";
pub const COMBAT_CAPTURE_SCHEMA_VERSION: u32 = 3;
const COMBAT_CAPTURE_INTEGRITY_ALGORITHM_V3: &str =
    "exact:blake2b_256_semantic_json_v2;stable:blake2b_256_canonical_json_v3";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaptureV3 {
    pub schema_name: String,
    pub schema_version: u32,
    pub header: ArtifactHeaderV1,
    pub capture_kind: CombatCaptureKindV3,
    pub trust_level: ArtifactTrustLevel,
    pub information_boundary: String,
    pub label: Option<String>,
    pub provenance: ArtifactProvenanceV1,
    pub source: CombatCaptureSourceV3,
    pub integrity: CombatCaptureIntegrityV3,
    pub fingerprints: StateFingerprintV3,
    pub legal_action_surface: CombatLegalActionSurfaceFingerprintV3,
    pub summary: CombatCaptureSummaryV3,
    pub position: CombatPosition,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatCaptureKindV3 {
    CombatPosition,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaptureSourceV3 {
    pub producer: String,
    pub capture_method: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaptureIntegrityV3 {
    pub fingerprint_algorithm: String,
    pub exact_state_fingerprint: String,
    pub stable_outcome_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaptureSummaryV3 {
    pub engine_state: String,
    pub terminal: CombatTerminal,
    pub stable_boundary: bool,
    pub player_class: String,
    pub ascension_level: u8,
    pub player_hp: i32,
    pub player_max_hp: i32,
    pub player_block: i32,
    pub energy: u8,
    pub turn_count: u32,
    pub hand: Vec<CombatCaptureCardSummaryV3>,
    pub draw_count: usize,
    pub discard_count: usize,
    pub exhaust_count: usize,
    pub limbo_count: usize,
    pub queued_cards_count: usize,
    pub potions: Vec<Option<CombatCapturePotionSummaryV3>>,
    pub monsters: Vec<CombatCaptureMonsterSummaryV3>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaptureCardSummaryV3 {
    pub lineage: CardLineageId,
    pub card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCapturePotionSummaryV3 {
    pub uuid: u32,
    pub potion_id: String,
    pub can_use: bool,
    pub can_discard: bool,
    pub requires_target: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCaptureMonsterSummaryV3 {
    pub slot: u8,
    pub entity_id: usize,
    pub enemy_id: String,
    pub hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub alive: bool,
    pub escaped: bool,
    pub dying: bool,
    pub half_dead: bool,
    pub planned_move_id: u8,
    pub visible_intent: String,
    pub preview_damage_per_hit: i32,
}

impl CombatCaptureV3 {
    pub fn position(&self) -> &CombatPosition {
        &self.position
    }
}

pub fn capture_combat_position_v3(
    label: Option<String>,
    position: &CombatPosition,
) -> Result<CombatCaptureV3, String> {
    capture_combat_position_with_provenance_v3(
        label,
        position,
        ArtifactProvenanceV1::exact_combat_position(),
    )
}

pub fn capture_combat_position_from_run_v3(
    label: Option<String>,
    position: &CombatPosition,
    run_state: &RunState,
) -> Result<CombatCaptureV3, String> {
    capture_combat_position_with_provenance_v3(
        label,
        position,
        ArtifactProvenanceV1::manual_run_control(run_state),
    )
}

pub fn capture_combat_position_from_runtime_progress_v3(
    label: Option<String>,
    position: &CombatPosition,
    run_state: &RunState,
) -> Result<CombatCaptureV3, String> {
    capture_combat_position_with_provenance_v3(
        label,
        position,
        ArtifactProvenanceV1::runtime_progress(run_state),
    )
}

pub fn capture_combat_position_with_provenance_v3(
    label: Option<String>,
    position: &CombatPosition,
    provenance: ArtifactProvenanceV1,
) -> Result<CombatCaptureV3, String> {
    if !active_combat_capture_boundary(&position.engine, &position.combat) {
        return Err(
            "CombatCaptureV3 requires an active stable combat decision boundary".to_string(),
        );
    }
    validate_combat_card_identity_for_capture(&position.combat)?;

    let integrity = integrity_for_position(position);
    let fingerprint_bundle = combat_fingerprint_bundle_v3(position);
    let summary = summary_for_position(position);
    let source_capture_method = provenance.capture_method.clone();
    Ok(CombatCaptureV3 {
        schema_name: COMBAT_CAPTURE_SCHEMA_NAME.to_string(),
        schema_version: COMBAT_CAPTURE_SCHEMA_VERSION,
        header: default_combat_capture_header(),
        capture_kind: CombatCaptureKindV3::CombatPosition,
        trust_level: ArtifactTrustLevel::Restorable,
        information_boundary: "engine_truth_exact_combat_position".to_string(),
        label,
        provenance,
        source: CombatCaptureSourceV3 {
            producer: ARTIFACT_PRODUCER.to_string(),
            capture_method: source_capture_method,
        },
        integrity,
        fingerprints: fingerprint_bundle.state,
        legal_action_surface: fingerprint_bundle.legal_action_surface,
        summary,
        position: position.clone(),
    })
}

pub fn load_combat_capture_v3(path: &Path) -> Result<CombatCaptureV3, String> {
    let payload = fs::read_to_string(path).map_err(|err| err.to_string())?;
    reject_legacy_capture_schema(&payload)?;
    let capture: CombatCaptureV3 = serde_json::from_str(&payload).map_err(|err| err.to_string())?;
    validate_combat_capture_v3(&capture)?;
    Ok(capture)
}

pub fn save_combat_capture_v3(path: &Path, capture: &CombatCaptureV3) -> Result<(), String> {
    validate_combat_capture_v3(capture)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let payload = serde_json::to_string_pretty(capture).map_err(|err| err.to_string())?;
    fs::write(path, payload).map_err(|err| err.to_string())
}

pub fn validate_combat_capture_v3(capture: &CombatCaptureV3) -> Result<(), String> {
    if capture.schema_name != COMBAT_CAPTURE_SCHEMA_NAME {
        return Err(format!(
            "unsupported combat capture schema '{}'",
            capture.schema_name
        ));
    }
    if capture.schema_version != COMBAT_CAPTURE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported combat capture schema_version {}",
            capture.schema_version
        ));
    }
    if capture.capture_kind != CombatCaptureKindV3::CombatPosition {
        return Err("unsupported combat capture kind".to_string());
    }
    if capture.header != default_combat_capture_header() {
        return Err("combat capture artifact header does not match schema".to_string());
    }
    validate_capture_provenance(&capture.provenance)?;
    if capture.integrity.fingerprint_algorithm != COMBAT_CAPTURE_INTEGRITY_ALGORITHM_V3 {
        return Err(format!(
            "unsupported combat capture fingerprint algorithm '{}'; expected '{}'; regenerate this artifact after the stable gameplay identity V3 cutover",
            capture.integrity.fingerprint_algorithm, COMBAT_CAPTURE_INTEGRITY_ALGORITHM_V3
        ));
    }
    if !active_combat_capture_boundary(&capture.position.engine, &capture.position.combat) {
        return Err(
            "combat capture position is not an active stable combat decision boundary".to_string(),
        );
    }
    validate_combat_card_identity_for_capture(&capture.position.combat)?;

    let expected = integrity_for_position(&capture.position);
    if capture.integrity != expected {
        return Err("combat capture integrity fingerprints do not match position".to_string());
    }
    let expected_fingerprint_bundle = combat_fingerprint_bundle_v3(&capture.position);
    if capture.fingerprints != expected_fingerprint_bundle.state {
        return Err("combat capture state fingerprints do not match position".to_string());
    }
    if capture.legal_action_surface != expected_fingerprint_bundle.legal_action_surface {
        return Err("combat capture legal action surface does not match position".to_string());
    }
    if capture.summary != summary_for_position(&capture.position) {
        return Err("combat capture summary does not match position".to_string());
    }
    Ok(())
}

fn integrity_for_position(position: &CombatPosition) -> CombatCaptureIntegrityV3 {
    let exact = combat_exact_state_key(&position.engine, &position.combat);
    let stable = stable_dominance_bucket_key(&position.engine, &position.combat)
        .map(|_| stable_outcome_key(&position.engine, &position.combat));
    CombatCaptureIntegrityV3 {
        fingerprint_algorithm: COMBAT_CAPTURE_INTEGRITY_ALGORITHM_V3.to_string(),
        exact_state_fingerprint: combat_exact_state_key_hash_v2(&exact),
        stable_outcome_fingerprint: stable
            .as_ref()
            .map(crate::eval::fingerprint::hash_serializable),
    }
}

fn reject_legacy_capture_schema(payload: &str) -> Result<(), String> {
    #[derive(Deserialize)]
    struct CaptureSchemaProbe {
        schema_name: String,
        schema_version: u32,
    }
    let probe: CaptureSchemaProbe = serde_json::from_str(payload).map_err(|err| err.to_string())?;
    if probe.schema_name == COMBAT_CAPTURE_SCHEMA_NAME
        && probe.schema_version == COMBAT_CAPTURE_SCHEMA_VERSION
    {
        return Ok(());
    }
    Err(format!(
        "unsupported combat capture schema '{}'/{}; production accepts CombatCaptureV3 only; regenerate captures after the stable gameplay identity cutover",
        probe.schema_name, probe.schema_version
    ))
}

fn validate_capture_provenance(provenance: &ArtifactProvenanceV1) -> Result<(), String> {
    if provenance.producer.trim().is_empty() {
        return Err("combat capture provenance producer cannot be empty".to_string());
    }
    if provenance.capture_method.trim().is_empty() {
        return Err("combat capture provenance capture_method cannot be empty".to_string());
    }
    if provenance.trainable_as_action_label || provenance.policy_quality_claim {
        return Err(
            "combat capture provenance must not claim teacher-label or policy-quality authority"
                .to_string(),
        );
    }
    Ok(())
}

fn active_combat_capture_boundary(
    engine: &EngineState,
    combat: &crate::runtime::combat::CombatState,
) -> bool {
    stable_boundary(engine, combat)
        && matches!(
            engine,
            EngineState::CombatPlayerTurn | EngineState::PendingChoice(_)
        )
}

fn summary_for_position(position: &CombatPosition) -> CombatCaptureSummaryV3 {
    let combat = &position.combat;
    CombatCaptureSummaryV3 {
        engine_state: position.engine.kind_v1().to_string(),
        terminal: combat_terminal(&position.engine, combat),
        stable_boundary: stable_boundary(&position.engine, combat),
        player_class: combat.meta.player_class.clone(),
        ascension_level: combat.meta.ascension_level,
        player_hp: combat.entities.player.current_hp,
        player_max_hp: combat.entities.player.max_hp,
        player_block: combat.entities.player.block,
        energy: combat.turn.energy,
        turn_count: combat.turn.turn_count,
        hand: combat
            .zones
            .hand
            .iter()
            .map(|card| CombatCaptureCardSummaryV3 {
                lineage: card.lineage,
                card_id: card.id.stable_id().to_string(),
                upgrades: card.upgrades,
                cost_for_turn: card.cost_for_turn_java(),
            })
            .collect(),
        draw_count: combat.zones.draw_pile.len(),
        discard_count: combat.zones.discard_pile.len(),
        exhaust_count: combat.zones.exhaust_pile.len(),
        limbo_count: combat.zones.limbo.len(),
        queued_cards_count: combat.zones.queued_cards.len(),
        potions: combat
            .entities
            .potions
            .iter()
            .map(|slot| {
                slot.as_ref().map(|potion| CombatCapturePotionSummaryV3 {
                    uuid: potion.uuid,
                    potion_id: potion.id.stable_id().to_string(),
                    can_use: potion.can_use,
                    can_discard: potion.can_discard,
                    requires_target: potion.requires_target,
                })
            })
            .collect(),
        monsters: combat
            .entities
            .monsters
            .iter()
            .map(|monster| {
                let observation = combat
                    .runtime
                    .monster_protocol
                    .get(&monster.id)
                    .map(|protocol| &protocol.observation);
                let turn_plan = monster.turn_plan();
                CombatCaptureMonsterSummaryV3 {
                    slot: monster.slot,
                    entity_id: monster.id,
                    enemy_id: monster.enemy_id.stable_id().to_owned(),
                    hp: monster.current_hp,
                    max_hp: monster.max_hp,
                    block: monster.block,
                    alive: monster.is_alive_for_action(),
                    escaped: monster.is_escaped,
                    dying: monster.is_dying,
                    half_dead: monster.half_dead,
                    planned_move_id: monster.planned_move_id(),
                    visible_intent: observation
                        .filter(|obs| obs.visible_intent != Intent::Unknown)
                        .map(|obs| obs.visible_intent.protocol_token_v1())
                        .unwrap_or_else(|| {
                            Intent::from_visible_move_spec(&turn_plan.summary_spec())
                                .protocol_token_v1()
                        }),
                    preview_damage_per_hit: observation
                        .filter(|obs| obs.preview_damage_per_hit > 0)
                        .map(|obs| obs.preview_damage_per_hit)
                        .or_else(|| turn_plan.attack().map(|attack| attack.base_damage))
                        .unwrap_or(0),
                }
            })
            .collect(),
    }
}

fn default_combat_capture_header() -> ArtifactHeaderV1 {
    ArtifactHeaderV1::new(
        COMBAT_CAPTURE_SCHEMA_NAME,
        COMBAT_CAPTURE_SCHEMA_VERSION,
        "combat_capture",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::artifact::{ArtifactSourceKind, ArtifactTrustLevel};
    use crate::fixtures::combat_start_spec::{compile_combat_start_spec, CombatStartSpec};
    use crate::sim::combat::CombatPosition;

    #[test]
    fn combat_capture_roundtrips_exact_position() {
        let position = jaw_worm_position();
        let capture = capture_combat_position_v3(Some("jaw_worm_capture".to_string()), &position)
            .expect("stable combat start should capture");

        let payload = serde_json::to_string_pretty(&capture).expect("capture should serialize");
        let loaded: CombatCaptureV3 =
            serde_json::from_str(&payload).expect("capture should deserialize");

        validate_combat_capture_v3(&loaded).expect("loaded capture should validate");
        assert_eq!(loaded.position, position);
        assert_eq!(loaded.schema_name, "CombatCaptureV3");
        assert_eq!(loaded.schema_version, COMBAT_CAPTURE_SCHEMA_VERSION);
        assert_eq!(loaded.header.schema_name, "CombatCaptureV3");
        assert_eq!(loaded.header.schema_version, COMBAT_CAPTURE_SCHEMA_VERSION);
        assert_eq!(loaded.fingerprints.schema_name, "StateFingerprintV3");
        assert_eq!(
            loaded.fingerprints.schema_version,
            crate::eval::fingerprint::FINGERPRINT_SCHEMA_VERSION
        );
        assert_eq!(loaded.header, default_combat_capture_header());
        assert_eq!(loaded.trust_level, ArtifactTrustLevel::Restorable);
        assert_eq!(
            loaded.provenance.source_kind,
            ArtifactSourceKind::ExactCombatPosition
        );
        assert_eq!(loaded.integrity, capture.integrity);
        assert_eq!(loaded.fingerprints, capture.fingerprints);
        assert_eq!(
            loaded.integrity.exact_state_fingerprint, loaded.fingerprints.exact_state_hash,
            "capture integrity and state fingerprint must share one exact identity owner"
        );
        assert_eq!(loaded.legal_action_surface, capture.legal_action_surface);
        assert_eq!(
            loaded.fingerprints.legal_input_language_hash,
            loaded.legal_action_surface.legal_input_language_digest
        );
        assert_eq!(
            loaded.fingerprints.action_enumeration_domain_hash,
            loaded.legal_action_surface.enumeration_domain_digest
        );
        assert!(loaded.legal_action_surface.atomic_action_count > 0);
        assert!(!loaded.legal_action_surface.atomic_actions.is_empty());
        assert_eq!(loaded.summary, capture.summary);

        let mut value = serde_json::to_value(&capture).expect("serialize capture value");
        value.as_object_mut().unwrap().remove("header");
        let error = serde_json::from_value::<CombatCaptureV3>(value)
            .expect_err("current captures require their artifact header");
        assert!(error.to_string().contains("header"), "{error}");
    }

    #[test]
    fn combat_capture_validation_rejects_tampered_summary() {
        let position = jaw_worm_position();
        let mut capture = capture_combat_position_v3(None, &position).expect("capture should work");
        capture.summary.player_hp -= 1;

        let err =
            validate_combat_capture_v3(&capture).expect_err("tampered summary should be rejected");

        assert!(err.contains("summary"));
    }

    #[test]
    fn combat_capture_validation_rejects_tampered_fingerprint() {
        let position = jaw_worm_position();
        let mut capture = capture_combat_position_v3(None, &position).expect("capture should work");
        capture.integrity.exact_state_fingerprint =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        let err = validate_combat_capture_v3(&capture)
            .expect_err("tampered fingerprint should be rejected");

        assert!(err.contains("fingerprints"));
    }

    #[test]
    fn combat_capture_validation_rejects_pre_semantic_exact_identities_explicitly() {
        let position = jaw_worm_position();
        for obsolete_algorithm in [
            "blake2b_256_of_typed_key_debug_v1",
            "blake2b_256_of_typed_key_debug_v2",
        ] {
            let mut capture =
                capture_combat_position_v3(None, &position).expect("capture should work");
            capture.integrity.fingerprint_algorithm = obsolete_algorithm.to_string();

            let err = validate_combat_capture_v3(&capture)
                .expect_err("Debug-derived exact identities must be regenerated");

            assert!(err.contains("stable gameplay identity V3 cutover"));
            assert!(err.contains("regenerate"));
        }
    }

    #[test]
    fn combat_capture_validation_rejects_tampered_state_fingerprint() {
        let position = jaw_worm_position();
        let mut capture = capture_combat_position_v3(None, &position).expect("capture should work");
        capture.fingerprints.public_observation_hash =
            "0000000000000000000000000000000000000000000000000000000000000000".to_string();

        let err = validate_combat_capture_v3(&capture)
            .expect_err("tampered state fingerprint should be rejected");

        assert!(err.contains("state fingerprints"));
    }

    #[test]
    fn combat_capture_roundtrips_large_symbolic_scry_without_action_enumeration() {
        let mut position = jaw_worm_position();
        let mut card_lineages = Vec::with_capacity(64);
        let mut cards = Vec::with_capacity(64);
        for _ in 0..64 {
            let lineage = position.combat.allocate_combat_generated_lineage();
            card_lineages.push(lineage);
            cards.push(crate::runtime::combat::CombatCard::with_lineage(
                crate::content::cards::CardId::Strike,
                lineage,
            ));
        }
        position.combat.zones.draw_pile = cards.into();
        position.engine =
            EngineState::PendingChoice(crate::state::core::PendingChoice::ScrySelect {
                cards: vec![crate::content::cards::CardId::Strike; 64],
                card_lineages,
            });

        let capture = capture_combat_position_v3(None, &position)
            .expect("large Scry should capture through one symbolic family");
        assert_eq!(capture.legal_action_surface.atomic_action_count, 0);
        assert_eq!(capture.legal_action_surface.action_family_count, 1);
        assert_eq!(
            capture.legal_action_surface.selection_families[0].raw_domain_count,
            64
        );

        let payload = serde_json::to_string(&capture).expect("capture should serialize");
        let loaded: CombatCaptureV3 =
            serde_json::from_str(&payload).expect("capture should deserialize");
        validate_combat_capture_v3(&loaded).expect("symbolic capture should validate");
    }

    #[test]
    fn combat_capture_validation_rejects_tampered_action_family() {
        let position = jaw_worm_position();
        let mut capture = capture_combat_position_v3(None, &position).expect("capture should work");
        capture.legal_action_surface.legal_input_language_digest = "0".repeat(64);

        let err = validate_combat_capture_v3(&capture)
            .expect_err("tampered action surface should be rejected");

        assert!(err.contains("legal action surface"));
    }

    #[test]
    fn legacy_capture_schema_is_rejected_before_v1_action_cache_validation() {
        let payload = r#"{
            "schema_name": "CombatCaptureV1",
            "schema_version": 1,
            "legal_actions": {"count": 18446744073709551615}
        }"#;
        let path = unique_temp_capture_path("legacy_v1");
        fs::write(&path, payload).expect("write legacy capture fixture");

        let err = load_combat_capture_v3(&path)
            .expect_err("V1 action caches are not authoritative migration evidence");
        let _ = fs::remove_file(path);

        assert!(err.contains("unsupported combat capture schema"));
        assert!(err.contains("CombatCaptureV1"));
    }

    #[test]
    fn combat_capture_validation_rejects_card_lineage_identity_conflict() {
        let mut position = jaw_worm_position();
        let lineage = position.combat.zones.hand[0].lineage;
        position
            .combat
            .zones
            .discard_pile
            .push(crate::runtime::combat::CombatCard::with_lineage(
                crate::content::cards::CardId::Slimed,
                lineage,
            ));

        let err = capture_combat_position_v3(None, &position)
            .expect_err("capture should reject same lineage mapping to different card ids");

        assert!(err.contains("card identity conflict"));
        assert!(err.contains("active lineage"));
    }

    #[test]
    fn combat_capture_validation_rejects_unallocated_generated_lineage() {
        let mut position = jaw_worm_position();
        let unallocated = CardLineageId::combat_generated(
            position
                .combat
                .zones
                .combat_lineage_allocator
                .next_creation_ordinal(),
        );
        position
            .combat
            .zones
            .discard_pile
            .push(crate::runtime::combat::CombatCard::with_lineage(
                crate::content::cards::CardId::Slimed,
                unallocated,
            ));

        let err = capture_combat_position_v3(None, &position)
            .expect_err("capture should reject lineage not issued by the combat allocator");

        assert!(err.contains("combat-generated lineage"));
        assert!(err.contains("not allocated"));
    }

    #[test]
    fn combat_capture_rejects_postcombat_engine_boundaries() {
        let mut position = jaw_worm_position();
        position.engine = EngineState::RewardScreen(crate::state::rewards::RewardState::new());

        let err = capture_combat_position_v3(None, &position)
            .expect_err("postcombat boundary should not be a search start capture");

        assert!(err.contains("active stable combat decision boundary"));
    }

    #[test]
    fn combat_capture_rejects_combat_start_request() {
        let mut position = jaw_worm_position();
        position.engine = EngineState::CombatStart(crate::state::core::CombatStartRequest::event(
            crate::content::monsters::factory::EncounterId::JawWorm,
            crate::state::rewards::RewardState::new(),
            true,
            false,
            false,
            crate::state::core::PostCombatReturn::MapNavigation,
        ));

        let err = capture_combat_position_v3(None, &position)
            .expect_err("CombatStart should not be a search start capture");

        assert!(err.contains("active stable combat decision boundary"));
    }

    fn jaw_worm_position() -> CombatPosition {
        let spec: CombatStartSpec = serde_json::from_str(
            r#"{
                "name": "jaw_worm_starter",
                "player_class": "Ironclad",
                "ascension_level": 0,
                "encounter_id": "JawWorm",
                "room_type": "monster",
                "seed": 1,
                "player_current_hp": 80,
                "player_max_hp": 80,
                "master_deck": [
                    {"id": "Strike_R", "count": 5},
                    {"id": "Defend_R", "count": 4},
                    "Bash"
                ]
            }"#,
        )
        .expect("test start spec should parse");
        let (engine, combat) =
            compile_combat_start_spec(&spec).expect("test start spec should compile");
        CombatPosition::new(engine, combat)
    }

    fn unique_temp_capture_path(label: &str) -> std::path::PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should follow Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sts_simulator_{label}_{}_{}.json",
            std::process::id(),
            nonce
        ))
    }
}
