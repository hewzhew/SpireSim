//! Launch and unique lifecycle ownership for one isolated Java parity session.
//!
//! This crate prepares the game process, admitted bridge process, and isolated
//! storage, then binds them to one live protocol epoch. It does not interpret
//! game scenes, paired roots, trajectories, or strategy.

mod launch;

pub use launch::{
    launch_java_parity_runtime_v1, JavaParityInitialBoundaryV1, JavaParityLaunchProfileV1,
    JavaParityLaunchRequestV1, JavaParityStorageSeedV1, PreparedJavaParityRuntimeV1,
};

use std::{fmt, fs, path::Path, path::PathBuf, time::Duration};
use sts_java_parity_live::{ActionReceipt, LiveStatus};
use sts_workbench_supervisor::{
    JavaRuntimeIdentityV1, NativeRecoveryCapsuleErrorV1, NativeRecoveryCaptureScopeV1,
    RuntimeSessionBindingV1, SupervisedRuntimeErrorV1, SupervisedRuntimeV1,
};

const PRIMARY_TREE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);
const PRIMARY_FORCED_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const COMPANION_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum JavaParityRuntimeErrorV1 {
    Closed,
    LiveAction(String),
    LiveStatus(String),
    MissingLaunchEvidence,
    LaunchPlanPath(String),
    LaunchPlanMismatch,
    MissingProtocolMinor,
    NativeRecovery(NativeRecoveryCapsuleErrorV1),
    ReceiptIdentityMismatch,
    Supervised(SupervisedRuntimeErrorV1),
}

impl fmt::Display for JavaParityRuntimeErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => write!(formatter, "Java parity runtime is closed"),
            Self::LiveAction(detail) => write!(formatter, "Java live action failed: {detail}"),
            Self::LiveStatus(detail) => write!(formatter, "Java live status failed: {detail}"),
            Self::MissingLaunchEvidence => {
                write!(formatter, "Java live session omitted launch evidence")
            }
            Self::LaunchPlanPath(detail) => {
                write!(formatter, "Java launch-plan identity failed: {detail}")
            }
            Self::LaunchPlanMismatch => {
                write!(formatter, "Java live session changed its launch plan")
            }
            Self::MissingProtocolMinor => {
                write!(formatter, "Java live frame omitted its protocol minor")
            }
            Self::NativeRecovery(error) => {
                write!(formatter, "Java native-recovery scope failed: {error}")
            }
            Self::ReceiptIdentityMismatch => {
                write!(
                    formatter,
                    "Java action receipt belongs to another live session"
                )
            }
            Self::Supervised(error) => write!(formatter, "Java runtime ownership failed: {error}"),
        }
    }
}

impl std::error::Error for JavaParityRuntimeErrorV1 {}

impl From<SupervisedRuntimeErrorV1> for JavaParityRuntimeErrorV1 {
    fn from(error: SupervisedRuntimeErrorV1) -> Self {
        Self::Supervised(error)
    }
}

/// A live observation accepted only after the process and protocol identities
/// have both been revalidated against the unique runtime owner.
#[derive(Debug)]
pub struct ValidatedJavaStatusV1 {
    live: LiveStatus,
    runtime_identity: JavaRuntimeIdentityV1,
}

impl ValidatedJavaStatusV1 {
    pub fn live(&self) -> &LiveStatus {
        &self.live
    }

    pub fn into_live(self) -> LiveStatus {
        self.live
    }

    pub fn runtime_identity(&self) -> &JavaRuntimeIdentityV1 {
        &self.runtime_identity
    }
}

/// Sole owner of the external resources for one admitted Java parity epoch.
///
/// The paths are evidence inputs, not process-discovery hints: every status
/// read is checked against the exact process/session binding already held by
/// `SupervisedRuntimeV1`.
#[derive(Debug)]
pub struct JavaParityRuntimeOwnerV1 {
    supervised: Option<SupervisedRuntimeV1>,
    launch_plan: PathBuf,
    live_output: PathBuf,
}

impl JavaParityRuntimeOwnerV1 {
    pub fn new(
        supervised: SupervisedRuntimeV1,
        launch_plan: impl Into<PathBuf>,
        live_output: impl Into<PathBuf>,
    ) -> Result<Self, JavaParityRuntimeErrorV1> {
        let mut owner = Self {
            supervised: Some(supervised),
            launch_plan: launch_plan.into(),
            live_output: live_output.into(),
        };
        owner.status()?;
        Ok(owner)
    }

