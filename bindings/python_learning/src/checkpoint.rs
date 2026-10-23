//! Opaque slot/bank checkpoints and their bounded canonical byte format.

use std::collections::BTreeSet;
use std::io::{self, Cursor, Write};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use serde::{Deserialize, Serialize};
use sts_oracle_learning::eval::run_control::{
    LearningCandidateSurfaceV1, LearningEnvCheckpointV3, LearningEnvPoolV3, LearningEnvV3,
};

use super::bridge_decision::{replay_bridge_state, LearningBatchDecisionSource};
use super::{value_error, BridgeSlotState, LearningBatchEnv};

const BATCH_CHECKPOINT_MAGIC: &[u8] = b"STS-LEARNING-BATCH\0";
const BATCH_CHECKPOINT_VERSION: u32 = 4;
const CHECKPOINT_BANK_MAGIC: &[u8] = b"STS-LEARNING-BANK\0";
const CHECKPOINT_BANK_VERSION: u32 = 4;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SerializedLearningBatchCheckpointV4 {
    slots: Vec<SerializedLearningSlotCheckpointV4>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SerializedLearningSlotCheckpointV4 {
    source_slot_index: usize,
    candidate_surface: LearningCandidateSurfaceV1,
    env: LearningEnvCheckpointV3,
    decision_ordinals: Vec<usize>,
}

struct BoundedCheckpointWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
}

impl BoundedCheckpointWriter {
    fn new(magic: &[u8], version: u32, max_bytes: usize) -> Result<Self, String> {
        let header_bytes = magic.len() + std::mem::size_of::<u32>();
        if max_bytes < header_bytes {
            return Err("checkpoint byte limit is smaller than its header".to_owned());
        }
        let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
        bytes.extend_from_slice(magic);
        bytes.extend_from_slice(&version.to_be_bytes());
        Ok(Self { bytes, max_bytes })
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedCheckpointWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next = self
            .bytes
            .len()
            .checked_add(buffer.len())
            .ok_or_else(|| io::Error::other("batch checkpoint byte count overflow"))?;
        if next > self.max_bytes {
            return Err(io::Error::other(
                "batch checkpoint exceeds its caller-provided byte limit",
            ));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Opaque, in-process exact state owned by the caller.
///
/// The bridge creates no automatic history and exposes no serialized session
/// payload. Retaining or discarding checkpoints is a curriculum decision.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub(super) struct LearningSlotCheckpoint {
    pub(super) source_slot_index: usize,
    pub(super) candidate_surface: LearningCandidateSurfaceV1,
    pub(super) env: LearningEnvCheckpointV3,
    pub(super) bridge_state: BridgeSlotState,
}

/// Opaque collection used for one foreign-language call per recovery batch.
#[pyclass(skip_from_py_object)]
pub(super) struct LearningCheckpointBatch {
    pub(super) checkpoints: Vec<LearningSlotCheckpoint>,
}

#[pymethods]
impl LearningCheckpointBatch {
    fn __len__(&self) -> usize {
        self.checkpoints.len()
    }

    fn select(&self, slot_indices: Vec<usize>) -> PyResult<Self> {
        let mut seen = BTreeSet::new();
        let mut selected = Vec::with_capacity(slot_indices.len());
        for slot_index in slot_indices {
            if !seen.insert(slot_index) {
                return Err(PyValueError::new_err(format!(
                    "slot {slot_index} appears more than once in checkpoint selection"
                )));
            }
            let checkpoint = self
                .checkpoints
                .iter()
                .find(|checkpoint| checkpoint.source_slot_index == slot_index)
                .ok_or_else(|| {
                    PyValueError::new_err(format!(
                        "checkpoint batch does not contain slot {slot_index}"
                    ))
                })?;
            selected.push(checkpoint.clone());
        }
        Ok(Self {
            checkpoints: selected,
        })
    }

    fn updated(&self, replacements: PyRef<'_, LearningCheckpointBatch>) -> PyResult<Self> {
        let mut updated = self.checkpoints.clone();
        for replacement in &replacements.checkpoints {
            let checkpoint = updated
                .iter_mut()
                .find(|checkpoint| checkpoint.source_slot_index == replacement.source_slot_index)
                .ok_or_else(|| {
                    PyValueError::new_err(format!(
                        "checkpoint batch does not contain slot {}",
                        replacement.source_slot_index
                    ))
                })?;
            *checkpoint = replacement.clone();
        }
        Ok(Self {
            checkpoints: updated,
        })
    }

    #[pyo3(signature = (*, max_bytes))]
    fn checkpoint_bytes<'py>(
        &self,
        py: Python<'py>,
        max_bytes: usize,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let payload = self
            .encode_cross_process_checkpoint(max_bytes)
            .map_err(value_error)?;
        Ok(PyBytes::new(py, &payload))
    }

    #[staticmethod]
    #[pyo3(signature = (payload, *, expected_slot_indices, max_bytes))]
    fn from_checkpoint_bytes(
        payload: &[u8],
        expected_slot_indices: Vec<usize>,
        max_bytes: usize,
    ) -> PyResult<Self> {
        Self::decode_cross_process_checkpoint(payload, &expected_slot_indices, max_bytes)
            .map_err(value_error)
    }
}

impl LearningCheckpointBatch {
    fn encode_cross_process_checkpoint(&self, max_bytes: usize) -> Result<Vec<u8>, String> {
        let slots = self
            .checkpoints
            .iter()
            .map(|checkpoint| SerializedLearningSlotCheckpointV4 {
                source_slot_index: checkpoint.source_slot_index,
                candidate_surface: checkpoint.candidate_surface,
                env: checkpoint.env.clone(),
                decision_ordinals: checkpoint.bridge_state.decision_ordinals().to_vec(),
            })
            .collect();
        encode_serialized_checkpoint(
            &SerializedLearningBatchCheckpointV4 { slots },
            CHECKPOINT_BANK_MAGIC,
            CHECKPOINT_BANK_VERSION,
            max_bytes,
        )
    }

