use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;
use sts_java_parity_live::{ActionReceipt, LiveStatus};
use sts_java_parity_runtime::JavaParityRuntimeOwnerV1;
use sts_parity_lockstep::{AdapterFailureKindV1, AdapterFailureV1, ProducerIdentityV1};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaRuntimeActionV1 {
    pub runtime_handle: String,
    pub action_id: String,
    pub kind: String,
    pub event_id: Option<String>,
    pub option_index: Option<usize>,
    pub neow_reward_type: Option<String>,
    pub neow_drawback: Option<String>,
    pub map_x: Option<i64>,
    pub map_y: Option<i64>,
    pub map_room_type: Option<String>,
    pub reward_type: Option<String>,
    pub reward_index: Option<usize>,
    pub reward_id: Option<String>,
    pub reward_gold: Option<i64>,
    pub potion_slot: Option<usize>,
    pub potion_id: Option<String>,
    pub parameters_required: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaRuntimeObservationV1 {
    pub producer: ProducerIdentityV1,
    pub frame_id: u64,
    pub frame: Value,
    pub actions: Vec<JavaRuntimeActionV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaLockstepReceiptV1 {
    pub status: String,
    pub request_id: String,
    pub based_on_frame_id: u64,
    pub action_id: String,
    pub successor_frame_id: Option<u64>,
}

/// The actual main-menu Continue execution, outside the adopted player
/// trajectory. Its receipt authorizes only the adjacent reconstructed boundary.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct JavaNativeReloadExecutionV1 {
    pub before: JavaRuntimeObservationV1,
    pub receipt: JavaLockstepReceiptV1,
    pub after: JavaRuntimeObservationV1,
}

impl JavaNativeReloadExecutionV1 {
    pub fn validate(&self) -> Result<(), String> {
        if !native_continue_ready(&self.before)?
            || self.before.producer != self.after.producer
            || self.receipt.status != "completed"
            || self.receipt.request_id.is_empty()
            || self.receipt.action_id != "continue_run"
            || self.receipt.based_on_frame_id != self.before.frame_id
            || self.receipt.successor_frame_id != Some(self.after.frame_id)
            || self.after.frame_id <= self.before.frame_id
            || self.before.frame["frame_id"].as_u64() != Some(self.before.frame_id)
            || self.after.frame["frame_id"].as_u64() != Some(self.after.frame_id)
        {
            return Err(
                "native Continue receipt does not link its original menu and successor".to_owned(),
            );
        }
        Ok(())
    }
}

/// Replaceable boundary around a resident Java control session.
///
/// Tests use an in-memory implementation. The production implementation only
/// attaches to an existing live session; it never starts or owns a JVM.
pub trait JavaLockstepTransportV1 {
    fn status(&mut self) -> Result<JavaRuntimeObservationV1, AdapterFailureV1>;

    fn act(&mut self, runtime_handle: &str) -> Result<JavaLockstepReceiptV1, AdapterFailureV1>;

    fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1>;

    fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1>;
}

/// Detached path transport retained for focused diagnostics and fixtures.
/// Production lockstep composition uses [`OwnedJavaLockstepTransportV1`].
pub struct LiveJavaLockstepTransportV1 {
    output_directory: PathBuf,
    action_timeout: Duration,
}

/// Production transport that cannot outlive or bypass its Java process owner.
pub struct OwnedJavaLockstepTransportV1 {
    runtime: JavaParityRuntimeOwnerV1,
    action_timeout: Duration,
}

/// Detached worker transport admitted by one exclusive action lease and bound
/// to the exact producer epoch published by the Java oracle host.
pub struct LeasedJavaLockstepTransportV1 {
    inner: LiveJavaLockstepTransportV1,
    expected_producer: ProducerIdentityV1,
    _lease: JavaOracleWorkerLeaseV1,
}

#[derive(Debug)]
struct JavaOracleWorkerLeaseV1 {
    _file: File,
}

impl LeasedJavaLockstepTransportV1 {
    pub fn connect(
        output_directory: impl Into<PathBuf>,
        lease_path: &Path,
        expected_producer: ProducerIdentityV1,
        action_timeout: Duration,
    ) -> Result<Self, AdapterFailureV1> {
        let lease = JavaOracleWorkerLeaseV1::acquire(lease_path)?;
        let mut transport = Self {
            inner: LiveJavaLockstepTransportV1::new(output_directory, action_timeout),
            expected_producer,
            _lease: lease,
        };
        transport.status()?;
        Ok(transport)
    }

    /// Returns the repository-managed isolated storage bound to this leased
    /// oracle. Manual research checkpoints may copy native saves only through
    /// this validated path; installed player storage is never eligible.
    pub fn isolated_storage_directory(&mut self) -> Result<PathBuf, AdapterFailureV1> {
        let live = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&live)?;
        let launch = live.manifest.launch.as_ref().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "Java oracle omitted its launch storage evidence",
            )
        })?;
        if launch.storage_class != "isolated_working_directory_v1" || launch.isolation_id.is_none()
        {
            return Err(failure(
                AdapterFailureKindV1::Rejected,
                "research checkpoints require repository-managed isolated Java storage",
            ));
        }
        let directory = PathBuf::from(&launch.working_directory);
        let directory = directory.canonicalize().map_err(|error| {
            failure(
                AdapterFailureKindV1::Evidence,
                format!("failed to resolve isolated Java storage: {error}"),
            )
        })?;
        let source = PathBuf::from(&launch.source_game_directory)
            .canonicalize()
            .map_err(|error| {
                failure(
                    AdapterFailureKindV1::Evidence,
                    format!("failed to resolve installed game storage: {error}"),
                )
            })?;
        if directory == source {
            return Err(failure(
                AdapterFailureKindV1::Rejected,
                "research checkpoint storage aliases the installed game",
            ));
        }
        Ok(directory)
    }

    pub fn start_run_with_content_profile(
        &mut self,
        character: &str,
        ascension: u32,
        seed: Option<&str>,
        content_profile: &str,
        timeout: Duration,
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        self.status()?;
        let receipt = sts_java_parity_live::start_run_with_content_profile(
            &self.inner.output_directory,
            character,
            ascension,
            seed,
            content_profile,
            timeout,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.status()?;
        Ok(lockstep_receipt(receipt))
    }

    pub fn reset_natural_run_to_main_menu_preserving_save(
        &mut self,
        report_path: &Path,
        timeout: Duration,
    ) -> Result<(), AdapterFailureV1> {
        let before = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&before)?;
        sts_java_parity_live::reset_natural_run_to_main_menu_preserving_save(
            &self.inner.output_directory,
            report_path,
            timeout,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        let after = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&after)
    }

    pub fn continue_run_with_content_profile(
        &mut self,
        content_profile: &str,
        timeout: Duration,
    ) -> Result<JavaNativeReloadExecutionV1, AdapterFailureV1> {
        // Installing a native generation can turn a resident Start menu into
        // Continue. Wait for the producer's refreshed stable frame; do not send
        // a synthetic action against the old menu or restart the JVM.
        let deadline = std::time::Instant::now() + timeout;
        let before = loop {
            let before = self.status()?;
            if native_continue_ready(&before)
                .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?
            {
                break before;
            }
            if std::time::Instant::now() >= deadline {
                return Err(failure(
                    AdapterFailureKindV1::Transport,
                    "native save installation did not publish a Continue menu before timeout",
                ));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        let receipt = sts_java_parity_live::continue_run_with_content_profile(
            &self.inner.output_directory,
            content_profile,
            timeout,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        let after = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&after)?;
        let execution = JavaNativeReloadExecutionV1 {
            before,
            receipt: lockstep_receipt(receipt),
            after: observation_from_live(after)?,
        };
        execution
            .validate()
            .map_err(|message| failure(AdapterFailureKindV1::Evidence, message))?;
        Ok(execution)
    }

    /// End one hosted case at any stable natural boundary without preserving
    /// its native save.
    ///
    /// The producer only publishes this action for repository-managed isolated
    /// storage. Keeping it on the leased transport ensures an unleased worker
    /// cannot mutate the resident oracle between cases.
    pub fn discard_run_for_next_case(&mut self) -> Result<(), AdapterFailureV1> {
        let before = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&before)?;
        validate_discard_boundary(&observation_from_live(before.clone())?)
            .map_err(|message| failure(AdapterFailureKindV1::Rejected, message))?;
        let from_frame_id = before.frame["frame_id"].as_u64().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "hosted case discard predecessor omitted its frame ID",
            )
        })?;
        let mut actions = before.actions.iter().filter(|action| {
            action.kind == "run_discard_to_main_menu"
                && action.action_id == "run_discard_to_main_menu"
        });
        let action = actions.next().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Rejected,
                "natural boundary omitted its managed-run discard action",
            )
        })?;
        if actions.next().is_some() || action.parameters_required {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "managed-run discard action is not uniquely and exactly typed",
            ));
        }

        let receipt = sts_java_parity_live::act_on_observation(
            &before,
            &action.handle,
            self.inner.action_timeout,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        let successor_frame_id = receipt.successor_frame_id.ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Evidence,
                "managed-run discard receipt omitted its stable successor",
            )
        })?;
        if receipt.status != "completed"
            || receipt.code.is_some()
            || receipt.based_on_frame_id != from_frame_id
            || receipt.action_id != "run_discard_to_main_menu"
            || receipt.parameters != serde_json::json!({})
        {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "managed-run discard did not produce an exact completed receipt",
            ));
        }

        let after = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&after)?;
        let reusable = after.frame["frame_id"].as_u64() == Some(successor_frame_id)
            && after.frame["boundary"]["kind"].as_str() == Some("main_menu_ready")
            && after.frame["public_base"]["decision"]["kind"].as_str() == Some("main_menu_ready")
            && after.frame["public_base"]["run"].is_null()
            && after.frame["scenario"].is_null()
            && after.frame["parity_internal"]["run_seed"].is_null()
            && after.frame["rng_internal"]["streams"]
                .as_array()
                .is_some_and(Vec::is_empty)
            && after.actions.len() == 1
            && after.actions[0].kind == "start_run"
            && after.actions[0].action_id == "start_run"
            && after.actions[0].parameters_required;
        if !reusable {
            return Err(failure(
                AdapterFailureKindV1::Evidence,
                "managed-run discard successor is not a fresh Start epoch",
            ));
        }
        Ok(())
    }

    fn validate_live_producer(&self, live: &LiveStatus) -> Result<(), AdapterFailureV1> {
        if producer_identity_from_live_v1(live) != self.expected_producer {
            return Err(failure(
                AdapterFailureKindV1::Transport,
                "Java oracle producer epoch differs from the published endpoint",
            ));
        }
        Ok(())
    }
}

