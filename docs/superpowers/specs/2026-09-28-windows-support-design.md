# Native Windows support — design

Status: approved in brainstorming, 2026-09-28. Target: one PR `feat/windows-support` → `dev`.

## Goal

`board` (TUI + daemon + CLI) builds, runs, and is CI-verified on native Windows against a native
Windows Herdr (0.9.x, protocol 22), with Linux/macOS behavior unchanged. Scope includes the Rust
port, a Windows unit/integration CI job, the live e2e suite (scenarios 01–39) on a Windows runner,
and docs/CHANGELOG.

## Facts this design rests on (verified)

- Herdr 0.9.1 on Windows speaks protocol 22 and serves its API on a **named pipe** whose name is
  `\\.\pipe\` + the socket path, e.g.
  `\\.\pipe\C:\Users\<u>\AppData\Roaming\herdr\herdr.sock`. The on-disk `herdr.sock` is a small
  marker file (`<pid>:<token>`), not a socket.
- Default Windows Herdr socket path: `%APPDATA%\herdr\herdr.sock` (`herdr session list`).
- Pane shell is configurable (`[terminal] default_shell`); it may be pwsh, Windows PowerShell or cmd.
- Herdr v0.9.0 publishes `herdr-windows-x86_64.zip`.
- `board-core` and `board-herdr` are independent leaf crates; `windows-sys` is already in
  `Cargo.lock` transitively. Toolchain is 1.98 (`File::try_lock` is stable since 1.89).

## Non-goals

- The Docker sandbox stays Linux/macOS-only (documented).
- No change to wire protocols, schema, or Unix code paths beyond `cfg` gating (the one deliberate
  exception is the singleton, see below).

## 1. IPC transport — new leaf crate `board-ipc`

No board or Herdr semantics. Exposes:

- `pipe_name(path) -> OsString` (Windows): `\\.\pipe\` + path. `BOARD_SOCKET` / `HERDR_SOCKET_PATH`
  stay plain paths on every platform.
- `Stream`: on Unix a re-export of `std::os::unix::net::UnixStream`; on Windows `PipeStream`.
- `PipeStream` (Windows):
  - `connect(path, timeout)`: `CreateFileW` on the pipe name; on `ERROR_PIPE_BUSY` loop on
    `WaitNamedPipeW` until the deadline (same contract as Unix `connect_with_deadline`).
  - **Owner check** after connect: `GetNamedPipeServerProcessId` → `OpenProcess` →
    `OpenProcessToken` → compare the token user SID with the current user's SID; mismatch is a
    connect error. Defends against pipe-name squatting (the `\\.\pipe\` namespace is global; Unix
    relies on a 0700 directory instead). Applies to both boardd and Herdr pipes.
  - `Read`/`Write`, `try_clone` (`DuplicateHandle` via `File::try_clone`).
  - Reads call `ReadFile` only after `PeekNamedPipe` reports data or EOF (`ERROR_BROKEN_PIPE` =
    EOF). Reason: I/O on a synchronous pipe handle is serialized per file object, so a blocking
    read would stall a write on the cloned handle.
  - `set_read_timeout`, `set_nonblocking`, `poll_read_ready(deadline)`, `poll_read_ready_infinite`
    are emulated with a peek loop and 1→20 ms sleep backoff. Marked `ponytail:` — ~20 ms latency
    ceiling on long event streams; upgrade path is overlapped I/O.
- `board-herdr/src/transport.rs` stays byte-identical under `#[cfg(unix)]`; a sibling
  `transport_windows.rs` adapts `board-ipc`. `events/stream.rs` and `client.rs` use `board_ipc::Stream`.
- `board-core/src/client/unix.rs` (`UnixClient`) uses `board_ipc::Stream`; public API unchanged.
- Herdr default socket path on Windows: `%APPDATA%\herdr\herdr.sock`.

## 2. boardd server and lifecycle

- **Listener** (Windows): `tokio::net::windows::named_pipe::ServerOptions` with
  `first_pipe_instance(true)` (a squatted or duplicate name fails startup) and
  `reject_remote_clients(true)`. Accept loop creates the next instance before handing off the
  connected one. `server.rs::handle_conn` becomes generic over `AsyncRead + AsyncWrite + Unpin`.
- **Access control**: no custom DACL. The default named-pipe DACL grants write only to the creator,
  SYSTEM and Administrators (Everyone: read only), which is the owner-only equivalent of the Unix
  `0600` socket.
- **Singleton**: replace the `libc::flock` block with `File::try_lock()` on both platforms
  (`flock` on Unix, `LockFileEx` on Windows) — one code path, one `unsafe` block fewer.
  `TryLockError::WouldBlock` → `Ok(None)`.
- **Shutdown signals**: Unix unchanged. Windows: `tokio::signal::windows::{ctrl_c, ctrl_break,
  ctrl_close, ctrl_shutdown}` → `trigger_shutdown()`. `daemon.stop` RPC remains the primary path.
- **Auto-start** (`board-cli/src/daemon.rs`): Windows uses
  `creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS)` in place of `process_group(0)` —
  still exactly one child with a known PID, no console. Risk to verify: if Herdr places pane
  processes in a Job Object with kill-on-close, add `CREATE_BREAKAWAY_FROM_JOB`.
- **Stop / stale socket**: a named pipe has no filesystem entry and disappears with its server, so
  there is no stale socket on Windows. `file_identity` and unlink logic become `cfg(unix)`; on
  Windows a failed connect means `ListenerCheck::Gone`. Daemon `remove_file(socket)` at start/end
  is `cfg(unix)`.

