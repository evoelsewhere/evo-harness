//! Who is on the other end of the pipe (spec FR-3): the pipe's per-user
//! address, the executable of a process, and the rule both sides apply — the
//! peer must run from the same folder as oneself (the host ships next to the
//! app).

use std::path::{Path, PathBuf};

/// Whether two executables sit in the same folder (case-insensitive on Windows).
pub fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.parent(), b.parent()) {
        (Some(a), Some(b)) => {
            if cfg!(windows) {
                a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
            } else {
                a == b
            }
        }
        _ => false,
    }
}

/// Whether `peer` is the executable named `expected` in `own`'s folder.
pub fn is_sibling(own: &Path, peer: &Path, expected: &[&str]) -> bool {
    let name = peer
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    same_folder(own, peer) && expected.iter().any(|e| e.to_lowercase() == name)
}

/// The pipe's address for the current user.
#[cfg(windows)]
pub fn pipe_address() -> std::io::Result<String> {
    Ok(format!(
        r"\\.\pipe\evo-webbridge-{}",
        win::current_user_sid()?
    ))
}

/// The socket's path for the current user.
#[cfg(unix)]
pub fn pipe_address() -> std::io::Result<String> {
    let dir = match std::env::var_os("EVO_CODING_AGENT_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => {
            let home =
                std::env::var_os("HOME").ok_or_else(|| std::io::Error::other("HOME is not set"))?;
            PathBuf::from(home).join(".evo").join("agent")
        }
    };
    Ok(dir
        .join("webbridge")
        .join("bridge.sock")
        .to_string_lossy()
        .into_owned())
}

/// The executable of process `pid`.
#[cfg(windows)]
pub fn process_image(pid: u32) -> std::io::Result<PathBuf> {
    win::process_image(pid)
}

#[cfg(target_os = "linux")]
pub fn process_image(pid: u32) -> std::io::Result<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
}

#[cfg(all(unix, not(target_os = "linux")))]
pub fn process_image(_pid: u32) -> std::io::Result<PathBuf> {
    Err(std::io::Error::other(
        "process images are not checked on this platform yet",
    ))
}

/// The process that serves a pipe the caller connected to.
#[cfg(windows)]
pub fn pipe_server_pid(handle: std::os::windows::io::RawHandle) -> std::io::Result<u32> {
    win::pipe_server_pid(handle)
}

/// The process that connected to a pipe the caller serves.
#[cfg(windows)]
pub fn pipe_client_pid(handle: std::os::windows::io::RawHandle) -> std::io::Result<u32> {
    win::pipe_client_pid(handle)
}

/// A new instance of the app's pipe that only the current user (and the
/// system) can open; `first` refuses to start when another process already
/// serves the name.
#[cfg(windows)]
pub fn create_server(
    address: &str,
    first: bool,
) -> std::io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    win::create_server(address, first)
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::io;
    use std::os::windows::io::RawHandle;
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::System::Pipes::{
        GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    struct Owned(HANDLE);
    impl Drop for Owned {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn current_user_sid() -> io::Result<String> {
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return Err(io::Error::last_os_error());
            }
            let token = Owned(token);
            let mut needed = 0u32;
            GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut needed);
            let mut buf = vec![0u8; needed as usize];
            if GetTokenInformation(
                token.0,
                TokenUser,
                buf.as_mut_ptr() as *mut c_void,
                needed,
                &mut needed,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut text: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut len = 0;
            while *text.add(len) != 0 {
                len += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text as *mut c_void);
            Ok(sid)
        }
    }

    pub fn process_image(pid: u32) -> io::Result<PathBuf> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return Err(io::Error::last_os_error());
            }
            let process = Owned(process);
            let mut buf = vec![0u16; 32_768];
            let mut size = buf.len() as u32;
            if QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_WIN32,
                buf.as_mut_ptr(),
                &mut size,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(PathBuf::from(String::from_utf16_lossy(
                &buf[..size as usize],
            )))
        }
    }

    pub fn pipe_server_pid(handle: RawHandle) -> io::Result<u32> {
        let mut pid = 0u32;
        if unsafe { GetNamedPipeServerProcessId(handle as HANDLE, &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(pid)
    }

    pub fn create_server(
        address: &str,
        first: bool,
    ) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};

        // Full access for this user and the system; nobody else, no inherited entries.
        let sddl = format!("D:P(A;;GA;;;{})(A;;GA;;;SY)", current_user_sid()?);
        let wide: Vec<u16> = sddl.encode_utf16().chain(std::iter::once(0)).collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let mut attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
        options
            .first_pipe_instance(first)
            .reject_remote_clients(true);
        let server = unsafe {
            options.create_with_security_attributes_raw(
                address,
                &mut attributes as *mut _ as *mut c_void,
            )
        };
        unsafe { LocalFree(descriptor) };
        server
    }

    pub fn pipe_client_pid(handle: RawHandle) -> io::Result<u32> {
        let mut pid = 0u32;
        if unsafe { GetNamedPipeClientProcessId(handle as HANDLE, &mut pid) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(pid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sibling_runs_from_the_same_folder_under_an_expected_name() {
        let own = Path::new("/apps/evo/evo-harness");
        assert!(is_sibling(
            own,
            Path::new("/apps/evo/evo-bridge-host"),
            &["evo-bridge-host"]
        ));
        assert!(!is_sibling(
            own,
            Path::new("/tmp/evo-bridge-host"),
            &["evo-bridge-host"]
        ));
        assert!(!is_sibling(
            own,
            Path::new("/apps/evo/other"),
            &["evo-bridge-host"]
        ));
    }

    #[cfg(windows)]
    #[test]
    fn folders_compare_without_case_on_windows() {
        assert!(same_folder(
            Path::new(r"C:\Program Files\Evo\a.exe"),
            Path::new(r"c:\program files\evo\B.EXE")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn the_pipe_address_names_the_current_user() {
        let address = pipe_address().unwrap();
        assert!(
            address.starts_with(r"\\.\pipe\evo-webbridge-S-1-"),
            "{address}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn the_own_process_image_is_found() {
        let image = process_image(std::process::id()).unwrap();
        assert_eq!(image, std::env::current_exe().unwrap());
    }
}
