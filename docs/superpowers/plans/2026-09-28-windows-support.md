# Native Windows Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `board` (TUI + daemon + CLI) builds, runs, and is CI-verified (unit + live e2e) on native Windows against Windows Herdr 0.9.x / protocol 22, with Linux/macOS behavior unchanged.

**Architecture:** A new leaf crate `board-ipc` hides the platform transport: on Unix it re-exports `UnixStream`/`UnixListener`; on Windows it provides a named-pipe `PipeStream`/`PipeListener` (`\\.\pipe\` + socket path, same convention as Herdr) with peek-based reads, emulated timeouts, and a same-user owner check. `board-herdr` and `board-core` consume it; `board-daemon` serves boardd on a tokio named pipe. Every other Unix-only mechanism (flock, signals, process groups, chmod, `/bin/sh` script, `O_NOFOLLOW`) gets a `cfg(windows)` sibling.

**Tech Stack:** Rust 2021 workspace (toolchain 1.98), `windows-sys` 0.59, tokio (`net` → `windows::named_pipe`, `signal` → `windows::ctrl_*`), Python 3 `ctypes` for the e2e harness, Git Bash on the Windows CI runner.

**Spec:** `docs/superpowers/specs/2026-09-28-windows-support-design.md`

## Global Constraints

- One PR, branch `feat/windows-support` → `dev`. Conventional Commits per crate/intent (`feat(ipc): …`, `feat(herdr,core): …`). No Claude attribution or `Co-Authored-By` trailer in commits or PR body (user's global CLAUDE.md).
- Herdr contract: Herdr 0.9.0 reference, protocol 22 only. Windows Herdr pipe name = `\\.\pipe\` + socket path, verbatim.
- Linux/macOS code paths change only by `cfg` gating. The one deliberate shared change is the singleton (`File::try_lock`, still `flock` on Unix).
- `anyhow` at edges, `thiserror` in core, no `unwrap()` outside tests. All `unsafe` Win32 code lives in `board-ipc` (plus one const-only flag in `board-core`).
- `windows-sys = "0.59"` (already in `Cargo.lock`); add it only as a `[target.'cfg(windows)'.dependencies]` entry.
- `UnixClient` keeps its public name (backward compatibility); it is the boardd client on every platform.
- Test-first: every behavior task writes the failing test before the code.
- **Host execution exception (document in the PR):** the Docker sandbox is Linux/macOS-only, so on this Windows machine `cargo test --workspace` runs on the host. It is hermetic (live-Herdr tests are `#[ignore]`d; fakes bind pipes under `tempfile` paths). Unix compile/regression is checked in WSL (`wsl -d Ubuntu`) with `cargo clippy --workspace --all-targets --all-features -- -D warnings` (compile only) and by CI Linux jobs. The live e2e suite is **never** run on this host without the user's explicit go-ahead at that moment; CI's Windows runner is its primary home.
- Mutations against the user's real Herdr only in disposable workspaces created for the check, each prefixed `HERDR MUTATION:` in the log, and only after the user approves Task 11.
- CHANGELOG: one `Unreleased` → `### Added` entry, ≤ 200 chars, starting with the PR link.

## Review Focus

- **Paths with spaces or non-ASCII in the user profile** (`C:\Users\Jan Kowalski\…`): pipe names, the `.ps1` script path handed to `herdr pane run`, and `%TEMP%` must all work. Pinned by tests in Tasks 1 and 8.
- **PowerShell smart quotes**: PowerShell treats `‘ ’ ‚ ‛` as single quotes, so an argv containing them must still round-trip. Pinned by the `ps_quote` test in Task 8.
- **Peer closes mid-read / daemon exits while the TUI is subscribed**: a Windows pipe returns `ERROR_BROKEN_PIPE`/`ERROR_PIPE_NOT_CONNECTED`; readers must see EOF (`Ok(0)`), not an error loop or a hang. Pinned by the `eof_after_server_drop` test in Task 1.
- **A second `board daemon` on Windows**: `first_pipe_instance(true)` plus the lock file must make it exit cleanly (`Ok(None)`), not crash or steal the name. Pinned by the singleton test in Task 5 and the `second_bind_fails` test in Task 6.
- **A provider CLI installed through npm (`pi.cmd`, `opencode.cmd`, `agy.cmd`)**: catalogs and the local spawner must find it. Pinned by the `resolve_in_finds_cmd_shim` test in Task 4.

## File Structure

| Path | Responsibility |
|---|---|
| `crates/board-ipc/Cargo.toml`, `src/lib.rs` | New crate. Platform facade: `Stream`, `Listener`, `connect_timeout`, `wait_readable`, `pipe_name` |
| `crates/board-ipc/src/windows.rs` | `PipeStream`, `PipeListener`, owner check (all Win32 `unsafe`) |
| `crates/board-ipc/tests/pipe.rs` | Cross-platform transport contract tests |
| `crates/board-herdr/src/transport.rs` | Unix transport gated `cfg(unix)`; platform-neutral parts stay; `Stream` alias |
| `crates/board-herdr/src/transport_windows.rs` | Windows adapters over `board-ipc` with the same `pub(crate)` signatures |
| `crates/board-herdr/src/{client.rs,events/stream.rs}` | `UnixStream` → `transport::Stream` |
| `crates/board-core/src/client/unix.rs` | `UnixStream` → `board_ipc::Stream` |
| `crates/board-core/src/paths.rs` | `session_name_from_socket` separator fix; `open_private_file` |
| `crates/board-core/src/process.rs` | New: `resolve`, `resolve_in`, `command` |
| `crates/board-daemon/src/singleton.rs` | `File::try_lock` |
| `crates/board-daemon/src/listener.rs` | New: async `Listener` (tokio Unix / named pipe) + `Conn` |
| `crates/board-daemon/src/{lib.rs,server.rs}` | Bind via `Listener`, generic `handle_conn`, Windows console-control shutdown |
| `crates/board-daemon/src/logging.rs` | `open_private_file`, `cfg(unix)` chmod |
| `crates/board-daemon/src/spawner/herdr/{configured.rs,managed.rs}` | `.ps1` script + runner argv on Windows; `cfg(unix)` chmod |
| `crates/board-daemon/src/spawner/local.rs` | `board_core::process::command` |
| `crates/board-core/src/{pi,opencode,agy}_catalog.rs` | `board_core::process::command` |
| `crates/board-cli/src/daemon.rs` | Detached Windows child, Windows stale-socket semantics |
| `crates/board-tui/src/editor.rs` | Resolver + `notepad` default on Windows |
| `e2e/*`, `scripts/ndjson_rpc.py`, `e2e/process_identity.py`, `e2e/fake-bin/*.cmd` | Windows harness port |
| `.github/workflows/ci.yml` | `windows` + `live-e2e-windows` jobs reusing existing `run:` commands |
| Docs + `herdr-plugin.toml` + `CHANGELOG.md` | Platform facts and entry |

---

### Task 1: `board-ipc` crate — Windows named-pipe stream and listener

**Files:**
- Create: `crates/board-ipc/Cargo.toml`, `crates/board-ipc/src/lib.rs`, `crates/board-ipc/src/windows.rs`, `crates/board-ipc/tests/pipe.rs`
- Modify: `Cargo.toml` (workspace `members`, `[workspace.dependencies]`)

**Interfaces:**
- Produces (all platforms):
  - `board_ipc::Stream` — Unix: `std::os::unix::net::UnixStream`; Windows: `PipeStream`
  - `board_ipc::Listener` — Unix: `std::os::unix::net::UnixListener`; Windows: `PipeListener`
  - `board_ipc::connect_timeout(path: &Path, timeout: Duration) -> io::Result<Stream>` (Windows only; Unix keeps `board-herdr`'s own deadline connect)
- Produces (Windows):
  - `board_ipc::pipe_name(path: &Path) -> OsString`
  - `PipeStream::connect(path: &Path) -> io::Result<PipeStream>` (2 s busy wait)
  - `PipeStream::{try_clone(&self) -> io::Result<PipeStream>, set_read_timeout(&self, Option<Duration>) -> io::Result<()>, set_write_timeout(&self, Option<Duration>) -> io::Result<()>, set_nonblocking(&self, bool) -> io::Result<()>, wait_readable(&self, Option<Duration>) -> io::Result<bool>}`
  - `impl Read for PipeStream`, `impl Read for &PipeStream`, `impl Write for PipeStream`, `impl Write for &PipeStream`
  - `PipeListener::{bind(path: &Path) -> io::Result<PipeListener>, accept(&self) -> io::Result<(PipeStream, ())>, incoming(&self) -> impl Iterator<Item = io::Result<PipeStream>>}`

- [ ] **Step 1: Scaffold the crate**

`Cargo.toml` (workspace root): add `"crates/board-ipc",` as the first entry of `members`, and under `[workspace.dependencies]` add:

```toml
board-ipc = { path = "crates/board-ipc" }
windows-sys = { version = "0.59", features = [
    "Win32_Foundation",
    "Win32_Security",
    "Win32_Storage_FileSystem",
    "Win32_System_IO",
    "Win32_System_Pipes",
    "Win32_System_Threading",
] }
```

(Match the existing style of `board-core = { path = … }` entries; if those carry `version.workspace`, copy that form.)

`crates/board-ipc/Cargo.toml`:

```toml
[package]
name = "board-ipc"
version.workspace = true
edition.workspace = true
license.workspace = true

[target.'cfg(windows)'.dependencies]
windows-sys = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

`crates/board-ipc/src/lib.rs`:

```rust
//! Platform local-IPC transport: AF_UNIX streams on Unix, named pipes on Windows.
//!
//! Herdr on Windows serves its API on `\\.\pipe\` + the socket path, and boardd
//! follows the same convention, so `BOARD_SOCKET` / `HERDR_SOCKET_PATH` stay plain
//! paths on every platform. This crate knows nothing about boards or Herdr.

#[cfg(unix)]
pub use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{connect_timeout, pipe_name, PipeListener, PipeStream};
#[cfg(windows)]
pub type Stream = PipeStream;
#[cfg(windows)]
pub type Listener = PipeListener;
```

- [ ] **Step 2: Write the failing transport contract tests**

`crates/board-ipc/tests/pipe.rs`:

```rust
//! Transport contract shared by the Unix socket and the Windows named pipe.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use board_ipc::{Listener, Stream};

/// A socket path under a directory whose name contains a space and a non-ASCII
/// letter, like `C:\Users\Jan Łukasz\…`.
fn socket_path(dir: &tempfile::TempDir) -> PathBuf {
    let nested = dir.path().join("Jan Ł");
    std::fs::create_dir_all(&nested).unwrap();
    nested.join("t.sock")
}

fn echo_server(listener: Listener) {
    thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(stream) = conn else { break };
            let mut writer = stream.try_clone().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                writer.write_all(line.as_bytes()).unwrap();
                line.clear();
            }
        }
    });
}