    pub fn launch_plan(&self) -> &Path {
        &self.launch_plan
    }

    pub fn live_output(&self) -> &Path {
        &self.live_output
    }

    pub fn is_closed(&self) -> bool {
        self.supervised.is_none()
    }

    pub fn runtime_identity(&self) -> Result<JavaRuntimeIdentityV1, JavaParityRuntimeErrorV1> {
        Ok(self.supervised()?.java_runtime_identity())
    }

    pub fn primary_process_id(&self) -> Result<u32, JavaParityRuntimeErrorV1> {
        Ok(self.supervised()?.primary_process_id())
    }

    pub fn runtime_epoch(&self) -> Result<&str, JavaParityRuntimeErrorV1> {
        Ok(self.supervised()?.session().runtime_epoch())
    }

    pub fn client_session_id(&self) -> Result<&str, JavaParityRuntimeErrorV1> {
        Ok(self.supervised()?.session().client_session_id())
    }

    pub fn connection_id(&self) -> Result<&str, JavaParityRuntimeErrorV1> {
        Ok(self.supervised()?.session().connection_id())
    }

    pub fn frame_token(&self) -> Result<&str, JavaParityRuntimeErrorV1> {
        self.supervised()?.frame_token().map_err(Into::into)
    }

    pub fn autosave_identity(&self) -> Result<&str, JavaParityRuntimeErrorV1> {
        self.supervised()?.autosave_identity().map_err(Into::into)
    }

    pub fn bind_native_recovery_scope(
        &self,
        relative_directories: &[&str],
    ) -> Result<NativeRecoveryCaptureScopeV1, JavaParityRuntimeErrorV1> {
        self.supervised()?
            .bind_native_recovery_scope(relative_directories)
            .map_err(JavaParityRuntimeErrorV1::NativeRecovery)
    }

    pub fn native_autosave_is_current(&self) -> Result<bool, JavaParityRuntimeErrorV1> {
        self.supervised()?
            .native_autosave_is_current()
            .map_err(Into::into)
    }

    pub fn current_autosave_identity_optional(
        &self,
    ) -> Result<Option<String>, JavaParityRuntimeErrorV1> {
        self.supervised()?
            .current_autosave_identity_optional()
            .map_err(Into::into)
    }

    pub fn verify_autosave(&self) -> Result<(), JavaParityRuntimeErrorV1> {
        self.supervised()?.verify_autosave().map_err(Into::into)
    }

    pub fn verify_frame(&self, observed: &str) -> Result<(), JavaParityRuntimeErrorV1> {
        self.supervised()?
            .verify_frame(observed)
            .map_err(Into::into)
    }

    pub fn accept_current_autosave(&mut self) -> Result<&str, JavaParityRuntimeErrorV1> {
        self.supervised_mut()?
            .accept_current_autosave()
            .map_err(Into::into)
    }

    pub fn accept_frame(
        &mut self,
        observed: impl Into<String>,
    ) -> Result<(), JavaParityRuntimeErrorV1> {
        self.supervised_mut()?
            .accept_frame(observed)
            .map_err(Into::into)
    }

    pub fn status(&mut self) -> Result<ValidatedJavaStatusV1, JavaParityRuntimeErrorV1> {
        let supervised = self
            .supervised
            .as_mut()
            .ok_or(JavaParityRuntimeErrorV1::Closed)?;
        let live = sts_java_parity_live::status(&self.live_output)
            .map_err(JavaParityRuntimeErrorV1::LiveStatus)?;
        let launch = live
            .manifest
            .launch
            .as_ref()
            .ok_or(JavaParityRuntimeErrorV1::MissingLaunchEvidence)?;
        if !same_existing_path(Path::new(&launch.plan_path), &self.launch_plan)? {
            return Err(JavaParityRuntimeErrorV1::LaunchPlanMismatch);
        }
        supervised.validate_running(&runtime_session_binding(&live)?)?;
        Ok(ValidatedJavaStatusV1 {
            runtime_identity: supervised.java_runtime_identity(),
            live,
        })
    }

    pub fn act(
        &mut self,
        runtime_handle: &str,
        timeout: Duration,
    ) -> Result<ActionReceipt, JavaParityRuntimeErrorV1> {
        let before = self.status()?.into_live();
        let receipt = sts_java_parity_live::act_on_observation(&before, runtime_handle, timeout)
            .map_err(JavaParityRuntimeErrorV1::LiveAction)?;
        if receipt.session_id != before.manifest.session_id
            || receipt.connection_id != before.manifest.connection_id
        {
            return Err(JavaParityRuntimeErrorV1::ReceiptIdentityMismatch);
        }
        self.status()?;
        Ok(receipt)
    }

