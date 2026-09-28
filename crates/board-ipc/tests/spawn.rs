//! Detached spawn must hand the child only the handles it names.
#![cfg(windows)]

use std::io::Read;
use std::os::windows::io::AsRawHandle;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT};

/// A plain `CreateProcess(bInheritHandles = TRUE)` would copy every inheritable
/// handle — like a caller's stdout pipe — into the long-lived child, and the
/// caller's reader would never see EOF.
#[test]
fn detached_child_does_not_inherit_unlisted_handles() {
    let (mut reader, writer) = std::io::pipe().unwrap();
    // SAFETY: valid handle owned by `writer`.
    let ok = unsafe {
        SetHandleInformation(
            writer.as_raw_handle() as HANDLE,
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        )
    };
    assert_ne!(ok, 0);

    let dir = tempfile::tempdir().unwrap();
    let log = std::fs::File::create(dir.path().join("stderr.log")).unwrap();
    let cmd = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32")
        .join("cmd.exe");
    let pid =
        board_ipc::spawn_detached(&cmd, &["/c", "ping", "-n", "4", "127.0.0.1"], log).unwrap();
    assert_ne!(pid, 0);

    drop(writer);
    let started = Instant::now();
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).unwrap();
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "the detached child inherited the pipe's write end"
    );
}