fn native_continue_ready(observation: &JavaRuntimeObservationV1) -> Result<bool, String> {
    if observation.frame["boundary"]["kind"].as_str() != Some("main_menu_ready")
        || observation.frame["public_base"]["decision"]["kind"].as_str() != Some("main_menu_ready")
        || !observation.frame["public_base"]["run"].is_null()
        || !observation.frame["scenario"].is_null()
        || observation.actions.len() != 1
    {
        return Err("native Continue requires a stable natural main menu".to_owned());
    }
    let action = &observation.actions[0];
    if action.kind != action.action_id || !action.parameters_required {
        return Err("native menu action is not exactly typed".to_owned());
    }
    match action.kind.as_str() {
        "start_run" => Ok(false),
        "continue_run" => Ok(true),
        _ => Err("native menu omitted Start or Continue".to_owned()),
    }
}

fn validate_discard_boundary(observation: &JavaRuntimeObservationV1) -> Result<(), String> {
    if !observation.frame["scenario"].is_null()
        || !observation.frame["public_base"]["run"].is_object()
    {
        return Err("hosted case discard requires a natural run".to_owned());
    }
    match observation.frame["boundary"]["kind"].as_str() {
        Some("run_decision" | "combat_player_decision") => Ok(()),
        Some("terminal") => {
            if crate::projection::boundary_kind(observation)? != "natural_heart_victory" {
                return Err("hosted case discard requires an observed Heart victory".to_owned());
            }
            crate::action::resolved_actions_v1(observation, None, None)?;
            Ok(())
        }
        _ => Err("hosted case discard requires a stable natural boundary".to_owned()),
    }
}

