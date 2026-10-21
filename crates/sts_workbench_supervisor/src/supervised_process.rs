use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Eq, PartialEq)]
pub enum SupervisedProcessErrorV1 {
    UnsupportedPlatform,
    InvalidProcessId,
    ResolveExpectedExecutable(String),
    Open(String),
    QueryExecutable(String),
    ExecutableMismatch { expected: PathBuf, actual: PathBuf },
    NotRunning,
    Wait(String),
    Terminate(String),
    Deadline,
}

impl fmt::Display for SupervisedProcessErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                write!(
                    formatter,
                    "exact process handles are unsupported on this platform"
                )
            }
            Self::InvalidProcessId => write!(formatter, "process id must be non-zero"),
            Self::ResolveExpectedExecutable(detail) => {
                write!(formatter, "failed to resolve expected executable: {detail}")
            }
            Self::Open(detail) => write!(formatter, "failed to open process: {detail}"),
            Self::QueryExecutable(detail) => {
                write!(formatter, "failed to query process executable: {detail}")
            }
            Self::ExecutableMismatch { expected, actual } => write!(
                formatter,
                "process executable mismatch: expected {}, found {}",
                expected.display(),
                actual.display()
            ),
            Self::NotRunning => write!(formatter, "process exited before ownership was admitted"),
            Self::Wait(detail) => write!(formatter, "failed to wait for process: {detail}"),
            Self::Terminate(detail) => write!(formatter, "failed to terminate process: {detail}"),
            Self::Deadline => write!(formatter, "process did not exit before its deadline"),
        }
    }
}

impl std::error::Error for SupervisedProcessErrorV1 {}

/// A non-owning admission candidate for a process created by another owner.
///
/// Opening by PID is not enough to grant termination authority. The caller can
/// first validate its own protocol/session identity while this exact OS handle
/// remains open, then explicitly promote it with [`Self::admit`]. Dropping an
/// unadmitted candidate only closes the handle and never terminates a process.
#[derive(Debug)]
pub struct ExistingProcessCandidateV1 {
    process_id: u32,
    #[cfg(windows)]
    process: OwnedProcessHandle,
}