    pub fn submit_grid_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
        timeout: Duration,
    ) -> Result<ActionReceipt, JavaParityRuntimeErrorV1> {
        let before = self.status()?.into_live();
        let receipt = sts_java_parity_live::submit_grid_select_on_observation(
            &before,
            runtime_handle,
            selected_card_uuids,
            timeout,
        )
        .map_err(JavaParityRuntimeErrorV1::LiveAction)?;
        if receipt.session_id != before.manifest.session_id
            || receipt.connection_id != before.manifest.connection_id
        {
            return Err(JavaParityRuntimeErrorV1::ReceiptIdentityMismatch);
        }
        self.status()?;
        Ok(receipt)
    }

    pub fn submit_hand_select(
        &mut self,
        runtime_handle: &str,
        selected_card_uuids: &[String],
        timeout: Duration,
    ) -> Result<ActionReceipt, JavaParityRuntimeErrorV1> {
        let before = self.status()?.into_live();
        let receipt = sts_java_parity_live::submit_hand_select_on_observation(
            &before,
            runtime_handle,
            selected_card_uuids,
            timeout,
        )
        .map_err(JavaParityRuntimeErrorV1::LiveAction)?;
        if receipt.session_id != before.manifest.session_id
            || receipt.connection_id != before.manifest.connection_id
        {
            return Err(JavaParityRuntimeErrorV1::ReceiptIdentityMismatch);
        }
        self.status()?;
        Ok(receipt)
    }

    pub fn shutdown(&mut self) -> Result<(), JavaParityRuntimeErrorV1> {
        let Some(supervised) = self.supervised.as_mut() else {
            return Ok(());
        };
        supervised.shutdown(
            PRIMARY_TREE_SHUTDOWN_TIMEOUT,
            PRIMARY_FORCED_SHUTDOWN_TIMEOUT,
            COMPANION_SHUTDOWN_TIMEOUT,
        )?;
        self.supervised = None;
        Ok(())
    }

    fn supervised(&self) -> Result<&SupervisedRuntimeV1, JavaParityRuntimeErrorV1> {
        self.supervised
            .as_ref()
            .ok_or(JavaParityRuntimeErrorV1::Closed)
    }

    fn supervised_mut(&mut self) -> Result<&mut SupervisedRuntimeV1, JavaParityRuntimeErrorV1> {
        self.supervised
            .as_mut()
            .ok_or(JavaParityRuntimeErrorV1::Closed)
    }
}

impl Drop for JavaParityRuntimeOwnerV1 {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn runtime_session_binding(
    live: &LiveStatus,
) -> Result<RuntimeSessionBindingV1, JavaParityRuntimeErrorV1> {
    let protocol_version = live.frame["protocol"]["minor"]
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(JavaParityRuntimeErrorV1::MissingProtocolMinor)?;
    RuntimeSessionBindingV1::new(
        format!(
            "{}:{}:{}",
            live.manifest.producer_session_id,
            live.manifest.connection_id,
            live.manifest.capture_id
        ),
        &live.manifest.session_id,
        &live.manifest.connection_id,
        live.manifest.process_id,
        &live.manifest.sts_jar_sha256,
        &live.manifest.loaded_mod_jar_sha256,
        protocol_version,
    )
    .map_err(JavaParityRuntimeErrorV1::Supervised)
}

fn same_existing_path(left: &Path, right: &Path) -> Result<bool, JavaParityRuntimeErrorV1> {
    let left = fs::canonicalize(left)
        .map_err(|error| JavaParityRuntimeErrorV1::LaunchPlanPath(error.to_string()))?;
    let right = fs::canonicalize(right)
        .map_err(|error| JavaParityRuntimeErrorV1::LaunchPlanPath(error.to_string()))?;
    #[cfg(windows)]
    {
        Ok(normalized_windows_path(&left).eq_ignore_ascii_case(&normalized_windows_path(&right)))
    }
    #[cfg(not(windows))]
    {
        Ok(left == right)
    }
}

#[cfg(windows)]
fn normalized_windows_path(path: &Path) -> String {
    let rendered = path.to_string_lossy();
    if let Some(tail) = rendered.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{tail}")
    } else if let Some(tail) = rendered.strip_prefix(r"\\?\") {
        tail.to_owned()
    } else {
        rendered.into_owned()
    }
}