#[test]
fn round_trips_a_line_over_a_path_with_space_and_non_ascii() {
    let dir = tempfile::tempdir().unwrap();
    let path = socket_path(&dir);
    echo_server(Listener::bind(&path).unwrap());

    let stream = Stream::connect(&path).unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    writer.write_all(b"hello\n").unwrap();
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line, "hello\n");
}

#[test]
fn connect_to_a_missing_endpoint_fails_fast() {
    let dir = tempfile::tempdir().unwrap();
    let started = Instant::now();
    assert!(Stream::connect(&dir.path().join("absent.sock")).is_err());
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[test]
fn read_timeout_surfaces_as_timed_out_or_would_block() {
    let dir = tempfile::tempdir().unwrap();
    let path = socket_path(&dir);
    let listener = Listener::bind(&path).unwrap();
    let _held = thread::spawn(move || {
        let conn = listener.incoming().next().unwrap().unwrap();
        thread::sleep(Duration::from_secs(2));
        drop(conn);
    });
    let mut stream = Stream::connect(&path).unwrap();
    stream.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
    let err = stream.read(&mut [0u8; 8]).unwrap_err();
    assert!(matches!(
        err.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    ));
}

#[test]
fn eof_after_server_drop() {
    let dir = tempfile::tempdir().unwrap();
    let path = socket_path(&dir);
    let listener = Listener::bind(&path).unwrap();
    thread::spawn(move || {
        let mut conn = listener.incoming().next().unwrap().unwrap();
        conn.write_all(b"bye\n").unwrap();
        // conn dropped here: peer closes.
    });
    let stream = Stream::connect(&path).unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    assert_eq!(reader.read_line(&mut line).unwrap(), 4);
    line.clear();
    assert_eq!(reader.read_line(&mut line).unwrap(), 0, "closed peer reads as EOF");
}

/// A reader parked waiting for data must not block a write on its clone —
/// synchronous Windows pipe handles serialize I/O per file object.
#[test]
fn pending_read_does_not_block_a_write_on_the_clone() {
    let dir = tempfile::tempdir().unwrap();
    let path = socket_path(&dir);
    echo_server(Listener::bind(&path).unwrap());

    let stream = Stream::connect(&path).unwrap();
    let mut writer = stream.try_clone().unwrap();
    let reader_thread = thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        line
    });
    thread::sleep(Duration::from_millis(200)); // reader is now waiting
    let started = Instant::now();
    writer.write_all(b"ping\n").unwrap();
    assert!(started.elapsed() < Duration::from_millis(500), "write stalled");
    assert_eq!(reader_thread.join().unwrap(), "ping\n");
}

#[cfg(windows)]
#[test]
fn second_bind_of_the_same_name_fails() {
    let dir = tempfile::tempdir().unwrap();
    let path = socket_path(&dir);
    let _first = Listener::bind(&path).unwrap();
    assert!(Listener::bind(&path).is_err());
}

#[cfg(windows)]
#[test]
fn pipe_name_prefixes_the_verbatim_path() {
    let name = board_ipc::pipe_name(std::path::Path::new(r"C:\Users\a b\herdr.sock"));
    assert_eq!(name, std::ffi::OsString::from(r"\\.\pipe\C:\Users\a b\herdr.sock"));
}

#[cfg(windows)]
#[test]
fn nonblocking_read_without_data_would_block() {
    let dir = tempfile::tempdir().unwrap();
    let path = socket_path(&dir);
    let listener = Listener::bind(&path).unwrap();
    let _held = thread::spawn(move || {
        let conn = listener.incoming().next().unwrap().unwrap();
        thread::sleep(Duration::from_secs(2));
        drop(conn);
    });
    let mut stream = Stream::connect(&path).unwrap();
    stream.set_nonblocking(true).unwrap();
    let err = stream.read(&mut [0u8; 8]).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::WouldBlock);
    assert!(!stream.wait_readable(Some(Duration::from_millis(50))).unwrap());
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p board-ipc`
Expected on Windows: compile FAIL — `could not find windows in the crate root` / `PipeStream` unresolved.

- [ ] **Step 4: Implement `crates/board-ipc/src/windows.rs`**

```rust
//! Named-pipe transport. All Win32 `unsafe` for the workspace lives here.
//!
//! Reads call `ReadFile` only after `PeekNamedPipe` reports data or EOF: I/O on
//! a synchronous pipe handle is serialized per file object, so a blocking read
//! would stall a write on a cloned handle. Timeouts and non-blocking mode are
//! emulated with a peek loop.
// ponytail: peek polling adds up to ~20 ms latency on idle streams, and a
// blocked WriteFile (server not reading, pipe buffer full) still serializes a
// concurrent peek; switch to overlapped I/O if either ever matters.

use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    ERROR_BROKEN_PIPE, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_PIPE_NOT_CONNECTED, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::{
    GetLengthSid, GetTokenInformation, TokenUser, SECURITY_IDENTIFICATION, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, PeekNamedPipe,
    WaitNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

const DEFAULT_CONNECT: Duration = Duration::from_secs(2);
const BUFFER: u32 = 64 * 1024;

/// `\\.\pipe\` + the verbatim socket path — Herdr's Windows convention.
pub fn pipe_name(path: &Path) -> OsString {
    let mut name = OsString::from(r"\\.\pipe\");
    name.push(path.as_os_str());
    name
}

fn wide(name: &OsString) -> Vec<u16> {
    name.encode_wide().chain(Some(0)).collect()
}

fn is_eof(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code) if code == ERROR_BROKEN_PIPE as i32 || code == ERROR_PIPE_NOT_CONNECTED as i32
    )
}

/// Settings shared by clones, like `SO_RCVTIMEO` / `O_NONBLOCK` on one fd.
#[derive(Default)]
struct Shared {
    /// Read timeout in ms; 0 = none.
    read_timeout_ms: AtomicU64,
    nonblocking: AtomicBool,
}

pub struct PipeStream {
    file: File,
    shared: Arc<Shared>,
}

/// Connect to the named pipe for `path`, waiting up to `timeout` while every
/// server instance is busy, then verify the server runs as the current user.
pub fn connect_timeout(path: &Path, timeout: Duration) -> io::Result<PipeStream> {
    let name = pipe_name(path);
    let deadline = Instant::now() + timeout;
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            // Identification only: the server may learn who we are but not
            // impersonate us.
            .security_qos_flags(SECURITY_IDENTIFICATION)
            .open(&name)
        {
            Ok(file) => {
                verify_same_user(&file)?;
                return Ok(PipeStream::from_file(file));
            }
            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, "named pipe busy"));
                }
                let ms = left.as_millis().clamp(1, u128::from(u32::MAX - 1)) as u32;
                // SAFETY: `wide` is a NUL-terminated UTF-16 buffer alive for the call.
                unsafe { WaitNamedPipeW(wide(&name).as_ptr(), ms) };
            }
            Err(error) => return Err(error),
        }
    }
}

impl PipeStream {
    fn from_file(file: File) -> Self {
        Self { file, shared: Arc::default() }
    }

    pub fn connect(path: &Path) -> io::Result<Self> {
        connect_timeout(path, DEFAULT_CONNECT)
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self { file: self.file.try_clone()?, shared: Arc::clone(&self.shared) })
    }

    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        let ms = match timeout {
            Some(t) if t.is_zero() => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "zero read timeout"))
            }
            Some(t) => t.as_millis().clamp(1, u128::from(u64::MAX)) as u64,
            None => 0,
        };
        self.shared.read_timeout_ms.store(ms, Ordering::Relaxed);
        Ok(())
    }

    /// Accepted for API parity with `UnixStream`; pipe writes are not bounded.
    // ponytail: writes block only when the peer stops reading and the 64 KiB
    // pipe buffer is full; bound them with overlapped I/O if a hung peer shows up.
    pub fn set_write_timeout(&self, _timeout: Option<Duration>) -> io::Result<()> {
        Ok(())
    }

    pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
        self.shared.nonblocking.store(nonblocking, Ordering::Relaxed);
        Ok(())
    }

    /// `Ok(true)` when data or EOF is available, `Ok(false)` on timeout.
    /// `None` waits forever.
    pub fn wait_readable(&self, timeout: Option<Duration>) -> io::Result<bool> {
        let deadline = timeout.map(|t| Instant::now() + t);
        let mut nap = Duration::from_millis(1);
        loop {
            if self.peek_ready()? {
                return Ok(true);
            }
            let sleep = match deadline {
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Ok(false);
                    }
                    nap.min(left)
                }
                None => nap,
            };
            std::thread::sleep(sleep);
            nap = (nap * 2).min(Duration::from_millis(20));
        }
    }

    fn peek_ready(&self) -> io::Result<bool> {
        let mut available = 0u32;
        // SAFETY: valid pipe handle owned by `self.file`; null buffer with size 0
        // asks only for the available byte count.
        let ok = unsafe {
            PeekNamedPipe(
                self.file.as_raw_handle() as HANDLE,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let error = io::Error::last_os_error();
            return if is_eof(&error) { Ok(true) } else { Err(error) };
        }
        Ok(available > 0)
    }
}