impl ExistingProcessCandidateV1 {
    #[cfg(not(windows))]
    pub fn open_running(
        _process_id: u32,
        _expected_executable: &Path,
    ) -> Result<Self, SupervisedProcessErrorV1> {
        Err(SupervisedProcessErrorV1::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub fn open_running(
        process_id: u32,
        expected_executable: &Path,
    ) -> Result<Self, SupervisedProcessErrorV1> {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
        };

        if process_id == 0 {
            return Err(SupervisedProcessErrorV1::InvalidProcessId);
        }
        let expected = std::fs::canonicalize(expected_executable).map_err(|error| {
            SupervisedProcessErrorV1::ResolveExpectedExecutable(error.to_string())
        })?;
        // SAFETY: the returned handle is immediately wrapped and closed exactly once.
        let raw = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                0,
                process_id,
            )
        };
        if raw.is_null() {
            return Err(SupervisedProcessErrorV1::Open(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        let process = OwnedProcessHandle(raw);
        let actual = process_executable(&process)?;
        let actual = std::fs::canonicalize(&actual).unwrap_or(actual);
        if !same_windows_path(&expected, &actual) {
            return Err(SupervisedProcessErrorV1::ExecutableMismatch { expected, actual });
        }
        let candidate = Self {
            process_id,
            process,
        };
        if !candidate.is_running()? {
            return Err(SupervisedProcessErrorV1::NotRunning);
        }
        Ok(candidate)
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    #[cfg(not(windows))]
    pub fn is_running(&self) -> Result<bool, SupervisedProcessErrorV1> {
        Err(SupervisedProcessErrorV1::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub fn is_running(&self) -> Result<bool, SupervisedProcessErrorV1> {
        process_is_running(&self.process)
    }

    pub fn admit(self) -> SupervisedProcessV1 {
        SupervisedProcessV1 {
            process_id: self.process_id,
            #[cfg(windows)]
            process: self.process,
            closed: false,
        }
    }
}

/// An exact, admitted process handle owned by the lifecycle Supervisor.
///
/// This is intended for a leaf process spawned indirectly by another managed
/// process. It never rediscovers the process by PID after admission.
#[derive(Debug)]
pub struct SupervisedProcessV1 {
    process_id: u32,
    #[cfg(windows)]
    process: OwnedProcessHandle,
    closed: bool,
}

impl SupervisedProcessV1 {
    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    #[cfg(not(windows))]
    pub fn is_running(&self) -> Result<bool, SupervisedProcessErrorV1> {
        Err(SupervisedProcessErrorV1::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub fn is_running(&self) -> Result<bool, SupervisedProcessErrorV1> {
        process_is_running(&self.process)
    }

    #[cfg(not(windows))]
    pub fn shutdown(&mut self, _timeout: Duration) -> Result<(), SupervisedProcessErrorV1> {
        Err(SupervisedProcessErrorV1::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub fn shutdown(&mut self, timeout: Duration) -> Result<(), SupervisedProcessErrorV1> {
        use windows_sys::Win32::System::Threading::TerminateProcess;

        if self.closed {
            return Ok(());
        }
        if !self.is_running()? {
            self.closed = true;
            return Ok(());
        }
        // SAFETY: this exact process handle was opened with PROCESS_TERMINATE.
        if unsafe { TerminateProcess(self.process.0, 1) } == 0 {
            if !self.is_running()? {
                self.closed = true;
                return Ok(());
            }
            return Err(SupervisedProcessErrorV1::Terminate(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        wait_for_exit(&self.process, timeout)?;
        self.closed = true;
        Ok(())
    }
}

impl Drop for SupervisedProcessV1 {
    fn drop(&mut self) {
        let _ = self.shutdown(Duration::from_secs(5));
    }
}

#[cfg(windows)]
fn process_executable(process: &OwnedProcessHandle) -> Result<PathBuf, SupervisedProcessErrorV1> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Threading::QueryFullProcessImageNameW;

    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    // SAFETY: `buffer` is writable for `length` UTF-16 code units and the
    // candidate owns a query-capable process handle for this entire call.
    if unsafe { QueryFullProcessImageNameW(process.0, 0, buffer.as_mut_ptr(), &mut length) } == 0 {
        return Err(SupervisedProcessErrorV1::QueryExecutable(
            std::io::Error::last_os_error().to_string(),
        ));
    }
    Ok(PathBuf::from(std::ffi::OsString::from_wide(
        &buffer[..length as usize],
    )))
}

#[cfg(windows)]
fn same_windows_path(left: &Path, right: &Path) -> bool {
    normalized_windows_path(left).eq_ignore_ascii_case(&normalized_windows_path(right))
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

#[cfg(windows)]
fn process_is_running(process: &OwnedProcessHandle) -> Result<bool, SupervisedProcessErrorV1> {
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    // SAFETY: this exact process handle remains owned for the call.
    match unsafe { WaitForSingleObject(process.0, 0) } {
        WAIT_TIMEOUT => Ok(true),
        WAIT_OBJECT_0 => Ok(false),
        _ => Err(SupervisedProcessErrorV1::Wait(
            std::io::Error::last_os_error().to_string(),
        )),
    }
}

#[cfg(windows)]
fn wait_for_exit(
    process: &OwnedProcessHandle,
    timeout: Duration,
) -> Result<(), SupervisedProcessErrorV1> {
    use windows_sys::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    let timeout_ms = timeout.as_millis().min(u128::from(u32::MAX - 1)) as u32;
    // SAFETY: this exact process handle remains owned for the bounded wait.
    match unsafe { WaitForSingleObject(process.0, timeout_ms) } {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err(SupervisedProcessErrorV1::Deadline),
        _ => Err(SupervisedProcessErrorV1::Wait(
            std::io::Error::last_os_error().to_string(),
        )),
    }
}

#[cfg(windows)]
#[derive(Debug)]
struct OwnedProcessHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for OwnedProcessHandle {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;

        if !self.0.is_null() {
            // SAFETY: this wrapper owns the handle and closes it exactly once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::*;

    #[cfg(windows)]
    #[test]
    fn candidate_is_non_owning_until_admitted_then_shutdown_is_exact_and_idempotent() {
        use std::process::{Command, Stdio};

        let executable = PathBuf::from(std::env::var_os("COMSPEC").unwrap());
        let mut child = Command::new(&executable)
            .args(["/D", "/C", "ping", "-n", "30", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let candidate = ExistingProcessCandidateV1::open_running(child.id(), &executable).unwrap();
        assert_eq!(candidate.process_id(), child.id());
        assert!(candidate.is_running().unwrap());
        drop(candidate);
        assert!(child.try_wait().unwrap().is_none());

        let candidate = ExistingProcessCandidateV1::open_running(child.id(), &executable).unwrap();
        let mut supervised = candidate.admit();
        supervised.shutdown(Duration::from_secs(5)).unwrap();
        supervised.shutdown(Duration::from_millis(1)).unwrap();
        assert!(!supervised.is_running().unwrap());
        child.wait().unwrap();
    }
}