impl JavaOracleWorkerLeaseV1 {
    fn acquire(path: &Path) -> Result<Self, AdapterFailureV1> {
        let parent = path.parent().ok_or_else(|| {
            failure(
                AdapterFailureKindV1::Transport,
                "Java oracle worker lease has no parent",
            )
        })?;
        if !parent.is_dir() {
            return Err(failure(
                AdapterFailureKindV1::Transport,
                "Java oracle worker lease parent does not exist",
            ));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| {
                failure(
                    AdapterFailureKindV1::Transport,
                    format!("failed to open Java oracle worker lease: {error}"),
                )
            })?;
        file.try_lock_exclusive().map_err(|_| {
            failure(
                AdapterFailureKindV1::Rejected,
                "another parity worker already owns the Java oracle action lease",
            )
        })?;
        Ok(Self { _file: file })
    }
}

impl OwnedJavaLockstepTransportV1 {
    pub fn new(runtime: JavaParityRuntimeOwnerV1, action_timeout: Duration) -> Self {
        Self {
            runtime,
            action_timeout,
        }
    }

    pub fn runtime(&self) -> &JavaParityRuntimeOwnerV1 {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut JavaParityRuntimeOwnerV1 {
        &mut self.runtime
    }

    pub fn into_runtime(self) -> JavaParityRuntimeOwnerV1 {
        self.runtime
    }
}

impl LiveJavaLockstepTransportV1 {
    pub fn new(output_directory: impl Into<PathBuf>, action_timeout: Duration) -> Self {
        Self {
            output_directory: output_directory.into(),
            action_timeout,
        }
    }
}

impl JavaLockstepTransportV1 for LiveJavaLockstepTransportV1 {
    fn status(&mut self) -> Result<JavaRuntimeObservationV1, AdapterFailureV1> {
        let live = sts_java_parity_live::status(&self.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        observation_from_live(live)
    }

    fn act(&mut self, runtime_handle: &str) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let receipt =
            sts_java_parity_live::act(&self.output_directory, runtime_handle, self.action_timeout)
                .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        Ok(lockstep_receipt(receipt))
    }

    fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let live = sts_java_parity_live::status(&self.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        let receipt = sts_java_parity_live::submit_grid_select_on_observation(
            &live,
            runtime_handle,
            selected_card_uuids,
            self.action_timeout,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        Ok(lockstep_receipt(receipt))
    }

    fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let live = sts_java_parity_live::status(&self.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        let receipt = sts_java_parity_live::submit_hand_select_on_observation(
            &live,
            runtime_handle,
            selected_card_uuids,
            self.action_timeout,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        Ok(lockstep_receipt(receipt))
    }
}

impl JavaLockstepTransportV1 for OwnedJavaLockstepTransportV1 {
    fn status(&mut self) -> Result<JavaRuntimeObservationV1, AdapterFailureV1> {
        let live = self
            .runtime
            .status()
            .map_err(|error| failure(AdapterFailureKindV1::Transport, error.to_string()))?
            .into_live();
        observation_from_live(live)
    }

    fn act(&mut self, runtime_handle: &str) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let receipt = self
            .runtime
            .act(runtime_handle, self.action_timeout)
            .map_err(|error| failure(AdapterFailureKindV1::Transport, error.to_string()))?;
        Ok(lockstep_receipt(receipt))
    }

    fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let receipt = self
            .runtime
            .submit_grid_select(runtime_handle, selected_card_uuids, self.action_timeout)
            .map_err(|error| failure(AdapterFailureKindV1::Transport, error.to_string()))?;
        Ok(lockstep_receipt(receipt))
    }

    fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let receipt = self
            .runtime
            .submit_hand_select(runtime_handle, selected_card_uuids, self.action_timeout)
            .map_err(|error| failure(AdapterFailureKindV1::Transport, error.to_string()))?;
        Ok(lockstep_receipt(receipt))
    }
}

impl JavaLockstepTransportV1 for LeasedJavaLockstepTransportV1 {
    fn status(&mut self) -> Result<JavaRuntimeObservationV1, AdapterFailureV1> {
        let observation = self.inner.status()?;
        if observation.producer != self.expected_producer {
            return Err(failure(
                AdapterFailureKindV1::Transport,
                "Java oracle producer epoch differs from the published endpoint",
            ));
        }
        Ok(observation)
    }

    fn act(&mut self, runtime_handle: &str) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        self.status()?;
        self.inner.act(runtime_handle)
    }

    fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let before = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&before)?;
        let receipt = sts_java_parity_live::submit_grid_select_on_observation(
            &before,
            runtime_handle,
            selected_card_uuids,
            self.inner.action_timeout,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        let after = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&after)?;
        Ok(lockstep_receipt(receipt))
    }

    fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
    ) -> Result<JavaLockstepReceiptV1, AdapterFailureV1> {
        let before = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&before)?;
        let receipt = sts_java_parity_live::submit_hand_select_on_observation(
            &before,
            runtime_handle,
            selected_card_uuids,
            self.inner.action_timeout,
        )
        .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        let after = sts_java_parity_live::status(&self.inner.output_directory)
            .map_err(|message| failure(AdapterFailureKindV1::Transport, message))?;
        self.validate_live_producer(&after)?;
        Ok(lockstep_receipt(receipt))
    }
}

fn observation_from_live(live: LiveStatus) -> Result<JavaRuntimeObservationV1, AdapterFailureV1> {
    let frame_id = live.frame["frame_id"].as_u64().ok_or_else(|| {
        failure(
            AdapterFailureKindV1::Evidence,
            "Java live observation omitted its frame ID",
        )
    })?;
    let producer = producer_identity_from_live_v1(&live);
    let actions = live
        .actions
        .into_iter()
        .map(|action| JavaRuntimeActionV1 {
            runtime_handle: action.handle,
            action_id: action.action_id,
            kind: action.kind,
            event_id: action.event_id,
            option_index: action.option_index,
            neow_reward_type: action.neow_reward_type,
            neow_drawback: action.neow_drawback,
            map_x: action.map_x,
            map_y: action.map_y,
            map_room_type: action.map_room_type,
            reward_type: action.reward_type,
            reward_index: action.reward_index,
            reward_id: action.reward_id,
            reward_gold: action.reward_gold,
            potion_slot: action.potion_slot,
            potion_id: action.potion_id,
            parameters_required: action.parameters_required,
        })
        .collect();
    Ok(JavaRuntimeObservationV1 {
        producer,
        frame_id,
        frame: live.frame,
        actions,
    })
}

pub fn producer_identity_from_live_v1(live: &LiveStatus) -> ProducerIdentityV1 {
    ProducerIdentityV1 {
        implementation: "communicationmod".to_owned(),
        version: live.manifest.loaded_mod_jar_sha256.clone(),
        session_epoch: format!(
            "{}:{}:{}",
            live.manifest.producer_session_id,
            live.manifest.connection_id,
            live.manifest.capture_id
        ),
    }
}

fn lockstep_receipt(receipt: ActionReceipt) -> JavaLockstepReceiptV1 {
    sts_parity_lockstep::record_timing_observation_v1(
        "bridge.send_to_settled_receipt",
        receipt
            .settlement_wall_time_ms
            .and_then(|ms| ms.checked_mul(1_000_000)),
    );
    JavaLockstepReceiptV1 {
        status: receipt.status,
        request_id: receipt.request_id,
        based_on_frame_id: receipt.based_on_frame_id,
        action_id: receipt.action_id,
        successor_frame_id: receipt.successor_frame_id,
    }
}