impl Read for &PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let nonblocking = self.shared.nonblocking.load(Ordering::Relaxed);
        let timeout = match (nonblocking, self.shared.read_timeout_ms.load(Ordering::Relaxed)) {
            (true, _) => Some(Duration::ZERO),
            (false, 0) => None,
            (false, ms) => Some(Duration::from_millis(ms)),
        };
        if !self.wait_readable(timeout)? {
            return Err(if nonblocking {
                io::ErrorKind::WouldBlock.into()
            } else {
                io::Error::new(io::ErrorKind::TimedOut, "named pipe read timed out")
            });
        }
        match (&self.file).read(buf) {
            Err(error) if is_eof(&error) => Ok(0),
            other => other,
        }
    }
}

impl Read for PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self).read(buf)
    }
}

impl Write for &PipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.file).write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Write for PipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self).write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Server side of a named pipe; `accept` hands out one connected instance at a
/// time and immediately creates the next one.
pub struct PipeListener {
    name: OsString,
    next: std::sync::Mutex<File>,
}

impl PipeListener {
    /// Fails if any process already serves this name (`FILE_FLAG_FIRST_PIPE_INSTANCE`).
    pub fn bind(path: &Path) -> io::Result<Self> {
        let name = pipe_name(path);
        let first = create_instance(&name, true)?;
        Ok(Self { name, next: std::sync::Mutex::new(first) })
    }

    pub fn accept(&self) -> io::Result<(PipeStream, ())> {
        let mut next = self.next.lock().map_err(|_| io::Error::other("listener poisoned"))?;
        // SAFETY: valid server pipe handle; synchronous (no OVERLAPPED).
        let ok = unsafe { ConnectNamedPipe(next.as_raw_handle() as HANDLE, std::ptr::null_mut()) };
        if ok == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32) {
                return Err(error);
            }
        }
        let connected = std::mem::replace(&mut *next, create_instance(&self.name, false)?);
        Ok((PipeStream::from_file(connected), ()))
    }

    pub fn incoming(&self) -> impl Iterator<Item = io::Result<PipeStream>> + '_ {
        std::iter::from_fn(move || Some(self.accept().map(|(stream, ())| stream)))
    }
}

fn create_instance(name: &OsString, first: bool) -> io::Result<File> {
    let open_mode = PIPE_ACCESS_DUPLEX | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
    // SAFETY: NUL-terminated name; null security attributes = default DACL
    // (write access for creator, SYSTEM, Administrators only).
    let handle = unsafe {
        CreateNamedPipeW(
            wide(name).as_ptr(),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            BUFFER,
            BUFFER,
            0,
            std::ptr::null(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fresh, valid, exclusively owned handle.
    Ok(unsafe { File::from_raw_handle(handle as RawHandle) })
}

/// Reject a pipe whose server process runs as another user (pipe squatting:
/// the `\\.\pipe\` namespace is global).
fn verify_same_user(file: &File) -> io::Result<()> {
    let mut server_pid = 0u32;
    // SAFETY: valid client pipe handle and out pointer.
    if unsafe { GetNamedPipeServerProcessId(file.as_raw_handle() as HANDLE, &mut server_pid) } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: no preconditions.
    let me = unsafe { GetCurrentProcessId() };
    if user_sid(server_pid)? != user_sid(me)? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "named pipe server runs as a different user",
        ));
    }
    Ok(())
}

fn user_sid(pid: u32) -> io::Result<Vec<u8>> {
    // SAFETY: every handle is checked, then owned by an `OwnedHandle` that
    // closes it; buffers outlive the calls that fill them.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let process = OwnedHandle::from_raw_handle(process as RawHandle);
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process.as_raw_handle() as HANDLE, TOKEN_QUERY, &mut token) == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = OwnedHandle::from_raw_handle(token as RawHandle);
        let mut len = 0u32;
        GetTokenInformation(token.as_raw_handle() as HANDLE, TokenUser, std::ptr::null_mut(), 0, &mut len);
        // u64 storage keeps TOKEN_USER's pointer field aligned.
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        if GetTokenInformation(
            token.as_raw_handle() as HANDLE,
            TokenUser,
            buf.as_mut_ptr().cast(),
            len,
            &mut len,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let sid = (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        let sid_len = GetLengthSid(sid) as usize;
        Ok(std::slice::from_raw_parts(sid.cast::<u8>(), sid_len).to_vec())
    }
}
```

If a `windows-sys` 0.59 symbol path differs from the above (e.g. a constant living in another module), fix the `use` line only; the function set is fixed.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p board-ipc`
Expected: all tests PASS on Windows. Then in WSL: `wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Projects/home/herdr-board && CARGO_TARGET_DIR=~/hb-target cargo test -p board-ipc'` — the cross-platform tests PASS on Unix too (they run against `UnixStream`/`UnixListener`, a pure local test with no Herdr).

- [ ] **Step 6: Clippy and commit**

Run: `cargo clippy -p board-ipc --all-targets -- -D warnings` → clean.

```bash
git add Cargo.toml Cargo.lock crates/board-ipc
git commit -m "feat(ipc): add board-ipc named-pipe transport for Windows"
```

---

### Task 2: `board-herdr` on `board-ipc`

**Files:**
- Modify: `crates/board-herdr/Cargo.toml`, `crates/board-herdr/src/transport.rs`, `crates/board-herdr/src/client.rs:1,21`, `crates/board-herdr/src/events/stream.rs:3,97-98`, `crates/board-herdr/src/lib.rs` (module decl)
- Create: `crates/board-herdr/src/transport_windows.rs`
- Test: `crates/board-herdr/tests/socket.rs`, `tests/subscription_ack.rs`, `tests/diagnostic_boundaries.rs` (migrate fakes), new unit test in `transport.rs`

**Interfaces:**
- Consumes: `board_ipc::{Stream, Listener, connect_timeout}`, `PipeStream::{wait_readable, set_nonblocking}` (Task 1)
- Produces (unchanged names, both platforms): `pub(crate) type Stream`; `pub(crate) fn connect_with_deadline(&Path, Duration) -> Result<Stream>`; `pub(crate) fn set_nonblocking(&Stream, bool) -> Result<()>`; `pub(crate) fn poll_read_ready(&Stream, Duration) -> Result<bool>`; `pub(crate) fn poll_read_ready_infinite(&Stream) -> Result<bool>`; `pub fn default_socket_path() -> PathBuf` (Windows: `%APPDATA%\herdr\herdr.sock`)

- [ ] **Step 1: Write the failing default-path test**

Append to `crates/board-herdr/src/transport.rs`:

```rust
#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn windows_default_socket_is_under_appdata() {
        let path = super::default_socket_path_from(None, None, Some(r"C:\Users\a b\AppData\Roaming".into()));
        assert_eq!(path, std::path::PathBuf::from(r"C:\Users\a b\AppData\Roaming\herdr\herdr.sock"));
    }

    #[test]
    fn explicit_socket_env_wins() {
        let path = super::default_socket_path_from(Some("/x/h.sock".into()), None, None);
        assert_eq!(path, std::path::PathBuf::from("/x/h.sock"));
    }
}
```

Run: `cargo test -p board-herdr --lib transport` → FAIL (`default_socket_path_from` not found; crate does not compile on Windows).

- [ ] **Step 2: Split `transport.rs` by platform**

In `crates/board-herdr/src/transport.rs`:
1. Put `#[cfg(unix)]` on `use std::os::fd::FromRawFd;`, `use std::os::unix::ffi::OsStrExt;`, `use std::os::unix::net::UnixStream;`, and on the functions `connect_with_deadline`, `set_nonblocking`, `poll_read_ready`, `poll_read_ready_infinite` (bodies untouched).
2. Add below the imports:

```rust
#[cfg(unix)]
pub(crate) type Stream = UnixStream;

#[cfg(windows)]
#[path = "transport_windows.rs"]
mod windows;
#[cfg(windows)]
pub(crate) use windows::{
    connect_with_deadline, poll_read_ready, poll_read_ready_infinite, set_nonblocking, Stream,
};
```

3. Replace `default_socket_path` with a pure core plus the env-reading wrapper (doc comment updated to mention Windows):

```rust
pub fn default_socket_path() -> PathBuf {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    default_socket_path_from(
        var("HERDR_SOCKET_PATH").or_else(|| var("HERDR_SOCKET")),
        var("HOME"),
        var("APPDATA"),
    )
}

/// Pure resolution: explicit socket, else the platform's default session path
/// (`~/.config/herdr/herdr.sock` on Unix, `%APPDATA%\herdr\herdr.sock` on Windows).
fn default_socket_path_from(
    explicit: Option<String>,
    home: Option<String>,
    appdata: Option<String>,
) -> PathBuf {
    if let Some(path) = explicit {
        return PathBuf::from(path);
    }
    if cfg!(windows) {
        if let Some(appdata) = appdata {
            return PathBuf::from(appdata).join("herdr").join("herdr.sock");
        }
    }
    PathBuf::from(home.unwrap_or_else(|| "/root".to_string())).join(".config/herdr/herdr.sock")
}
```

Remove `#[allow]`-free unused warnings: `appdata` is used under `cfg!(windows)` as a runtime `if`, so it compiles on both.

- [ ] **Step 3: Create `crates/board-herdr/src/transport_windows.rs`**

