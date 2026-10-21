use std::path::Path;

#[cfg(not(windows))]
use std::process::{Child, Command, Stdio};

#[cfg(windows)]
use std::ffi::{c_void, OsStr, OsString};
#[cfg(windows)]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
#[cfg(windows)]
use std::ptr::{null, null_mut};
#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, OpenProcess, QueryFullProcessImageNameW,
    UpdateProcThreadAttribute, WaitForSingleObject, CREATE_BREAKAWAY_FROM_JOB, DETACHED_PROCESS,
    EXTENDED_STARTUPINFO_PRESENT, PROCESS_CREATE_PROCESS, PROCESS_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROC_THREAD_ATTRIBUTE_PARENT_PROCESS, STARTUPINFOEXW,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

pub(super) struct ResidentProcess {
    #[cfg(not(windows))]
    child: Child,
    #[cfg(windows)]
    process: OwnedHandle,
    #[cfg(windows)]
    process_id: u32,
}

#[cfg(windows)]
pub(super) fn process_matches_executable(process_id: u32, expected: &Path) -> bool {
    // SAFETY: query-only access to an OS process id does not transfer ownership.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if handle.is_null() {
        return false;
    }
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    // SAFETY: `buffer` is writable for `length` UTF-16 code units and `handle`
    // remains valid until the matching CloseHandle below.
    let read = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) };
    // SAFETY: `handle` was returned by OpenProcess in this function.
    unsafe { CloseHandle(handle) };
    if read == 0 {
        return false;
    }
    let actual = Path::new(&OsString::from_wide(&buffer[..length as usize])).to_path_buf();
    paths_refer_to_same_file(&actual, expected)
}

#[cfg(any(target_os = "linux", target_os = "android"))]
pub(super) fn process_matches_executable(process_id: u32, expected: &Path) -> bool {
    std::fs::read_link(format!("/proc/{process_id}/exe"))
        .ok()
        .is_some_and(|actual| paths_refer_to_same_file(&actual, expected))
}

#[cfg(not(any(windows, target_os = "linux", target_os = "android")))]
pub(super) fn process_matches_executable(_process_id: u32, _expected: &Path) -> bool {
    false
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    left == right
        || match (left.canonicalize(), right.canonicalize()) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        }
}

impl ResidentProcess {
    pub(super) fn id(&self) -> u32 {
        #[cfg(not(windows))]
        {
            self.child.id()
        }
        #[cfg(windows)]
        {
            self.process_id
        }
    }

    pub(super) fn try_exit(&mut self) -> Result<Option<String>, String> {
        #[cfg(not(windows))]
        {
            self.child
                .try_wait()
                .map(|status| status.map(|status| status.to_string()))
                .map_err(|error| format!("failed to inspect resident oracle service: {error}"))
        }
        #[cfg(windows)]
        {
            // SAFETY: the owned process handle remains valid for this call.
            let wait = unsafe { WaitForSingleObject(self.process.0, 0) };
            match wait {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut exit_code = 0;
                    // SAFETY: `exit_code` is writable and the process handle is still owned.
                    if unsafe { GetExitCodeProcess(self.process.0, &mut exit_code) } == 0 {
                        return Err(format!(
                            "failed to read resident oracle service exit code: {}",
                            std::io::Error::last_os_error()
                        ));
                    }
                    Ok(Some(format!("exit code {exit_code}")))
                }
                other => Err(format!(
                    "failed to inspect resident oracle service: WaitForSingleObject returned {other:#x}"
                )),
            }
        }
    }
}

pub(super) fn spawn_resident_service(
    executable: &Path,
    workspace: &Path,
    endpoint: &Path,
    repository_root: &Path,
) -> Result<ResidentProcess, String> {
    #[cfg(not(windows))]
    {
        let child = Command::new(executable)
            .current_dir(repository_root)
            .arg("--canonical-oracle")
            .arg("--workspace")
            .arg(workspace)
            .arg("--endpoint")
            .arg(endpoint)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                format!(
                    "failed to start resident oracle service '{}': {error}",
                    executable.display()
                )
            })?;
        Ok(ResidentProcess { child })
    }
    #[cfg(windows)]
    {
        spawn_windows_resident_service(executable, workspace, endpoint, repository_root)
    }
}