    fn decode_cross_process_checkpoint(
        payload: &[u8],
        expected_slot_indices: &[usize],
        max_bytes: usize,
    ) -> Result<Self, String> {
        let mut seen = BTreeSet::new();
        for slot_index in expected_slot_indices {
            if !seen.insert(*slot_index) {
                return Err(format!(
                    "slot {slot_index} appears more than once in expected checkpoint bank"
                ));
            }
        }
        let serialized = decode_serialized_checkpoint(
            payload,
            CHECKPOINT_BANK_MAGIC,
            CHECKPOINT_BANK_VERSION,
            max_bytes,
        )?;
        if serialized.slots.len() != expected_slot_indices.len() {
            return Err(format!(
                "checkpoint bank contains {} slots, expected {}",
                serialized.slots.len(),
                expected_slot_indices.len()
            ));
        }
        for (expected_slot, slot) in expected_slot_indices.iter().zip(&serialized.slots) {
            if slot.source_slot_index != *expected_slot {
                return Err(format!(
                    "checkpoint bank contains slot {} where slot {expected_slot} was expected",
                    slot.source_slot_index
                ));
            }
        }

        let checkpoints = serialized
            .slots
            .into_iter()
            .map(|slot| {
                let env = LearningEnvV3::from_checkpoint(slot.env.clone())?;
                let pool = LearningEnvPoolV3::from_envs([env])
                    .map_err(|error| error.to_string())?
                    .with_candidate_surface(slot.candidate_surface);
                let source = LearningBatchDecisionSource::new(&pool);
                let bridge_state = replay_bridge_state(&source, 0, &slot.decision_ordinals)?;
                Ok(LearningSlotCheckpoint {
                    source_slot_index: slot.source_slot_index,
                    candidate_surface: slot.candidate_surface,
                    env: slot.env,
                    bridge_state,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self { checkpoints })
    }
}

impl LearningBatchEnv {
    pub(super) fn encode_cross_process_checkpoint(
        &self,
        max_bytes: usize,
    ) -> Result<Vec<u8>, String> {
        if self.pool.potion_policy().root_slots().is_some() {
            return Err(
                "cross-process batch checkpoints require the all-potions surface".to_owned(),
            );
        }
        let slots = self
            .states
            .iter()
            .enumerate()
            .map(|(source_slot_index, state)| {
                let env = self
                    .pool
                    .checkpoint_slot(source_slot_index)
                    .map_err(|error| error.to_string())?;
                Ok(SerializedLearningSlotCheckpointV4 {
                    source_slot_index,
                    candidate_surface: self.pool.candidate_surface(),
                    env,
                    decision_ordinals: state.decision_ordinals().to_vec(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let checkpoint = SerializedLearningBatchCheckpointV4 { slots };
        encode_serialized_checkpoint(
            &checkpoint,
            BATCH_CHECKPOINT_MAGIC,
            BATCH_CHECKPOINT_VERSION,
            max_bytes,
        )
    }

    pub(super) fn decode_cross_process_checkpoint(
        payload: &[u8],
        expected_slots: usize,
        max_bytes: usize,
    ) -> Result<Self, String> {
        if expected_slots == 0 {
            return Err("batch checkpoint expected slot count must be positive".to_owned());
        }
        let checkpoint = decode_serialized_checkpoint(
            payload,
            BATCH_CHECKPOINT_MAGIC,
            BATCH_CHECKPOINT_VERSION,
            max_bytes,
        )?;
        if checkpoint.slots.len() != expected_slots {
            return Err(format!(
                "batch checkpoint contains {} slots, expected {expected_slots}",
                checkpoint.slots.len()
            ));
        }
        for (expected_slot, slot) in checkpoint.slots.iter().enumerate() {
            if slot.source_slot_index != expected_slot {
                return Err(format!(
                    "batch checkpoint slot {} is stored at position {expected_slot}",
                    slot.source_slot_index
                ));
            }
        }

        let envs = checkpoint
            .slots
            .iter()
            .map(|slot| LearningEnvV3::from_checkpoint(slot.env.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        let surface = checkpoint.slots[0].candidate_surface;
        if checkpoint
            .slots
            .iter()
            .any(|slot| slot.candidate_surface != surface)
        {
            return Err("batch checkpoint mixes candidate surfaces".into());
        }
        let pool = LearningEnvPoolV3::from_envs(envs)
            .map_err(|error| error.to_string())?
            .with_candidate_surface(surface);
        let source = LearningBatchDecisionSource::new(&pool);
        let states = checkpoint
            .slots
            .iter()
            .map(|slot| {
                replay_bridge_state(&source, slot.source_slot_index, &slot.decision_ordinals)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { pool, states })
    }
}

fn encode_serialized_checkpoint(
    checkpoint: &SerializedLearningBatchCheckpointV4,
    magic: &[u8],
    version: u32,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut writer = BoundedCheckpointWriter::new(magic, version, max_bytes)?;
    rmp_serde::encode::write_named(&mut writer, checkpoint)
        .map_err(|error| format!("cannot encode checkpoint: {error}"))?;
    Ok(writer.finish())
}

fn decode_serialized_checkpoint(
    payload: &[u8],
    magic: &[u8],
    version: u32,
    max_bytes: usize,
) -> Result<SerializedLearningBatchCheckpointV4, String> {
    if payload.len() > max_bytes {
        return Err("checkpoint exceeds its caller-provided byte limit".to_owned());
    }
    let header_bytes = magic.len() + std::mem::size_of::<u32>();
    if payload.len() < header_bytes {
        return Err("checkpoint ended before its header".to_owned());
    }
    if &payload[..magic.len()] != magic {
        return Err("checkpoint magic is invalid".to_owned());
    }
    let version_start = magic.len();
    let encoded_version = u32::from_be_bytes(
        payload[version_start..header_bytes]
            .try_into()
            .map_err(|_| "checkpoint version is truncated")?,
    );
    if encoded_version != version {
        return Err("checkpoint format version is unsupported".to_owned());
    }
    let mut decoder = rmp_serde::Deserializer::new(Cursor::new(&payload[header_bytes..]));
    let checkpoint = SerializedLearningBatchCheckpointV4::deserialize(&mut decoder)
        .map_err(|error| format!("cannot decode checkpoint: {error}"))?;
    if usize::try_from(decoder.position()).ok() != Some(payload.len() - header_bytes) {
        return Err("checkpoint contains trailing bytes".to_owned());
    }
    if encode_serialized_checkpoint(&checkpoint, magic, version, max_bytes)? != payload {
        return Err("checkpoint encoding is not canonical".to_owned());
    }
    Ok(checkpoint)
}