```rust
//! Windows adapters with the same signatures as the Unix transport.

use std::path::Path;
use std::time::Duration;

use crate::error::{HerdrError, Result};

pub(crate) type Stream = board_ipc::Stream;

pub(crate) fn connect_with_deadline(path: &Path, timeout: Duration) -> Result<Stream> {
    board_ipc::connect_timeout(path, timeout).map_err(|error| {
        if error.kind() == std::io::ErrorKind::TimedOut {
            HerdrError::Deadline { operation: "connect" }
        } else {
            HerdrError::Io(error)
        }
    })
}

pub(crate) fn set_nonblocking(stream: &Stream, nonblocking: bool) -> Result<()> {
    stream.set_nonblocking(nonblocking).map_err(HerdrError::from)
}

pub(crate) fn poll_read_ready(stream: &Stream, deadline: Duration) -> Result<bool> {
    stream.wait_readable(Some(deadline)).map_err(HerdrError::from)
}

pub(crate) fn poll_read_ready_infinite(stream: &Stream) -> Result<bool> {
    stream.wait_readable(None).map_err(HerdrError::from)
}
```

`crates/board-herdr/Cargo.toml`: add `board-ipc = { workspace = true }` to `[dependencies]`; move `libc` to `[target.'cfg(unix)'.dependencies]`.

- [ ] **Step 4: Switch consumers to `transport::Stream`**

- `client.rs:1` doc: "Blocking herdr socket client (`UnixStream` on Unix, a named pipe on Windows; no async)."
- `events/stream.rs`: delete `use std::os::unix::net::UnixStream;`; fields become `reader: BufReader<transport::Stream>`, `writer: transport::Stream`.
- `lib.rs` crate doc line 3: mention the Windows named pipe `\\.\pipe\%APPDATA%\herdr\herdr.sock`.

- [ ] **Step 5: Migrate the test fakes**

In `crates/board-herdr/tests/{socket.rs,subscription_ack.rs,diagnostic_boundaries.rs}` replace `use std::os::unix::net::{UnixListener, UnixStream};` with `use board_ipc::{Listener as UnixListener, Stream as UnixStream};` (keeps each file's body unchanged). Add `board-ipc = { workspace = true }` to `[dev-dependencies]` if not inherited. Any test that asserts a Unix-only fact (`ENAMETOOLONG` for > 108-byte paths, `EINVAL` read-timeout quirks) gets `#[cfg(unix)]`.

- [ ] **Step 6: Run tests**

Run: `cargo test -p board-herdr` → PASS on Windows (live tests stay `#[ignore]`d).
Run in WSL: `cargo clippy -p board-herdr --all-targets -- -D warnings` → clean.

- [ ] **Step 7: Commit**

```bash
git add crates/board-herdr Cargo.lock
git commit -m "feat(herdr): speak Herdr's Windows named pipe via board-ipc"
```

---

### Task 3: `board-core` client on `board-ipc`, session names, private file opens

**Files:**
- Modify: `crates/board-core/Cargo.toml`, `crates/board-core/src/client/unix.rs:3,100-101,107,163,237`, `crates/board-core/src/paths.rs:57-68` (+ new fn)
- Test: `crates/board-core/tests/client.rs:3`, unit tests in `paths.rs`

**Interfaces:**
- Consumes: `board_ipc::{Stream, Listener}` (Task 1)
- Produces:
  - `board_core::paths::open_private_file(options: &mut std::fs::OpenOptions, path: &Path) -> std::io::Result<std::fs::File>` — owner-only, refuses symlinks, both platforms
  - `session_name_from_socket` accepts `/` and `\`

- [ ] **Step 1: Write failing tests** (append to the existing `#[cfg(test)]` module of `paths.rs`, or create one)

```rust
#[test]
fn session_name_parses_windows_separators() {
    assert_eq!(
        session_name_from_socket(Some(r"C:\Users\a\AppData\Roaming\herdr\sessions\work\herdr.sock")),
        Some("work".to_string())
    );
    assert_eq!(
        session_name_from_socket(Some(r"C:\Users\a\AppData\Roaming\herdr\herdr.sock")),
        None
    );
}

#[test]
fn open_private_file_refuses_a_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target.log");
    std::fs::write(&target, b"").unwrap();
    let link = dir.path().join("link.log");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    if std::os::windows::fs::symlink_file(&target, &link).is_err() {
        return; // creating symlinks needs Developer Mode or admin; nothing to test
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    assert!(open_private_file(&mut options, &link).is_err());
}

#[test]
fn open_private_file_creates_a_regular_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.log");
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    open_private_file(&mut options, &path).unwrap();
    assert!(path.is_file());
}
```

Run: `cargo test -p board-core --lib paths` → FAIL (does not compile: `open_private_file` missing; on Windows also `client/unix.rs`).

- [ ] **Step 2: Implement**

`paths.rs` — replace the body of `session_name_from_socket`:

```rust
pub fn session_name_from_socket(path: Option<&str>) -> Option<String> {
    // Expect the tail `sessions/<name>/herdr.sock` with `/` or `\` separators.
    let mut parts = path?.rsplit(['/', '\\']);
    (parts.next()? == "herdr.sock").then_some(())?;
    let name = parts.next()?;
    (parts.next()? == "sessions" && !name.is_empty()).then(|| name.to_string())
}
```

Add to `paths.rs`:

```rust
/// Open a diagnostic file owner-only without following a symlink at `path`.
/// Unix: `0600` + `O_NOFOLLOW`. Windows: the profile ACL is already owner-only;
/// open the reparse point itself and refuse it if it is a symlink.
pub fn open_private_file(
    options: &mut std::fs::OpenOptions,
    path: &Path,
) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let file = options.mode(0o600).custom_flags(libc::O_NOFOLLOW).open(path)?;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        Ok(file)
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        let file = options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(path)?;
        if file.metadata()?.file_type().is_symlink() {
            return Err(std::io::Error::other("refusing to open a symlinked private file"));
        }
        Ok(file)
    }
}
```

`crates/board-core/Cargo.toml`: add `board-ipc = { workspace = true }` to `[dependencies]` and `[target.'cfg(unix)'.dependencies] libc = { workspace = true }`.

`client/unix.rs`: replace `use std::os::unix::net::UnixStream;` with `use board_ipc::Stream as UnixStream;`. Update the module doc comment ("Unix socket on Unix, named pipe on Windows"). No other body change — `connect`, `try_clone`, `set_read_timeout` exist on both types.

`tests/client.rs:3`: `use board_ipc::Listener as UnixListener;` and add `board-ipc` to `[dev-dependencies]`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p board-core` → paths + client tests PASS. Catalog tests that `chmod` fake shell executables fail to compile on Windows — gate each such test function (not the file) with `#[cfg(unix)]` in `tests/{agy_catalog,opencode_catalog,pi_catalog,scope}.rs`; a Windows sibling for the resolver is added in Task 4.

- [ ] **Step 4: Commit**

```bash
git add crates/board-core Cargo.lock
git commit -m "feat(core): boardd client over board-ipc; Windows session paths and private opens"
```

---

### Task 4: Program resolution for `.cmd` shims

**Files:**
- Create: `crates/board-core/src/process.rs`
- Modify: `crates/board-core/src/lib.rs` (`pub mod process;`), `pi_catalog.rs:156`, `opencode_catalog.rs:298`, `agy_catalog.rs:220`, `crates/board-daemon/src/spawner/local.rs:85`, `crates/board-tui/src/editor.rs:36-38,54`

**Interfaces:**
- Produces:
  - `board_core::process::resolve_in(program: &OsStr, path_var: Option<&OsStr>, pathext: Option<&str>) -> PathBuf`
  - `board_core::process::resolve(program: impl AsRef<OsStr>) -> PathBuf`
  - `board_core::process::command(program: impl AsRef<OsStr>) -> std::process::Command`

- [ ] **Step 1: Write the failing tests** — `crates/board-core/src/process.rs` bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn resolve_in_finds_cmd_shim() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pi.cmd"), "@echo off\r\n").unwrap();
        let resolved = resolve_in(
            OsStr::new("pi"),
            Some(dir.path().as_os_str()),
            Some(".COM;.EXE;.BAT;.CMD"),
        );
        assert_eq!(resolved, dir.path().join("pi.cmd"));
    }

    #[test]
    fn explicit_paths_and_extensions_pass_through() {
        assert_eq!(resolve_in(OsStr::new("pi.exe"), None, None), PathBuf::from("pi.exe"));
        let p = Path::new("dir").join("pi");
        assert_eq!(resolve_in(p.as_os_str(), None, None), p);
    }

    #[test]
    fn unknown_program_is_returned_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_in(OsStr::new("nope"), Some(dir.path().as_os_str()), Some(".CMD")),
            PathBuf::from("nope")
        );
    }
}
```

Run: `cargo test -p board-core --lib process` → FAIL (module missing).

- [ ] **Step 2: Implement** (top of `process.rs`)

```rust
//! Resolve a bare program name the way a shell would. On Windows,
//! `Command::new("pi")` only finds `pi.exe`, but npm installs `pi.cmd`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `Command::new` with Windows `PATH` × `PATHEXT` resolution.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    Command::new(resolve(program))
}

pub fn resolve(program: impl AsRef<OsStr>) -> PathBuf {
    let path_var = std::env::var_os("PATH");
    let pathext = std::env::var("PATHEXT").ok();
    resolve_in(program.as_ref(), path_var.as_deref(), pathext.as_deref())
}

