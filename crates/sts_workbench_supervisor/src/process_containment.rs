use std::fmt;

// This persisted value predates the Supervisor/Engine split. Keep it stable so
// existing owned-storage markers do not silently lose their cleanup identity.
pub const PROCESS_CONTAINMENT_CLASS_V1: &str = "windows-actor-job-kill-on-close-v1";

#[derive(Debug, Eq, PartialEq)]
pub enum ProcessContainmentErrorV1 {
    UnsupportedPlatform,
    Create(String),
    Configure(String),
    Assign(String),
    Verify,
}

impl fmt::Display for ProcessContainmentErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for ProcessContainmentErrorV1 {}

/// Process-lifetime ownership for every descendant launched after acquisition.
///
/// The raw Windows handle intentionally has no `Drop` implementation. Closing
/// it would terminate the current owner process as well as its descendants;
/// the operating system closes it when that owner process exits.
pub struct ProcessContainmentV1 {
    #[cfg(windows)]
    _process_lifetime_job: windows_sys::Win32::Foundation::HANDLE,
}

impl ProcessContainmentV1 {
    #[cfg(not(windows))]
    pub fn acquire_current_process() -> Result<Self, ProcessContainmentErrorV1> {
        Err(ProcessContainmentErrorV1::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub fn acquire_current_process() -> Result<Self, ProcessContainmentErrorV1> {
        use std::ffi::c_void;
        use std::ptr::null;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        // SAFETY: null security/name arguments request one unnamed job.
        let job = unsafe { CreateJobObjectW(null(), null()) };
        if job.is_null() {
            return Err(ProcessContainmentErrorV1::Create(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `limits` matches the declared information class and `job` is live.
        if unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast::<c_void>(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            let error = std::io::Error::last_os_error().to_string();
            // SAFETY: assignment has not happened, so this process is not killed.
            unsafe { CloseHandle(job) };
            return Err(ProcessContainmentErrorV1::Configure(error));
        }
        // SAFETY: both handles are live.
        if unsafe { AssignProcessToJobObject(job, GetCurrentProcess()) } == 0 {
            let error = std::io::Error::last_os_error().to_string();
            // SAFETY: the assignment call failed, so closing the sole handle is safe.
            unsafe { CloseHandle(job) };
            return Err(ProcessContainmentErrorV1::Assign(error));
        }
        let mut assigned = 0;
        // SAFETY: both handles are live and `assigned` is writable.
        if unsafe { IsProcessInJob(GetCurrentProcess(), job, &mut assigned) } == 0 || assigned == 0
        {
            // Assignment may already have succeeded. Do not close the kill-on-
            // close handle while the current process belongs to this job.
            return Err(ProcessContainmentErrorV1::Verify);
        }
        Ok(Self {
            _process_lifetime_job: job,
        })
    }

    pub fn class(&self) -> &'static str {
        PROCESS_CONTAINMENT_CLASS_V1
    }
}