## 3. Spawner, files, paths

- **Configured harness script** (`spawner/herdr/configured.rs`): Unix unchanged. Windows writes a
  `.ps1` temp file:
  ```powershell
  Remove-Item -LiteralPath '<script>' -Force -ErrorAction SilentlyContinue
  & 'arg0' 'arg1' ...
  $childStatus = $LASTEXITCODE
  if ($env:BOARD_BIN) { & $env:BOARD_BIN __pane-exited --run-id $env:BOARD_RUN_ID }
  exit $childStatus
  ```
  Quoting: single quotes, embedded `'` doubled. `herdr pane run` receives
  `pwsh -NoProfile -ExecutionPolicy Bypass -File <script>` (fallback `powershell.exe` when `pwsh`
  is not on `PATH`), which works whatever the pane shell is. `ponytail:` Windows PowerShell 5.1
  mangles native arguments containing `"`; pwsh ≥ 7.3 is correct.
- **Permissions**: every `set_permissions(0o600/0o700)` and `OpenOptionsExt::mode` is `cfg(unix)`.
  On Windows temp files (`%TEMP%`), logs and DB (`%APPDATA%\herdr-board`) live in the user profile
  whose inherited ACL is owner + SYSTEM + Administrators.
- **No-follow opens** (daily logs, `bootstrap.log`): Windows opens with
  `FILE_FLAG_OPEN_REPARSE_POINT` and rejects the handle if its metadata is a symlink — fail-closed
  like `O_NOFOLLOW`.
- **`paths::session_name_from_socket`** accepts both `/` and `\` separators
  (`…\sessions\<name>\herdr.sock`).
- **Owner-only file opens** are centralized in `board_core::paths::open_private_file`
  (Unix: `mode(0o600)` + `O_NOFOLLOW` + `set_permissions`; Windows: reparse-point check), used by
  the daemon's daily log and the CLI's `bootstrap.log`.
- **Program resolution** (added during planning): `std::process::Command::new("pi")` on Windows
  only finds `pi.exe`, while npm-installed CLIs (`pi`, `opencode`, `agy`, `code`) are `.cmd`
  shims. `board_core::process::command(program)` resolves a bare name through `PATH` × `PATHEXT`
  on Windows (identity on Unix) and is used for the provider catalogs, the local spawner, the TUI
  editor, and the configured runner's `pwsh` lookup. Default `$EDITOR` on Windows is `notepad`.
- **Managed agents** (`agent.start` + `pane_id`): no board change; manual verification that
  Windows Herdr starts npm `.cmd` shims (e.g. `claude.cmd`), result recorded in `docs/herdr.md`.

## 4. Tests, CI, e2e, docs

- **Rust tests**: Unix-only semantics (symlinks, modes, `process_group`, inode stale-socket) get
  `#[cfg(unix)]`. Transport/server tests move to `board_ipc` and run on both. New Windows tests
  written red first: pipe connect/deadline/busy/EOF; a pending read does not block a write on the
  clone; owner-check rejection where it can be simulated; `try_lock` singleton; `.ps1` generator
  and quoting; `session_name_from_socket` with `\`; detached auto-start child.
- **CI** (`.github/workflows/ci.yml`, mirrored in `docs/README.md` → Test gates, enforced by
  `scripts/tests/test_docs.py`):
  - `windows`: `cargo clippy --workspace --all-targets -- -D warnings` and
    `cargo test --workspace` on `windows-latest`.
  - `live-e2e-windows`: download pinned `herdr-windows-x86_64.zip` v0.9.0, verify SHA-256, run
    `e2e/run-all.sh --require-all` under Git Bash.
- **e2e harness port** (no weakening of isolation rules in `AGENTS.md` / `docs/testing.md`):
  - `process_identity.py`: Windows backend via `ctypes` — start time and exe path via
    `OpenProcess`/`GetProcessTimes`/`QueryFullProcessImageNameW`; cmdline and environment read
    from the PEB (`NtQueryInformationProcess` + `ReadProcessMemory`). The environ-based ownership
    token check is kept.
  - `ndjson_rpc` / `hrpc.py`: connect via `open(r'\\.\pipe\…', 'r+b')` on Windows.
  - `herdr-proxy.py`: named-pipe server on asyncio `ProactorEventLoop`.
  - `fake-bin/*`: `.cmd` shims (`@python "%~dp0<name>" %*`).
  - `lib.sh` and scenarios: platform branches where `/proc`, `ps`, `chmod` etc. are used; `cygpath
    -w` when passing paths to `board.exe` / `herdr.exe`.
  - `e2e/test-harness.sh` and `scripts/tests` pass on Windows.
- **Docs**: README (platform badge + install), `docs/install.md`, `docs/herdr.md` (Windows pipe
  facts), `docs/testing.md` + `e2e/README.md` (Windows isolation), `docs/operations.md`
  (`%APPDATA%` paths), `docs/sandbox.md` (Linux/macOS only), `herdr-plugin.toml`
  `platforms = ["linux", "macos", "windows"]`, one `CHANGELOG.md` `Unreleased` → `### Added` entry.

## Order of work (single PR, Conventional Commits per crate)

1. `board-ipc` 2. `board-herdr` + `board-core` 3. `board-daemon` 4. `board-cli` 5. spawner
6. manual verification against local Windows Herdr 7. CI `windows` job 8. e2e harness port
9. CI `live-e2e-windows` job 10. docs + CHANGELOG.

Stop-and-report trigger: if Windows Herdr's `--session` / ephemeral-session or cleanup behavior
differs enough that the harness port needs a redesign, pause and bring options back before
proceeding.
