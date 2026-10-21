use std::path::Path;
use sts_workbench_supervisor::{
    NativeRecoveryCapsuleRegistryV1, OwnedStorageRegistryV1, RepositoryLifecycleOwnerV1,
};

use crate::RuntimeStartupReconciliationV1;

pub(super) const GAME_DIRECTORY: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\SlayTheSpire";

/// Game-specific startup reporting attached to the Supervisor-owned repository
/// and disposable-storage lifecycle.
pub(super) struct ActorRuntimeOwnerV1 {
    supervisor_owner: RepositoryLifecycleOwnerV1,
    storage_registry: OwnedStorageRegistryV1,
    startup_reconciliation: RuntimeStartupReconciliationV1,
}

impl ActorRuntimeOwnerV1 {
    pub(super) fn acquire(repository: &Path) -> Result<Self, String> {
        let supervisor_owner =
            RepositoryLifecycleOwnerV1::acquire(repository).map_err(|error| error.to_string())?;
        let storage_registry =
            OwnedStorageRegistryV1::new(&supervisor_owner, Path::new(GAME_DIRECTORY));
        let reconciliation = storage_registry
            .reconcile_startup_and_persist()
            .map_err(|error| error.to_string())?;
        let startup_reconciliation = RuntimeStartupReconciliationV1 {
            health: if reconciliation.unrecognized_markers.is_empty() {
                if reconciliation.reclaimed_storage.is_empty() {
                    "clean"
                } else {
                    "reclaimed"
                }
            } else {
                "scoped_unknown"
            }
            .to_owned(),
            reclaimed_storage: reconciliation.reclaimed_storage.len(),
            foreign_owned_storage: reconciliation.foreign_owned_storage.len(),
            unrecognized_markers: reconciliation.unrecognized_markers.len(),
        };

        Ok(Self {
            supervisor_owner,
            storage_registry,
            startup_reconciliation,
        })
    }

    pub(super) fn storage_registry(&self) -> OwnedStorageRegistryV1 {
        self.storage_registry.clone()
    }

    pub(super) fn native_recovery_capsules(
        &self,
        content_store_root: &Path,
    ) -> Result<NativeRecoveryCapsuleRegistryV1, String> {
        NativeRecoveryCapsuleRegistryV1::new(
            &self.supervisor_owner,
            self.storage_registry.clone(),
            content_store_root,
        )
        .map_err(|error| error.to_string())
    }

    pub(super) fn startup_reconciliation(&self) -> &RuntimeStartupReconciliationV1 {
        &self.startup_reconciliation
    }
}

pub(super) fn same_path(left: &Path, right: &Path) -> bool {
    normalized_windows_path(left).eq_ignore_ascii_case(&normalized_windows_path(right))
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    #[cfg(windows)]
    use std::process::{Command, Stdio};
    #[cfg(windows)]
    use std::thread;
    #[cfg(windows)]
    use std::time::{Duration, Instant};
    use uuid::Uuid;

    #[cfg(windows)]
    #[test]
    fn actor_process_job_kills_an_inherited_child_on_exit() {
        if env::var_os("STS_WORKBENCH_JOB_HELPER").is_some() {
            return;
        }
        let base = env::temp_dir().join(format!("sts-workbench-job-{}", Uuid::new_v4()));
        let repository = base.join("repository");
        let child_pid_path = base.join("child.pid");
        fs::create_dir_all(&repository).unwrap();
        let status = Command::new(env::current_exe().unwrap())
            .arg("--exact")
            .arg("runtime_ownership::tests::actor_process_job_helper")
            .arg("--nocapture")
            .env("STS_WORKBENCH_JOB_HELPER", "1")
            .env("STS_WORKBENCH_JOB_REPOSITORY", &repository)
            .env("STS_WORKBENCH_JOB_CHILD_PID", &child_pid_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let child_pid = fs::read_to_string(&child_pid_path)
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while process_is_running(child_pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(25));
        }
        assert!(!process_is_running(child_pid));
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn actor_process_job_helper() {
        if env::var_os("STS_WORKBENCH_JOB_HELPER").is_none() {
            return;
        }
        let repository = PathBuf::from(env::var_os("STS_WORKBENCH_JOB_REPOSITORY").unwrap());
        let child_pid_path = PathBuf::from(env::var_os("STS_WORKBENCH_JOB_CHILD_PID").unwrap());
        let _owner = ActorRuntimeOwnerV1::acquire(&repository).unwrap();
        let child = Command::new("powershell.exe")
            .arg("-NoLogo")
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg("Start-Sleep -Seconds 60")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        fs::write(child_pid_path, child.id().to_string()).unwrap();
        std::process::exit(0);
    }

    #[cfg(windows)]
    fn process_is_running(process_id: u32) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
        };

        // SAFETY: the handle is query/synchronize-only and is closed below.
        let process = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, process_id) };
        if process.is_null() {
            return false;
        }
        // SAFETY: `process` is a valid synchronize handle.
        let running = unsafe { WaitForSingleObject(process, 0) } == WAIT_TIMEOUT;
        // SAFETY: this function owns the handle returned by OpenProcess.
        unsafe { CloseHandle(process) };
        running
    }
}
