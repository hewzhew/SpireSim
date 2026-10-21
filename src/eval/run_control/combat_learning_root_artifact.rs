//! Versioned handoff from production run checkpoints into combat learning.
//!
//! The envelope deliberately contains only exact run-control checkpoints plus
//! their independently recomputed combat-root identity and compact context.
//! Python callers may carry the opaque bytes but never decode simulator state.

use std::collections::BTreeSet;
use std::io::{self, Cursor, Write};

use serde::{Deserialize, Serialize};

use crate::eval::combat_case::{
    capture_production_combat_case_v2, CombatCase, CombatCaseGap, CombatCaseSource,
    CombatCaseWitnessBudgetV1,
};

use super::{
    CombatLearningRootContextV1, CombatLearningRootIdentityV1, CombatLearningRootV1,
    RunControlSessionCheckpointV1,
};

pub const COMBAT_LEARNING_ROOT_ARTIFACT_MAGIC: &[u8] = b"STS-COMBAT-LEARNING-ROOTS\0";
pub const COMBAT_LEARNING_ROOT_ARTIFACT_FORMAT_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningRootArtifactV1 {
    identity: CombatLearningRootIdentityV1,
    context: CombatLearningRootContextV1,
    session: RunControlSessionCheckpointV1,
}

impl CombatLearningRootArtifactV1 {
    pub fn identity(&self) -> &CombatLearningRootIdentityV1 {
        &self.identity
    }