#[cfg(windows)]
fn spawn_windows_resident_service(
    executable: &Path,
    workspace: &Path,
    endpoint: &Path,
    repository_root: &Path,
) -> Result<ResidentProcess, String> {
    let parent = shell_parent_process()?;
    let mut attribute_bytes = 0;
    // SAFETY: a null list is the documented sizing call; `attribute_bytes`
    // points to writable storage.
    unsafe {
        InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut attribute_bytes);
    }
    if attribute_bytes == 0 {
        return Err(format!(
            "failed to size resident process attribute list: {}",
            std::io::Error::last_os_error()
        ));
    }
    let word_size = std::mem::size_of::<usize>();
    let mut attribute_storage =
        vec![0usize; attribute_bytes.saturating_add(word_size - 1) / word_size];
    let attribute_list = attribute_storage.as_mut_ptr().cast();
    // SAFETY: `attribute_storage` is aligned, sized from the API's first call,
    // and lives until after the attribute list is deleted.
    if unsafe { InitializeProcThreadAttributeList(attribute_list, 1, 0, &mut attribute_bytes) } == 0
    {
        return Err(format!(
            "failed to initialize resident process attribute list: {}",
            std::io::Error::last_os_error()
        ));
    }
    let attributes = OwnedAttributeList(attribute_list);
    let parent_handle = parent.0;
    // SAFETY: both the initialized attribute list and parent handle remain
    // alive through `CreateProcessW`.
    if unsafe {
        UpdateProcThreadAttribute(
            attributes.0,
            0,
            PROC_THREAD_ATTRIBUTE_PARENT_PROCESS as usize,
            (&parent_handle as *const HANDLE).cast::<c_void>(),
            std::mem::size_of::<HANDLE>(),
            null_mut(),
            null(),
        )
    } == 0
    {
        return Err(format!(
            "failed to attach resident service to the Windows shell process: {}",
            std::io::Error::last_os_error()
        ));
    }

    let executable_wide = nul_terminated(executable.as_os_str());
    let current_directory = nul_terminated(repository_root.as_os_str());
    let mut command_line = windows_command_line([
        executable.as_os_str(),
        OsStr::new("--canonical-oracle"),
        OsStr::new("--workspace"),
        workspace.as_os_str(),
        OsStr::new("--endpoint"),
        endpoint.as_os_str(),
    ]);
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.lpAttributeList = attributes.0;
    let mut process_info = PROCESS_INFORMATION::default();
    // SAFETY: every pointer is either null or references writable,
    // NUL-terminated storage that outlives this call.
    let created = unsafe {
        CreateProcessW(
            executable_wide.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_BREAKAWAY_FROM_JOB | DETACHED_PROCESS,
            null(),
            current_directory.as_ptr(),
            &startup.StartupInfo,
            &mut process_info,
        )
    };
    if created == 0 {
        return Err(format!(
            "failed to start resident oracle service '{}': {}",
            executable.display(),
            std::io::Error::last_os_error()
        ));
    }
    let process = OwnedHandle(process_info.hProcess);
    let thread = OwnedHandle(process_info.hThread);
    drop(thread);
    Ok(ResidentProcess {
        process,
        process_id: process_info.dwProcessId,
    })
}

#[cfg(windows)]
fn shell_parent_process() -> Result<OwnedHandle, String> {
    // SAFETY: these calls take no borrowed buffers and only return opaque
    // desktop-shell identifiers.
    let shell_window = unsafe { GetShellWindow() };
    if shell_window.is_null() {
        return Err(
            "failed to find the Windows shell process needed to own a resident service".to_string(),
        );
    }
    let mut process_id = 0;
    // SAFETY: `process_id` points to writable storage for the duration of the call.
    if unsafe { GetWindowThreadProcessId(shell_window, &mut process_id) } == 0 || process_id == 0 {
        return Err(format!(
            "failed to identify the Windows shell process: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: `process_id` came from the current interactive shell window; the
    // returned handle is immediately wrapped for deterministic closure.
    let handle = unsafe { OpenProcess(PROCESS_CREATE_PROCESS, 0, process_id) };
    if handle.is_null() {
        return Err(format!(
            "failed to open Windows shell process {process_id} for resident launch: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(OwnedHandle(handle))
}

#[cfg(windows)]
fn windows_command_line<'a>(arguments: impl IntoIterator<Item = &'a OsStr>) -> Vec<u16> {
    let mut command_line = Vec::new();
    for argument in arguments {
        if !command_line.is_empty() {
            command_line.push(b' ' as u16);
        }
        append_windows_argument(&mut command_line, argument);
    }
    command_line.push(0);
    command_line
}

#[cfg(windows)]
fn append_windows_argument(command_line: &mut Vec<u16>, argument: &OsStr) {
    let argument: Vec<u16> = argument.encode_wide().collect();
    let needs_quotes = argument.is_empty()
        || argument
            .iter()
            .any(|character| matches!(*character, 9 | 32 | 34));
    if !needs_quotes {
        command_line.extend(argument);
        return;
    }

    command_line.push(b'"' as u16);
    let mut backslashes = 0usize;
    for character in argument {
        if character == b'\\' as u16 {
            backslashes += 1;
            continue;
        }
        if character == b'"' as u16 {
            command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
        } else {
            command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
        }
        backslashes = 0;
        command_line.push(character);
    }
    command_line.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    command_line.push(b'"' as u16);
}

#[cfg(windows)]
fn nul_terminated(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
struct OwnedHandle(HANDLE);

#[cfg(windows)]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: `OwnedHandle` is the sole owner and closes exactly once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(windows)]
struct OwnedAttributeList(windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST);

#[cfg(windows)]
impl Drop for OwnedAttributeList {
    fn drop(&mut self) {
        // SAFETY: the list was initialized successfully and is deleted exactly once.
        unsafe {
            DeleteProcThreadAttributeList(self.0);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn windows_command_line_quotes_spaces_quotes_and_trailing_backslashes() {
        let command = windows_command_line([
            OsStr::new("plain"),
            OsStr::new("two words"),
            OsStr::new("say\"hi"),
            OsStr::new("ends with slash\\"),
        ]);
        let rendered = String::from_utf16(&command[..command.len() - 1]).expect("UTF-16 command");
        assert_eq!(
            rendered,
            r#"plain "two words" "say\"hi" "ends with slash\\""#,
        );
    }
}