/// Pure core. Unix: identity. Windows: a bare name without extension is
/// searched in each `PATH` dir with each `PATHEXT` suffix; no match (or an
/// explicit path/extension) returns the input unchanged.
pub fn resolve_in(program: &OsStr, path_var: Option<&OsStr>, pathext: Option<&str>) -> PathBuf {
    let program = Path::new(program);
    if !cfg!(windows) || program.extension().is_some() || program.components().count() > 1 {
        return program.to_path_buf();
    }
    let exts = pathext.unwrap_or(".COM;.EXE;.BAT;.CMD");
    for dir in std::env::split_paths(path_var.unwrap_or_default()) {
        for ext in exts.split(';').filter_map(|e| e.strip_prefix('.')) {
            let candidate = dir.join(program).with_extension(ext);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    program.to_path_buf()
}
```

- [ ] **Step 3: Use it at every bare-name spawn**

- `pi_catalog.rs:156`: `crate::process::command(pi_bin).arg("--list-models")`
- `opencode_catalog.rs:298` and `agy_catalog.rs:220`: `crate::process::command(&argv[0])`
- `board-daemon/src/spawner/local.rs:85`: `board_core::process::command(prog)`
- `board-tui/src/editor.rs`: default `.unwrap_or_else(|_| if cfg!(windows) { "notepad" } else { "vi" }.to_string())` and `board_core::process::command(&editor).arg(&path).status()`

- [ ] **Step 4: Run tests**

Run: `cargo test -p board-core --lib process && cargo test -p board-core && cargo test -p board-tui` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/board-core crates/board-daemon/src/spawner/local.rs crates/board-tui/src/editor.rs
git commit -m "feat(core,daemon,tui): resolve .cmd shims for provider CLIs and the editor on Windows"
```

---

### Task 5: Daemon singleton and Windows shutdown signals

**Files:**
- Modify: `crates/board-daemon/src/singleton.rs`, `crates/board-daemon/src/lib.rs:237-265`, `crates/board-daemon/Cargo.toml` (libc → unix-only)

**Interfaces:**
- Produces: `singleton::acquire(&Path) -> anyhow::Result<Option<File>>` (unchanged signature)

- [ ] **Step 1: Write the failing test** — append to `singleton.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn second_acquire_reports_held() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("board.db");
        let first = super::acquire(&db).unwrap();
        assert!(first.is_some());
        assert!(super::acquire(&db).unwrap().is_none(), "second daemon must exit quietly");
        drop(first);
        assert!(super::acquire(&db).unwrap().is_some(), "lock released on drop");
    }
}
```

Run: `cargo test -p board-daemon --lib singleton` → FAIL on Windows (`libc::flock` does not compile).

- [ ] **Step 2: Implement** — replace the `flock` block and the `AsRawFd` import:

```rust
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => Err(error.into()),
    }
```

Module doc: "an exclusive `File::try_lock` (`flock` on Unix, `LockFileEx` on Windows) on `<db>.lock`."

- [ ] **Step 3: Windows shutdown** — in `lib.rs`, put `#[cfg(unix)]` on the existing `spawn_signal_handler` and add:

```rust
#[cfg(windows)]
fn spawn_signal_handler(d: Arc<Daemon>) {
    tokio::spawn(async move {
        use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close, ctrl_shutdown};
        let (Ok(mut c), Ok(mut b), Ok(mut close), Ok(mut shutdown)) =
            (ctrl_c(), ctrl_break(), ctrl_close(), ctrl_shutdown())
        else {
            tracing::warn!(
                error_category = "signal_handler",
                "console control handler setup failed"
            );
            return;
        };
        tokio::select! {
            _ = c.recv() => tracing::info!("CTRL_C received"),
            _ = b.recv() => tracing::info!("CTRL_BREAK received"),
            _ = close.recv() => tracing::info!("CTRL_CLOSE received"),
            _ = shutdown.recv() => tracing::info!("CTRL_SHUTDOWN received"),
        }
        d.trigger_shutdown();
    });
}
```

- [ ] **Step 4: Run** `cargo test -p board-daemon --lib singleton` → PASS (the crate may still fail elsewhere on Windows until Task 6; if so run this in WSL too and continue — Task 6 makes the crate compile).

- [ ] **Step 5: Commit**

```bash
git add crates/board-daemon/src/singleton.rs crates/board-daemon/src/lib.rs crates/board-daemon/Cargo.toml
git commit -m "feat(daemon): portable singleton lock and Windows console-control shutdown"
```

---

### Task 6: boardd listener on a named pipe

**Files:**
- Create: `crates/board-daemon/src/listener.rs`
- Modify: `crates/board-daemon/src/lib.rs:208-222,280-293` (bind), `crates/board-daemon/src/server.rs:13,167-195`, `crates/board-daemon/src/server/tests.rs:120-160`, `crates/board-daemon/Cargo.toml` (`board-ipc`)

**Interfaces:**
- Consumes: `board_ipc::{pipe_name, Stream}` (Task 1)
- Produces: `pub(crate) struct listener::Listener` with `pub(crate) fn bind(path: &Path) -> io::Result<Listener>`, `#[cfg(unix)] pub(crate) fn from_unix(tokio::net::UnixListener) -> Listener`, `pub(crate) async fn accept(&mut self) -> io::Result<Conn>`; `pub(crate) type Conn`; `server::serve(d: Arc<Daemon>, listener: Listener)`

- [ ] **Step 1: Write the failing test** — in `server/tests.rs`, change `serve_on_tempdir` to bind through the new type and the blocking `Client` to `board_ipc::Stream`:

```rust
fn serve_on_tempdir(rt: &tokio::runtime::Runtime, d: Arc<Daemon>) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("b.sock");
    let guard = rt.enter();
    let listener = crate::listener::Listener::bind(&socket).unwrap();
    drop(guard);
    rt.spawn(serve(d, listener));
    (dir, socket)
}

struct Client {
    reader: std::io::BufReader<board_ipc::Stream>,
    write: board_ipc::Stream,
}
// in Client::connect: `let write = board_ipc::Stream::connect(socket).unwrap();`
```

Add a Windows-only test in the same file:

```rust
#[cfg(windows)]
#[test]
fn second_listener_on_the_same_pipe_fails() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("b.sock");
    let _first = crate::listener::Listener::bind(&socket).unwrap();
    assert!(crate::listener::Listener::bind(&socket).is_err());
}
```

Run: `cargo test -p board-daemon --lib server` → FAIL (`crate::listener` missing).

- [ ] **Step 2: Implement `listener.rs`**

```rust
//! The boardd accept surface: a tokio Unix socket, or a Windows named pipe
//! (`\\.\pipe\` + socket path) rotated one instance per client.

use std::io;
use std::path::Path;

#[cfg(unix)]
pub(crate) type Conn = tokio::net::UnixStream;
#[cfg(windows)]
pub(crate) type Conn = tokio::net::windows::named_pipe::NamedPipeServer;

pub(crate) struct Listener {
    #[cfg(unix)]
    inner: tokio::net::UnixListener,
    #[cfg(windows)]
    name: std::ffi::OsString,
    #[cfg(windows)]
    next: Conn,
}

impl Listener {
    #[cfg(unix)]
    pub(crate) fn bind(path: &Path) -> io::Result<Self> {
        Ok(Self::from_unix(tokio::net::UnixListener::bind(path)?))
    }

    #[cfg(unix)]
    pub(crate) fn from_unix(inner: tokio::net::UnixListener) -> Self {
        Self { inner }
    }

    /// `first_pipe_instance` makes a squatted or duplicate name a startup error;
    /// the default DACL grants write access to this user, SYSTEM and
    /// Administrators only — the named-pipe equivalent of a 0600 socket.
    #[cfg(windows)]
    pub(crate) fn bind(path: &Path) -> io::Result<Self> {
        use tokio::net::windows::named_pipe::ServerOptions;
        let name = board_ipc::pipe_name(path);
        let next = ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&name)?;
        Ok(Self { name, next })
    }

    pub(crate) async fn accept(&mut self) -> io::Result<Conn> {
        #[cfg(unix)]
        {
            self.inner.accept().await.map(|(stream, _)| stream)
        }
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            self.next.connect().await?;
            let fresh = ServerOptions::new().reject_remote_clients(true).create(&self.name)?;
            Ok(std::mem::replace(&mut self.next, fresh))
        }
    }
}
```

Register `mod listener;` in `lib.rs`.

- [ ] **Step 3: Generic server** — `server.rs`:
- `use tokio::net::{UnixListener, UnixStream};` → `use crate::listener::{Conn, Listener};`
- `pub async fn serve(d: Arc<Daemon>, mut listener: Listener)`; the select arm becomes `accepted = listener.accept() => match accepted { Ok(stream) => { … } … }`.
- `async fn handle_conn(d: Arc<Daemon>, stream: Conn, conn_id: u64)`, first line `let (read_half, write_half) = tokio::io::split(stream);`.

Visibility: `serve` is `pub`; `Listener` is `pub(crate)`. If `serve` is used outside the crate (grep `board_daemon::server::serve` in `crates/board-cli`), make `Listener` `pub` and re-export it from `lib.rs` instead.

- [ ] **Step 4: Bind in `lib.rs`** — replace the bind/secure block:

```rust
    #[cfg(unix)]
    let _ = std::fs::remove_file(&socket_path);
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create the boardd socket directory {parent:?}"))?;
    }
    #[cfg(unix)]
    let listener = listener::Listener::from_unix(bind_secured_socket(&socket_path, |path| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    })?);
    #[cfg(windows)]
    let listener = listener::Listener::bind(&socket_path)
        .with_context(|| format!("cannot bind the boardd pipe for {socket_path:?}"))?;
```

Gate `bind_secured_socket`, its test module, the `PermissionsExt` import, and the final `remove_file(&socket_path)` with `#[cfg(unix)]`.

- [ ] **Step 5: Migrate remaining daemon test fakes**

- `testkit.rs:19`: `use board_ipc::{Listener as UnixListener, Stream as UnixStream};`; `testkit.rs:80-81`: build paths from `std::env::temp_dir()` instead of literal `/tmp/…` (`std::env::temp_dir().join("board-test.db")`). Same for `server/tests.rs` literal `/tmp/board-server-test.db`.
- `ops/tests/mod.rs:7`, `ops/tests/discovery.rs:293,296`: `board_ipc::Listener`; tests using `symlink` (`discovery.rs:1304`) or `PermissionsExt` (`discovery.rs:1088,1361`, `validation.rs:212`, `session/tests.rs:77`, `spawner/tests/{mod.rs:340, configured.rs:176,225,432, failures.rs:288}`, `logging.rs` tests) get `#[cfg(unix)]` on the test fn.

- [ ] **Step 6: Run**

Run: `cargo test -p board-daemon` → PASS on Windows. WSL: `cargo clippy -p board-daemon --all-targets --all-features -- -D warnings` → clean.

- [ ] **Step 7: Commit**

```bash
git add crates/board-daemon Cargo.lock
git commit -m "feat(daemon): serve boardd on a Windows named pipe"
```

---

### Task 7: Daemon logging and managed prompt file

**Files:**
- Modify: `crates/board-daemon/src/logging.rs:5,95-108,230-242`, `crates/board-daemon/src/spawner/herdr/managed.rs:19,492`

**Interfaces:**
- Consumes: `board_core::paths::open_private_file` (Task 3)

- [ ] **Step 1: Write the failing test** — in `logging.rs` tests (outside the unix-gated ones):

```rust
#[test]
fn daily_file_opens_on_every_platform() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    assert!(matches!(DailyFile::open(&logs).unwrap(), DailyFile::File(_)));
}
```

Run: `cargo test -p board-daemon --lib logging` → FAIL on Windows (unix imports).

- [ ] **Step 2: Implement**
- `logging.rs:5`: `#[cfg(unix)] use std::os::unix::fs::PermissionsExt;` (drop `OpenOptionsExt`).
- `ensure_private_dir`: last line becomes
  ```rust
      #[cfg(unix)]
      fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
      Ok(())
  ```
- `DailyFile::open`:
  ```rust
          let file = board_core::paths::open_private_file(
              OpenOptions::new().create(true).append(true),
              &path,
          )?;
          Ok(Self::File(file))
  ```
- `managed.rs:492`: wrap the `set_permissions(…0o600)` statement in `#[cfg(unix)]` and gate the `PermissionsExt` import. The `%TEMP%` profile ACL covers Windows (comment one line).

- [ ] **Step 3: Run** `cargo test -p board-daemon` → PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/board-daemon/src/logging.rs crates/board-daemon/src/spawner/herdr/managed.rs
git commit -m "feat(daemon): owner-only diagnostic logs on Windows"
```

---

### Task 8: Configured-harness `.ps1` launch script

**Files:**
- Modify: `crates/board-daemon/src/spawner/herdr/configured.rs:6,59-90,121-134`
- Test: `crates/board-daemon/src/spawner/tests/configured.rs`

**Interfaces:**
- Produces (Windows): `pub(crate) fn configured_script(path: &Path, argv: &[String]) -> String` (PowerShell), `pub(crate) fn ps_quote(&str) -> String`, `fn runner_argv(pane_id: &str, script: &Path) -> Vec<String>`; Unix `configured_script`/`posix_quote` unchanged.

- [ ] **Step 1: Write failing tests** — `spawner/tests/configured.rs`:

```rust
#[cfg(windows)]
#[test]
fn ps_quote_doubles_ascii_and_smart_single_quotes() {
    use crate::spawner::herdr::configured::ps_quote;
    assert_eq!(ps_quote("it's"), "'it''s'");
    assert_eq!(ps_quote("a\u{2019}b"), "'a\u{2019}\u{2019}b'");
    assert_eq!(ps_quote("$env:X `n"), "'$env:X `n'");
}

#[cfg(windows)]
#[test]
fn windows_script_removes_itself_runs_argv_and_reports_exit() {
    use crate::spawner::herdr::configured::configured_script;
    let script = configured_script(
        std::path::Path::new(r"C:\Users\Jan K\AppData\Local\Temp\herdr-board-run-1.ps1"),
        &["my tool".to_string(), "--flag=it's".to_string()],
    );
    assert_eq!(
        script,
        "Remove-Item -LiteralPath 'C:\\Users\\Jan K\\AppData\\Local\\Temp\\herdr-board-run-1.ps1' -Force -ErrorAction SilentlyContinue\n\
         $childStatus = 1\n\
         try { & 'my tool' '--flag=it''s'; $childStatus = $LASTEXITCODE } catch { Write-Error $_ }\n\
         if ($null -eq $childStatus) { $childStatus = 0 }\n\
         if ($env:BOARD_BIN) { & $env:BOARD_BIN __pane-exited --run-id $env:BOARD_RUN_ID }\n\
         exit $childStatus\n"
    );
}

#[cfg(windows)]
#[test]
fn windows_runner_argv_invokes_powershell_file_with_a_quoted_path() {
    use crate::spawner::herdr::configured::runner_argv;
    let argv = runner_argv("w1:p2", std::path::Path::new(r"C:\Users\Jan K\x.ps1"));
    assert_eq!(&argv[..3], ["pane", "run", "w1:p2"]);
    assert!(argv[3].ends_with(r#" -NoProfile -ExecutionPolicy Bypass -File "C:\Users\Jan K\x.ps1""#));
    assert!(argv[3].starts_with("pwsh") || argv[3].starts_with("powershell.exe"));
}
```

Also gate the existing Unix script-content tests in that file with `#[cfg(unix)]`, and make the existing runner-argv test assert via `runner_argv` so it covers Unix (`["pane","run",pane,script]`).

Run: `cargo test -p board-daemon --lib configured` → FAIL.

- [ ] **Step 2: Implement** in `configured.rs`:

```rust
#[cfg(unix)]
const SCRIPT_SUFFIX: &str = "";
#[cfg(windows)]
const SCRIPT_SUFFIX: &str = ".ps1";

#[cfg(unix)]
pub(crate) fn runner_argv(pane_id: &str, script: &Path) -> Vec<String> {
    vec!["pane".into(), "run".into(), pane_id.into(), script.to_string_lossy().into_owned()]
}

/// One command line that works whatever the pane shell is (pwsh, Windows
/// PowerShell or cmd): a double-quoted path survives all three.
#[cfg(windows)]
pub(crate) fn runner_argv(pane_id: &str, script: &Path) -> Vec<String> {
    let shell = if board_core::process::resolve("pwsh").extension().is_some() {
        "pwsh"
    } else {
        "powershell.exe"
    };
    let command = format!(
        "{shell} -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        script.display()
    );
    vec!["pane".into(), "run".into(), pane_id.into(), command]
}

#[cfg(windows)]
pub(crate) fn configured_script(path: &Path, argv: &[String]) -> String {
    let quoted: Vec<String> = argv.iter().map(|arg| ps_quote(arg)).collect();
    format!(
        "Remove-Item -LiteralPath {} -Force -ErrorAction SilentlyContinue\n\
         $childStatus = 1\n\
         try {{ & {}; $childStatus = $LASTEXITCODE }} catch {{ Write-Error $_ }}\n\
         if ($null -eq $childStatus) {{ $childStatus = 0 }}\n\
         if ($env:BOARD_BIN) {{ & $env:BOARD_BIN __pane-exited --run-id $env:BOARD_RUN_ID }}\n\
         exit $childStatus\n",
        ps_quote(&path.to_string_lossy()),
        quoted.join(" ")
    )
}

/// PowerShell single-quoted literal. PowerShell also treats U+2018..U+201B as
/// single quotes, so each is doubled like `'`.
// ponytail: Windows PowerShell 5.1 still mangles native args containing `"`;
// pwsh >= 7.3 passes them intact, which is why pwsh is preferred.
#[cfg(windows)]
pub(crate) fn ps_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if matches!(ch, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(ch);
        }
        out.push(ch);
    }
    out.push('\'');
    out
}
```

Gate the existing `configured_script` and `posix_quote` with `#[cfg(unix)]`. In `launch_configured`: add `.suffix(SCRIPT_SUFFIX)` to the `tempfile::Builder`; wrap the `set_permissions(0o700)` statement and the `PermissionsExt` import in `#[cfg(unix)]`; replace the inline `runner_argv` vec with `let runner_argv = runner_argv(pane_id, &script_path);`.

- [ ] **Step 3: Run** `cargo test -p board-daemon` → PASS.

- [ ] **Step 4: Commit**

```bash
git add crates/board-daemon/src/spawner
git commit -m "feat(daemon): PowerShell launch script for configured harnesses on Windows"
```

---

### Task 9: CLI auto-start and stop on Windows

**Files:**
- Modify: `crates/board-cli/src/daemon.rs`, `crates/board-cli/src/daemon/tests.rs`, `crates/board-cli/Cargo.toml` (libc unix-only), `crates/board-cli/tests/integration/{support.rs,comments.rs,events.rs,exit_codes.rs,harness.rs,meta.rs}`

**Interfaces:**
- Consumes: `board_core::paths::open_private_file` (Task 3), `board_ipc::{Listener, Stream}` (Task 1)
- Produces: unchanged `connect_or_start`, `stop_daemon`, `daemon_command`

- [ ] **Step 1: Write the failing test** — `daemon/tests.rs`:

```rust
#[cfg(windows)]
#[test]
fn stop_reports_not_running_when_no_pipe_exists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("boardd.sock");
    assert!(matches!(
        super::check_listener_after_connect_failure(&path, super::file_identity(&path)),
        super::ListenerCheck::Gone
    ));
}
```

Gate `daemon_child_owns_a_distinct_process_group` and the symlink/permission tests with `#[cfg(unix)]`; the imports at `tests.rs:3` likewise.

Run: `cargo test -p board-cli --lib daemon` → FAIL.

- [ ] **Step 2: Implement** in `daemon.rs`:
- Imports: `#[cfg(unix)] use std::os::unix::fs::{MetadataExt, PermissionsExt};`, `#[cfg(unix)] use std::os::unix::process::CommandExt;`.
- `spawn_daemon`: `#[cfg(unix)]` on the `set_permissions(parent, 0o700)` line.
- `daemon_command`:
  ```rust
      let err = board_core::paths::open_private_file(
          OpenOptions::new().create(true).write(true).truncate(true),
          bootstrap_path,
      )?;
      let mut cmd = Command::new(exe);
      cmd.arg("daemon")
          .stdin(std::process::Stdio::null())
          .stdout(std::process::Stdio::null())
          .stderr(err);
      // One child, one owned process group (see above).
      #[cfg(unix)]
      cmd.process_group(0);
      // Windows: one detached child in its own group, with no console to
      // inherit, so closing the launching pane does not end boardd.
      #[cfg(windows)]
      {
          use std::os::windows::process::CommandExt;
          const DETACHED_PROCESS: u32 = 0x0000_0008;
          const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
          cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
      }
      Ok(cmd)
  ```
- Gate `FileIdentity`, `SOCKET_FILE_TYPE`, `file_identity`, and the existing `check_listener_after_connect_failure` with `#[cfg(unix)]`; add:
  ```rust
  /// A named pipe has no filesystem entry and vanishes with its server, so a
  /// failed connect already means the listener is gone.
  #[cfg(windows)]
  type FileIdentity = ();
  #[cfg(windows)]
  fn file_identity(_path: &Path) -> Option<FileIdentity> {
      None
  }
  #[cfg(windows)]
  fn check_listener_after_connect_failure(path: &Path, _original: Option<FileIdentity>) -> ListenerCheck {
      if UnixClient::connect(path).is_ok() {
          ListenerCheck::Live
      } else {
          ListenerCheck::Gone
      }
  }
  ```
- `Cargo.toml`: `libc` → `[target.'cfg(unix)'.dependencies]`.

- [ ] **Step 3: Migrate integration tests** — in `tests/integration/*.rs` replace `std::os::unix::net::{UnixListener, UnixStream}` imports with `board_ipc::{Listener as UnixListener, Stream as UnixStream}` (add `board-ipc` to `[dev-dependencies]`); gate tests that assert socket-file identity, permissions, or symlinks with `#[cfg(unix)]`. `support.rs` `TestDaemon` keeps using `BOARD_SOCKET` paths unchanged.

- [ ] **Step 4: Run the whole workspace**

Run: `cargo build --workspace && cargo test --workspace --all-features` → PASS on Windows.
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings` → clean on Windows.
Run in WSL (compile + lint only, no tests against any Herdr): `wsl -d Ubuntu -- bash -lc 'cd /mnt/c/Projects/home/herdr-board && CARGO_TARGET_DIR=~/hb-target cargo clippy --workspace --all-targets --all-features -- -D warnings && cargo fmt --all --check'` → clean.

- [ ] **Step 5: Commit**

```bash
git add crates/board-cli Cargo.lock
git commit -m "feat(cli): detached boardd auto-start and pipe-aware stop on Windows"
```

---

### Task 10: Windows CI job (build + unit/integration)

**Files:**
- Modify: `.github/workflows/ci.yml`, `docs/README.md:63-68`

- [ ] **Step 1: Add the job** after `test:` in `ci.yml` — it reuses the existing `run:` commands so `test_docs.py`'s set equality still holds:

```yaml
  windows:
    name: windows
    runs-on: windows-latest
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v4
        with:
          persist-credentials: false

      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy

      - uses: Swatinem/rust-cache@v2

      - name: clippy
        run: cargo clippy --workspace --all-targets --all-features -- -D warnings

      - name: cargo test
        run: cargo test --workspace --all-features
```

- [ ] **Step 2: Docs** — `docs/README.md` paragraph under the gate block: add "`windows` runs the clippy and cargo test commands on `windows-latest`, and `live-e2e-windows` runs `bash e2e/ci.sh` there under Git Bash against the SHA-verified Windows Herdr 0.9.0 build."

- [ ] **Step 3: Verify locally**

Run: `python -m unittest discover -s scripts/tests -p 'test_docs.py'` → PASS.

- [ ] **Step 4: Commit and push** (first push of the branch; CI Linux jobs are the Unix regression gate)

```bash
git add .github/workflows/ci.yml docs/README.md
git commit -m "ci: build and test on windows-latest"
git push -u origin feat/windows-support
```

Check: `gh run watch` on the branch run → `fmt`, `clippy`, `test`, `windows` green. Fix any Linux regression before continuing.

---

### Task 11: Manual verification against the local Windows Herdr (needs the user's go-ahead)

**Files:** none (findings go into Task 14's docs).

- [ ] **Step 1: Ask the user** for explicit approval to create one disposable workspace in their running Herdr for this check. Do nothing below without it.
- [ ] **Step 2: Read-only probe**: `cargo build --release`; `BOARD_SOCKET=$env:TEMP\hb-manual\boardd.sock BOARD_DB=$env:TEMP\hb-manual\board.db target\release\board.exe space list --json` (auto-starts an isolated boardd; lists spaces via Herdr's pipe). Expected: JSON listing the user's workspaces; `board daemon stop` then reports `boardd stopped`.
- [ ] **Step 3: Job-object check**: from a Herdr pane, start boardd via the CLI, close that pane, and confirm boardd is still listening (`board daemon status` or a second `space list`). If it died: add `const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;` to the Task 9 creation flags, re-test, commit `fix(cli): break boardd away from the pane's job object`.
- [ ] **Step 4: `HERDR MUTATION:` dispatch in a disposable workspace**: create workspace `hb-manual-<random>`, a board with a configured harness running `cmd /c echo ok`, dispatch one card; confirm `herdr pane run` executed the `.ps1` (pane shows `ok`, script file removed, card reaches its done/exit column via `__pane-exited`). Also confirm how `herdr pane run` joins multiple COMMAND args; if it passes argv verbatim rather than typing a line, adjust `runner_argv` (Task 8) to pass `[shell, "-NoProfile", …, script]` as separate elements and update its test.
- [ ] **Step 5: Managed agent**: dispatch one card with a managed harness whose CLI is an npm `.cmd` shim, if one is installed (`Get-Command pi, claude, codex -ErrorAction SilentlyContinue`). Record whether `agent.start` launches it.
- [ ] **Step 6: Clean up** the disposable workspace (`HERDR MUTATION: workspace close …`), stop the isolated boardd, delete `%TEMP%\hb-manual`. Write findings into a scratch note for Task 14.

---

### Task 12: e2e Python helpers on Windows

**Files:**
- Modify: `scripts/ndjson_rpc.py:50`, `e2e/process_identity.py:20,48-60`, `scripts/tests/test_e2e_process_identity.py`
- Create: `e2e/fake-bin/{agy,claude,codex,opencode,pi}.cmd`

**Interfaces:**
- Produces: `process_identity.snapshot(pid)` / `process_exists(pid)` working with **Windows PIDs** on `PLATFORM == "windows"`; `ndjson_rpc.request_line` over a named pipe on Windows.

- [ ] **Step 1: Failing tests** — add to `scripts/tests/test_e2e_process_identity.py`:

```python
@unittest.skipUnless(sys.platform == "win32", "Windows backend")
class WindowsSnapshotTest(unittest.TestCase):
    def test_snapshot_of_a_child_reads_exe_cmdline_and_environ(self) -> None:
        env = dict(os.environ, HB_PROBE_TOKEN="tok-123")
        child = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(30)"], env=env
        )
        try:
            snap = process_identity.snapshot(child.pid)
            self.assertEqual(os.path.normcase(snap.exe), os.path.normcase(sys.executable))
            self.assertEqual(snap.cmdline[1:], ["-c", "import time; time.sleep(30)"])
            self.assertIn(b"HB_PROBE_TOKEN=tok-123", snap.environ)
            self.assertEqual(snap.parent_pid, str(os.getpid()))
            self.assertTrue(process_identity.process_exists(child.pid))
        finally:
            child.kill()
            child.wait()
        self.assertFalse(process_identity.process_exists(child.pid))
```

Run: `python -m unittest discover -s scripts/tests -p 'test_e2e_process_identity.py'` → FAIL (`unsupported E2E platform`).

- [ ] **Step 2: Windows backend** in `e2e/process_identity.py`:
- `PLATFORM = {"Linux": "linux", "Darwin": "darwin", "Windows": "windows"}.get(SYSTEM, "unsupported")`
- Add `_windows_snapshot(pid)` using `ctypes.WinDLL("kernel32")`/`ntdll`:
  - `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ = 0x1000 | 0x0010, False, pid)`; failure → `IdentityError`.
  - `IsWow64Process` → a 32-bit target raises `IdentityError` (fail closed; only x64 layouts below are known).
  - `start_time`: `GetProcessTimes` creation `FILETIME` as `str((high << 32) | low)`.
  - `exe`: `QueryFullProcessImageNameW`.
  - `state`: `GetExitCodeProcess` → `"R"` if `STILL_ACTIVE (259)` else `"Z"`.
  - `parent_pid`: `NtQueryInformationProcess(h, ProcessBasicInformation=0, PROCESS_BASIC_INFORMATION)` → `InheritedFromUniqueProcessId`; `PebBaseAddress` from the same struct.
  - x64 offsets: `PEB+0x20` → `ProcessParameters`; `RTL_USER_PROCESS_PARAMETERS+0x70` → `CommandLine` (`UNICODE_STRING`: `Length` u16 at +0, `Buffer` ptr at +8); `+0x80` → `Environment` ptr; `+0x3F0` → `EnvironmentSize` (u64). Read each with `ReadProcessMemory`.
  - `cmdline`: `CommandLineToArgvW(command_line)` → `list[str]`.
  - `environ`: decode the environment block UTF-16LE, split on `"\0"`, drop empties and `=`-prefixed drive entries, `frozenset(s.encode("utf-8") for s in …)`.
  - `CloseHandle` in `finally`.
- `snapshot`: dispatch `"windows"` → `_windows_snapshot`.
- `process_exists` on Windows: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` + `GetExitCodeProcess == 259`.

- [ ] **Step 3: Pipe transport** — `scripts/ndjson_rpc.py` `request_line`:

```python
    if sys.platform == "win32":
        # Herdr and boardd serve `\\.\pipe\` + the socket path on Windows.
        with open("\\\\.\\pipe\\" + path, "r+b", buffering=0) as pipe:
            pipe.write(line.encode("utf-8"))
            chunks = b""
            while not chunks.endswith(b"\n"):
                chunk = pipe.read(65536)
                if not chunk:
                    break
                chunks += chunk
            return chunks.decode("utf-8")
```

(keep the existing AF_UNIX branch for other platforms; match its return type exactly — if it returns the stripped line, strip here too.)

- [ ] **Step 4: Fake agent shims** — for each of `agy claude codex opencode pi`, create `e2e/fake-bin/<name>.cmd`:

```bat
@python "%~dp0<name>" %*
```

(literal file names, e.g. `pi.cmd` contains `@python "%~dp0pi" %*`). Mark them in `.gitattributes` with `e2e/fake-bin/*.cmd text eol=crlf`.

- [ ] **Step 5: Run** `python -m unittest discover -s scripts/tests -p 'test_e2e_*.py'` → PASS on Windows; the same command in WSL → PASS (Linux backend untouched).

- [ ] **Step 6: Commit**

```bash
git add e2e/process_identity.py scripts/ndjson_rpc.py scripts/tests e2e/fake-bin .gitattributes
git commit -m "test(e2e): Windows process identity, pipe RPC and fake agent shims"
```

---

### Task 13: e2e shell harness on Git Bash + `live-e2e-windows` CI job

**Files:**
- Modify: `e2e/ci.sh:12-40`, `e2e/lib.sh` (platform branches), `e2e/run-all.sh`, `e2e/test-harness.sh`, `e2e/herdr-proxy.py`, scenario files as triage requires, `.github/workflows/ci.yml`

- [ ] **Step 1: Pinned Windows Herdr in `ci.sh`** — branch on `uname -s`:

```bash
case "$(uname -s)" in
  MINGW*|MSYS*)
    HERDR_ASSET=herdr-windows-x86_64.zip
    HERDR_SHA256=b4508c445de1c1a68c760a01735da2aba2fa214b2aafd4b07f732e49b2a64b11
    HERDR_EXE=herdr.exe ;;
  *)
    HERDR_ASSET=herdr-linux-x86_64
    HERDR_SHA256=4fa1a01158dd8043da92d31b270780b0dcc10603038d9b61cac4d81ab63fb71f
    HERDR_EXE=herdr ;;
esac
HERDR_URL=https://github.com/herdrdev/herdr/releases/download/v$HERDR_VERSION/$HERDR_ASSET
```

The zip hash above was computed from the v0.9.0 release asset on 2026-09-28. On Windows, verify the zip's SHA, then `unzip -o` into `$CACHE_DIR`; `HERDR_BIN="$CACHE_DIR/$HERDR_EXE"`; the cached check verifies a sibling `herdr.zip.sha256` marker written after a verified extract. `CACHE_DIR` suffix becomes `…-$HERDR_ASSET`. Update `scripts/tests/test_e2e_ci.py` for the new variables in the same step (red first: add a test asserting the Windows asset name and a 64-hex SHA are pinned).

- [ ] **Step 2: PID bridge** — Git Bash `$!` is an MSYS PID; the Python identity layer needs Windows PIDs. Add to `lib.sh`:

```bash
# e2e_os_pid <pid> — the OS PID for a bash `$!` (MSYS PID → Windows PID on Git Bash).
e2e_os_pid() {
  if [ -r "/proc/$1/winpid" ]; then cat "/proc/$1/winpid"; else printf '%s\n' "$1"; fi
}
```

and route every PID handed to `process_identity.py` through it (`e2e_provisional_child_capture`, `e2e_process_exists`, and the ledger writers). Paths handed to `board.exe`/`herdr.exe`/Windows Python go through `cygpath -w` when `command -v cygpath` exists.

- [ ] **Step 3: Temp and permissions** — `mktemp -d /tmp/hb-e2e.XXXXXX` stays (Git Bash maps `/tmp`); keep `chmod 600` (a no-op on NTFS under Git Bash is acceptable because the directory is under the runner's profile). `umask 077` unchanged.

- [ ] **Step 4: `herdr-proxy.py`** — on Windows serve the proxy on a named pipe with `asyncio.get_event_loop().start_serving_pipe` (ProactorEventLoop), forwarding to Herdr's pipe; keep the AF_UNIX path elsewhere.

- [ ] **Step 5: Managed-shell isolation** — the Unix harness isolates pane shells via `ZDOTDIR`. On Windows the pane shell comes from Herdr's `[terminal] default_shell`; boot each ephemeral session with an isolated Herdr config (the session's own `config.toml` written under the scenario root with `default_shell` set to `cmd.exe`, plus whatever env var/flag `herdr --help` / `herdr api schema` show selects a config dir). Verify the mechanism with `herdr --help` before coding it. **Stop-and-report trigger:** if Windows Herdr offers no way to give an ephemeral session its own config or shell, stop here and bring options to the user.

- [ ] **Step 6: Triage loop** — on the CI runner only (not on the user's host): add the job, push, read failures, fix, repeat until `--require-all` passes:

```yaml
  live-e2e-windows:
    name: live-e2e-windows
    needs: [windows, e2e-safety]
    runs-on: windows-latest
    permissions:
      contents: read
    defaults:
      run:
        shell: bash
    steps:
      - uses: actions/checkout@v4
        with:
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: actions/cache@v4
        with:
          path: ${{ runner.tool_cache }}/herdr-board-0.9.0-herdr-windows-x86_64.zip
          key: herdr-${{ runner.os }}-x86_64-0.9.0-b4508c445de1c1a68c760a01735da2aba2fa214b2aafd4b07f732e49b2a64b11
      - name: live e2e
        env:
          HERDR_CACHE_DIR: ${{ runner.tool_cache }}/herdr-board-0.9.0-herdr-windows-x86_64.zip
        run: bash e2e/ci.sh
      - if: always()
        uses: actions/upload-artifact@v4
        with:
          name: live-e2e-windows-${{ github.run_id }}-${{ github.run_attempt }}
          path: e2e-artifacts
```

(Mirror the Linux `live-e2e` job's artifact step and any `timeout-minutes` it has.) Each fix is its own commit: `test(e2e): <scenario or lib area> on Windows`. Isolation rules from `docs/testing.md` stay intact — no fix may relax owner-token, ledger, or ephemeral-session checks; a scenario that cannot keep them on Windows is a stop-and-report.

- [ ] **Step 7: `e2e/test-harness.sh` on Git Bash** — run it in the `live-e2e-windows` job before `ci.sh` via the existing `bash e2e/test-harness.sh` step line (same command string as the Linux job, so the docs gate set stays equal). Fix platform branches it trips on.

- [ ] **Step 8: Verify** — CI run green: `fmt, clippy, docs, scripts, e2e-safety, test, live-e2e, windows, live-e2e-windows`. `python -m unittest discover -s scripts/tests -p 'test_docs.py'` → PASS.

---

### Task 14: Docs, plugin manifest, CHANGELOG, PR

**Files:**
- Modify: `README.md:6,141`, `docs/install.md:8`, `docs/herdr.md`, `docs/testing.md`, `e2e/README.md:85,178`, `docs/operations.md:34,108`, `docs/sandbox.md`, `herdr-plugin.toml:16`, `CHANGELOG.md`, `AGENTS.md` (herdr gotchas: Windows pipe bullet; sandbox-first note: Windows host exception)

- [ ] **Step 1: Content**
- README badge: `platforms-linux%2C%20macOS%2C%20Windows`; install line: "Linux, macOS, and Windows (native Herdr ≥ 0.9.0, protocol 22) are supported."
- `docs/herdr.md`: new "Windows" subsection — pipe naming `\\.\pipe\<socket path>`, marker file, `%APPDATA%\herdr\herdr.sock`, sessions under `…\sessions\<name>\herdr.sock`, `pane run` behavior and managed-agent findings from Task 11.
- `docs/operations.md`: Windows data dir `%APPDATA%\herdr-board`, logs `%APPDATA%\herdr-board\logs`, boardd pipe `\\.\pipe\%APPDATA%\herdr-board\boardd.sock`.
- `docs/testing.md` + `e2e/README.md`: Windows isolation (Windows PIDs via `/proc/<pid>/winpid`, PEB-based environ owner-token check, per-session Herdr config), Git Bash requirement.
- `docs/sandbox.md`: sandbox stays Linux/macOS; Windows uses host `cargo test` + CI `windows`/`live-e2e-windows`.
- `herdr-plugin.toml`: `platforms = ["linux", "macos", "windows"]`.
- `CHANGELOG.md` → `## Unreleased` → `### Added`: `- [#NN](https://github.com/nelsonPires5/herdr-board/pull/NN) feat: Run board natively on Windows with a Windows Herdr, including CI-verified live e2e.` (fill `NN` after the PR opens).

- [ ] **Step 2: Gates** — `python -m unittest discover -s scripts/tests -p 'test_*.py'` → PASS (docs version matrix, CHANGELOG format, install refs).

- [ ] **Step 3: Commit, open PR, fill the link**

```bash
git add README.md docs AGENTS.md e2e/README.md herdr-plugin.toml CHANGELOG.md
git commit -m "docs: document native Windows support"
git push
gh pr create --base dev --title "feat: native Windows support" --body-file <scratchpad>/pr-body.md
```

PR body (no tooling footer): summary per crate, the host-execution exception, the Task 11 manual findings, the ponytail ceilings (peek polling latency, unbounded pipe writes, PS 5.1 `"` args). Then replace `#NN` in `CHANGELOG.md` with the PR number, commit `docs: link the Windows support changelog entry`, push, and confirm CI green on the PR.