fn failure(kind: AdapterFailureKindV1, message: impl Into<String>) -> AdapterFailureV1 {
    AdapterFailureV1::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn native_continue_waits_for_the_refreshed_menu_and_rejects_other_boundaries() {
        let mut observation: JavaRuntimeObservationV1 = serde_json::from_value(json!({
            "producer": {"implementation": "fixture", "version": "test", "session_epoch": "test"},
            "frame_id": 1,
            "frame": {"boundary": {"kind": "main_menu_ready"},
                "public_base": {"decision": {"kind": "main_menu_ready"}, "run": null}},
            "actions": [{"runtime_handle": "f1:a0", "action_id": "start_run",
                "kind": "start_run", "parameters_required": true}]
        }))
        .unwrap();
        assert!(!native_continue_ready(&observation).unwrap());
        observation.actions[0].kind = "continue_run".into();
        observation.actions[0].action_id = "continue_run".into();
        assert!(native_continue_ready(&observation).unwrap());
        for bad in [
            "boundary",
            "run",
            "scenario",
            "duplicate",
            "parameters",
            "alias",
        ] {
            let mut invalid = observation.clone();
            match bad {
                "boundary" => invalid.frame["boundary"]["kind"] = json!("run_decision"),
                "run" => invalid.frame["public_base"]["run"] = json!({"act": 2}),
                "scenario" => invalid.frame["scenario"] = json!({"kind": "constructed"}),
                "duplicate" => invalid.actions.push(invalid.actions[0].clone()),
                "parameters" => invalid.actions[0].parameters_required = false,
                "alias" => invalid.actions[0].action_id = "start_run".into(),
                _ => unreachable!(),
            }
            assert!(native_continue_ready(&invalid).is_err(), "{bad}");
        }
    }

    #[test]
    fn terminal_discard_requires_a_natural_living_heart_endpoint() {
        let observation: JavaRuntimeObservationV1 = serde_json::from_value(json!({
            "producer": {"implementation": "fixture", "version": "test", "session_epoch": "test"},
            "frame_id": 1,
            "frame": {
                "boundary": {"kind": "terminal"},
                "public_base": {
                    "decision": {"kind": "heart_victory"},
                    "run": {"act": 4, "current_hp": 10},
                    "map": {"current_x": 3, "current_y": 4, "nodes": [
                        {"x": 3, "y": 4, "room_type": "TrueVictoryRoom", "edges": []}
                    ]}
                }
            },
            "actions": [{"runtime_handle": "f1:a0", "action_id": "run_discard_to_main_menu",
                "kind": "run_discard_to_main_menu", "parameters_required": false}]
        }))
        .unwrap();
        validate_discard_boundary(&observation).unwrap();
        for bad in [
            "scenario", "dead", "room", "unknown", "decision", "gameplay",
        ] {
            let mut invalid = observation.clone();
            match bad {
                "scenario" => invalid.frame["scenario"] = json!({"kind": "constructed"}),
                "dead" => invalid.frame["public_base"]["run"]["current_hp"] = json!(0),
                "room" => {
                    invalid.frame["public_base"]["map"]["nodes"][0]["room_type"] =
                        json!("MonsterRoomBoss")
                }
                "unknown" => invalid.frame["boundary"]["kind"] = json!("unknown"),
                "decision" => invalid.frame["public_base"]["decision"]["kind"] = json!("death"),
                "gameplay" => invalid.actions[0].kind = "play_card".into(),
                _ => unreachable!(),
            }
            assert!(validate_discard_boundary(&invalid).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_one_worker_can_hold_an_oracle_action_lease() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "sts-java-oracle-worker-lease-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("worker.lock");
        let first = JavaOracleWorkerLeaseV1::acquire(&path).unwrap();

        let error = JavaOracleWorkerLeaseV1::acquire(&path).unwrap_err();
        assert_eq!(error.kind, AdapterFailureKindV1::Rejected);
        drop(first);
        JavaOracleWorkerLeaseV1::acquire(&path).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }
}