    pub fn context(&self) -> &CombatLearningRootContextV1 {
        &self.context
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatLearningRootBatchArtifactV1 {
    roots: Vec<CombatLearningRootArtifactV1>,
}

impl CombatLearningRootBatchArtifactV1 {
    /// Capture exact roots without exposing their private run-control checkpoints
    /// to downstream callers.
    pub fn from_roots<'a>(
        roots: impl IntoIterator<Item = &'a CombatLearningRootV1>,
    ) -> Result<Self, String> {
        Self::from_checkpoints(
            roots
                .into_iter()
                .map(CombatLearningRootV1::session_checkpoint),
        )
    }

    pub fn from_checkpoints(
        checkpoints: impl IntoIterator<Item = RunControlSessionCheckpointV1>,
    ) -> Result<Self, String> {
        let roots = checkpoints
            .into_iter()
            .map(|session| {
                let root = CombatLearningRootV1::from_checkpoint(session.clone())?;
                Ok(CombatLearningRootArtifactV1 {
                    identity: root.identity().clone(),
                    context: *root.context(),
                    session,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let artifact = Self { roots };
        artifact.validate(None)?;
        Ok(artifact)
    }

    pub fn roots(&self) -> &[CombatLearningRootArtifactV1] {
        &self.roots
    }

    /// Retain an explicit non-empty ordered subset without exposing checkpoint
    /// fields. Caller order becomes the canonical order of the new artifact.
    pub fn select_root_slots(
        &self,
        root_slots: impl IntoIterator<Item = usize>,
    ) -> Result<Self, String> {
        self.validate(None)?;
        let root_slots = root_slots.into_iter().collect::<Vec<_>>();
        if root_slots.is_empty() {
            return Err("combat learning root selection requires at least one slot".to_owned());
        }
        let mut distinct = BTreeSet::new();
        let roots = root_slots
            .into_iter()
            .map(|slot| {
                if !distinct.insert(slot) {
                    return Err(format!(
                        "combat learning root selection repeats slot {slot}"
                    ));
                }
                self.roots.get(slot).cloned().ok_or_else(|| {
                    format!("combat learning root selection slot {slot} is out of range")
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let artifact = Self { roots };
        artifact.validate(None)?;
        Ok(artifact)
    }

    pub fn encode(&self, max_bytes: usize) -> Result<Vec<u8>, String> {
        self.validate(None)?;
        encode_artifact(self, max_bytes)
    }

    pub fn decode(
        payload: &[u8],
        expected_root_count: usize,
        max_bytes: usize,
    ) -> Result<Self, String> {
        if expected_root_count == 0 {
            return Err("combat learning root artifact expected count must be positive".to_owned());
        }
        if payload.len() > max_bytes {
            return Err(
                "combat learning root artifact exceeds its caller-provided byte limit".to_owned(),
            );
        }
        let header_bytes = artifact_header_bytes();
        if payload.len() < header_bytes {
            return Err("combat learning root artifact ended before its header".to_owned());
        }
        if &payload[..COMBAT_LEARNING_ROOT_ARTIFACT_MAGIC.len()]
            != COMBAT_LEARNING_ROOT_ARTIFACT_MAGIC
        {
            return Err("combat learning root artifact magic is invalid".to_owned());
        }
        let version_start = COMBAT_LEARNING_ROOT_ARTIFACT_MAGIC.len();
        let encoded_version = u32::from_be_bytes(
            payload[version_start..header_bytes]
                .try_into()
                .map_err(|_| "combat learning root artifact version is truncated")?,
        );
        if encoded_version != COMBAT_LEARNING_ROOT_ARTIFACT_FORMAT_VERSION {
            return Err("combat learning root artifact format version is unsupported".to_owned());
        }

        let mut decoder = rmp_serde::Deserializer::new(Cursor::new(&payload[header_bytes..]));
        let artifact = Self::deserialize(&mut decoder)
            .map_err(|error| format!("cannot decode combat learning root artifact: {error}"))?;
        if usize::try_from(decoder.position()).ok() != Some(payload.len() - header_bytes) {
            return Err("combat learning root artifact contains trailing bytes".to_owned());
        }
        artifact.validate(Some(expected_root_count))?;
        if encode_artifact(&artifact, max_bytes)? != payload {
            return Err("combat learning root artifact encoding is not canonical".to_owned());
        }
        Ok(artifact)
    }

    pub fn into_checkpoints(self) -> Result<Vec<RunControlSessionCheckpointV1>, String> {
        self.validate(None)?;
        Ok(self.roots.into_iter().map(|root| root.session).collect())
    }

    /// Merge canonical single-root payloads without exposing checkpoint fields.
    pub fn merge_single_root_payloads<'a>(
        payloads: impl IntoIterator<Item = &'a [u8]>,
        max_input_bytes: usize,
    ) -> Result<Self, String> {
        Self::merge_canonical_payloads(
            payloads.into_iter().map(|payload| (payload, 1)),
            max_input_bytes,
        )
    }

    /// Merge canonical payloads with an explicit expected width for each input
    /// without exposing checkpoint fields.
    pub fn merge_canonical_payloads<'a>(
        payloads: impl IntoIterator<Item = (&'a [u8], usize)>,
        max_input_bytes: usize,
    ) -> Result<Self, String> {
        let mut total_input_bytes = 0usize;
        let mut checkpoints = Vec::new();
        for (index, (payload, expected_root_count)) in payloads.into_iter().enumerate() {
            total_input_bytes = total_input_bytes
                .checked_add(payload.len())
                .ok_or_else(|| "combat learning root merge input byte count overflow".to_owned())?;
            if total_input_bytes > max_input_bytes {
                return Err("combat learning root merge exceeds its input byte limit".to_owned());
            }
            let artifact =
                Self::decode(payload, expected_root_count, max_input_bytes).map_err(|error| {
                    format!("combat learning root merge input {index} is invalid: {error}")
                })?;
            checkpoints.extend(artifact.into_checkpoints()?);
        }
        if checkpoints.is_empty() {
            return Err("combat learning root merge requires at least one input".to_owned());
        }
        Self::from_checkpoints(checkpoints)
    }

    fn validate(&self, expected_root_count: Option<usize>) -> Result<(), String> {
        if self.roots.is_empty() {
            return Err("combat learning root artifact must contain at least one root".to_owned());
        }
        if let Some(expected) = expected_root_count {
            if self.roots.len() != expected {
                return Err(format!(
                    "combat learning root artifact contains {} roots, expected {expected}",
                    self.roots.len()
                ));
            }
        }

        let mut identities = BTreeSet::new();
        for (index, captured) in self.roots.iter().enumerate() {
            let recomputed = CombatLearningRootV1::from_checkpoint(captured.session.clone())
                .map_err(|error| {
                    format!("combat learning root artifact root {index} is invalid: {error}")
                })?;
            if recomputed.identity() != captured.identity() {
                return Err(format!(
                    "combat learning root artifact root {index} identity does not match its session"
                ));
            }
            if recomputed.context() != captured.context() {
                return Err(format!(
                    "combat learning root artifact root {index} context does not match its session"
                ));
            }
            if !identities.insert(captured.identity.root_id.as_str()) {
                return Err(format!(
                    "combat learning root artifact repeats exact root {}",
                    captured.identity.root_id
                ));
            }
        }
        Ok(())
    }
}

/// Decode one opaque learning root into the standard replayable production
/// combat-case boundary. This remains an evaluator/tooling operation: actor
/// views never receive the private checkpoint contained by the artifact.
pub fn production_combat_case_from_root_artifact_v1(
    payload: &[u8],
    expected_root_count: usize,
    root_slot: usize,
    max_bytes: usize,
) -> Result<CombatCase, String> {
    let artifact =
        CombatLearningRootBatchArtifactV1::decode(payload, expected_root_count, max_bytes)?;
    let expected_exact_state_hash = artifact
        .roots()
        .get(root_slot)
        .ok_or_else(|| {
            format!(
                "combat learning root slot {root_slot} is outside artifact width {expected_root_count}"
            )
        })?
        .identity()
        .exact_combat_state_hash
        .clone();
    let checkpoint = artifact
        .into_checkpoints()?
        .into_iter()
        .nth(root_slot)
        .ok_or_else(|| format!("combat learning root slot {root_slot} is absent"))?;
    let session = checkpoint.into_session()?;
    let case = capture_production_combat_case_v2(
        &session,
        CombatCaseSource {
            seed: session.run_state.seed,
            ascension: session.run_state.ascension_level,
            generation: 0,
            branch_id: 0,
            parent_id: None,
        },
        CombatCaseGap {
            boundary: "learning_root_case_export".to_owned(),
            reason: "exact opaque learning root selected for bounded diagnosis".to_owned(),
            witness_budget: CombatCaseWitnessBudgetV1::NotRun,
        },
    )?;
    if case.replay_identity_v2()?.root_exact_state_hash != expected_exact_state_hash {
        return Err("learning root case export changed the exact combat state".to_owned());
    }
    Ok(case)
}

struct BoundedArtifactWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
}

impl BoundedArtifactWriter {
    fn new(max_bytes: usize) -> Result<Self, String> {
        let header_bytes = artifact_header_bytes();
        if max_bytes < header_bytes {
            return Err(
                "combat learning root artifact byte limit is smaller than its header".to_owned(),
            );
        }
        let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
        bytes.extend_from_slice(COMBAT_LEARNING_ROOT_ARTIFACT_MAGIC);
        bytes.extend_from_slice(&COMBAT_LEARNING_ROOT_ARTIFACT_FORMAT_VERSION.to_be_bytes());
        Ok(Self { bytes, max_bytes })
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedArtifactWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next =
            self.bytes.len().checked_add(buffer.len()).ok_or_else(|| {
                io::Error::other("combat learning root artifact byte count overflow")
            })?;
        if next > self.max_bytes {
            return Err(io::Error::other(
                "combat learning root artifact exceeds its caller-provided byte limit",
            ));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn artifact_header_bytes() -> usize {
    COMBAT_LEARNING_ROOT_ARTIFACT_MAGIC.len() + std::mem::size_of::<u32>()
}

fn encode_artifact(
    artifact: &CombatLearningRootBatchArtifactV1,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut writer = BoundedArtifactWriter::new(max_bytes)?;
    rmp_serde::encode::write_named(&mut writer, artifact)
        .map_err(|error| format!("cannot encode combat learning root artifact: {error}"))?;
    Ok(writer.finish())
}

#[cfg(test)]
mod tests {
    use crate::content::cards::CardId;
    use crate::content::monsters::exordium::jaw_worm::JawWorm;
    use crate::content::monsters::{EnemyId, MonsterBehavior};
    use crate::runtime::combat::CombatCard;
    use crate::state::core::{ActiveCombat, CombatContext, EngineState, RoomCombatContext};
    use crate::state::map::node::RoomType;
    use crate::test_support::{blank_test_combat, test_monster};

    use super::*;
    use crate::eval::run_control::{LearningBoundaryV1, LearningEnvV3, RunControlSession};

    #[test]
    fn artifact_round_trip_recomputes_exact_combat_roots() {
        let first = combat_root_checkpoint(20);
        let second = combat_root_checkpoint(21);
        let artifact =
            CombatLearningRootBatchArtifactV1::from_checkpoints([first.clone(), second.clone()])
                .expect("capture exact roots");
        let payload = artifact.encode(1024 * 1024).expect("encode roots");
        let restored = CombatLearningRootBatchArtifactV1::decode(&payload, 2, 1024 * 1024)
            .expect("decode roots");

        assert_eq!(restored.roots().len(), 2);
        for checkpoint in restored.into_checkpoints().expect("validated checkpoints") {
            assert!(matches!(
                LearningEnvV3::from_session(checkpoint.into_session().expect("restore session"))
                    .observe()
                    .expect("observe restored combat"),
                LearningBoundaryV1::Combat { .. }
            ));
        }
        assert!(CombatLearningRootBatchArtifactV1::decode(&payload, 1, 1024 * 1024,).is_err());
        assert!(CombatLearningRootBatchArtifactV1::decode(&payload, 2, 16).is_err());
        let mut legacy_version = payload.clone();
        let version_start = COMBAT_LEARNING_ROOT_ARTIFACT_MAGIC.len();
        legacy_version[version_start..version_start + std::mem::size_of::<u32>()]
            .copy_from_slice(&1u32.to_be_bytes());
        let error = CombatLearningRootBatchArtifactV1::decode(&legacy_version, 2, 1024 * 1024)
            .expect_err("reject the pre-encounter-identity artifact version");
        assert!(error.contains("format version is unsupported"));
    }

    #[test]
    fn opaque_root_exports_one_identity_preserving_production_combat_case() {
        let artifact =
            CombatLearningRootBatchArtifactV1::from_checkpoints([combat_root_checkpoint(20)])
                .expect("capture exact root");
        let expected = artifact.roots()[0].identity().clone();
        let payload = artifact.encode(1024 * 1024).expect("encode root");

        let case = production_combat_case_from_root_artifact_v1(&payload, 1, 0, 1024 * 1024)
            .expect("export production combat case");

        assert!(case.production_context.is_some());
        assert_eq!(
            case.replay_identity_v2()
                .expect("production replay identity")
                .root_exact_state_hash,
            expected.exact_combat_state_hash
        );
        assert_eq!(case.core.combat.enemies, vec!["JawWorm"]);
        assert!(
            production_combat_case_from_root_artifact_v1(&payload, 1, 1, 1024 * 1024,).is_err()
        );
    }

    #[test]
    fn artifact_rejects_non_combat_duplicate_and_tampered_roots() {
        let non_combat = RunControlSessionCheckpointV1::from_session(&RunControlSession::new(
            Default::default(),
        ));
        assert!(CombatLearningRootBatchArtifactV1::from_checkpoints([non_combat]).is_err());

        let root = combat_root_checkpoint(20);
        assert!(
            CombatLearningRootBatchArtifactV1::from_checkpoints([root.clone(), root.clone(),])
                .is_err()
        );

        let mut artifact =
            CombatLearningRootBatchArtifactV1::from_checkpoints([root]).expect("capture root");
        artifact.roots[0].context.hp -= 1;
        assert!(artifact.encode(1024 * 1024).is_err());
    }

    #[test]
    fn artifact_selects_distinct_root_slots_in_caller_order() {
        let artifact = CombatLearningRootBatchArtifactV1::from_checkpoints([
            combat_root_checkpoint(20),
            combat_root_checkpoint(21),
            combat_root_checkpoint(22),
        ])
        .expect("capture roots");

        let selected = artifact
            .select_root_slots([2, 0])
            .expect("select exact roots");

        assert_eq!(selected.roots().len(), 2);
        assert_eq!(selected.roots()[0].context(), artifact.roots()[2].context());
        assert_eq!(
            selected.roots()[0].identity(),
            artifact.roots()[2].identity()
        );
        assert_eq!(
            selected.roots()[1].identity(),
            artifact.roots()[0].identity()
        );
        assert!(artifact.select_root_slots([]).is_err());
        assert!(artifact.select_root_slots([1, 1]).is_err());
        assert!(artifact.select_root_slots([3]).is_err());
    }

    #[test]
    fn artifact_merges_distinct_canonical_single_root_payloads() {
        let first =
            CombatLearningRootBatchArtifactV1::from_checkpoints([combat_root_checkpoint(20)])
                .expect("capture first root")
                .encode(1024 * 1024)
                .expect("encode first root");
        let second =
            CombatLearningRootBatchArtifactV1::from_checkpoints([combat_root_checkpoint(21)])
                .expect("capture second root")
                .encode(1024 * 1024)
                .expect("encode second root");

        let merged = CombatLearningRootBatchArtifactV1::merge_single_root_payloads(
            [first.as_slice(), second.as_slice()],
            1024 * 1024,
        )
        .expect("merge distinct roots");

        assert_eq!(merged.roots().len(), 2);
        assert!(
            CombatLearningRootBatchArtifactV1::merge_single_root_payloads(
                [first.as_slice(), second.as_slice()],
                first.len() + second.len() - 1,
            )
            .is_err()
        );
        assert!(
            CombatLearningRootBatchArtifactV1::merge_single_root_payloads(
                [first.as_slice(), first.as_slice()],
                1024 * 1024,
            )
            .is_err()
        );
        assert!(
            CombatLearningRootBatchArtifactV1::merge_single_root_payloads(
                std::iter::empty::<&[u8]>(),
                1024 * 1024,
            )
            .is_err()
        );
    }

    #[test]
    fn artifact_merges_canonical_batches_only_at_declared_widths() {
        let first = CombatLearningRootBatchArtifactV1::from_checkpoints([
            combat_root_checkpoint(20),
            combat_root_checkpoint(21),
        ])
        .expect("capture first batch")
        .encode(1024 * 1024)
        .expect("encode first batch");
        let second =
            CombatLearningRootBatchArtifactV1::from_checkpoints([combat_root_checkpoint(22)])
                .expect("capture second batch")
                .encode(1024 * 1024)
                .expect("encode second batch");

        let merged = CombatLearningRootBatchArtifactV1::merge_canonical_payloads(
            [(first.as_slice(), 2), (second.as_slice(), 1)],
            1024 * 1024,
        )
        .expect("merge declared canonical batches");

        assert_eq!(merged.roots().len(), 3);
        assert!(CombatLearningRootBatchArtifactV1::merge_canonical_payloads(
            [(first.as_slice(), 1), (second.as_slice(), 1)],
            1024 * 1024,
        )
        .is_err());
        assert!(CombatLearningRootBatchArtifactV1::merge_canonical_payloads(
            [(first.as_slice(), 2), (first.as_slice(), 2)],
            1024 * 1024,
        )
        .is_err());
    }

    fn combat_root_checkpoint(monster_hp: i32) -> RunControlSessionCheckpointV1 {
        let mut session = RunControlSession::new(Default::default());
        let mut combat = blank_test_combat();
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 51)];
        combat.entities.power_db.insert(11, Vec::new());
        combat.entities.power_db.insert(7, Vec::new());
        combat
            .runtime
            .monster_protocol
            .insert(11, Default::default());
        combat
            .runtime
            .monster_protocol
            .insert(7, Default::default());
        let mut monster = test_monster(EnemyId::JawWorm);
        monster.id = 7;
        monster.current_hp = monster_hp;
        monster.max_hp = monster_hp;
        monster.set_planned_move_id(1);
        let plan = JawWorm::turn_plan(&combat, &monster);
        monster.set_planned_steps(plan.steps);
        monster.set_planned_visible_spec(plan.visible_spec);
        combat.entities.monsters.push(monster);
        session.engine_state = EngineState::CombatPlayerTurn;
        session.active_combat = Some(ActiveCombat::new(
            EngineState::CombatPlayerTurn,
            combat,
            CombatContext::Room(RoomCombatContext {
                room_type: RoomType::MonsterRoom,
            }),
        ));
        RunControlSessionCheckpointV1::from_session(&session)
    }
}
