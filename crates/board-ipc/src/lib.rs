//! Platform local-IPC transport: AF_UNIX streams on Unix, named pipes on Windows.
//!
//! Herdr on Windows serves its API on `\.\pipe\` + the socket path, and boardd
//! follows the same convention, so `BOARD_SOCKET` / `HERDR_SOCKET_PATH` stay plain
//! paths on every platform. This crate knows nothing about boards or Herdr.

#[cfg(unix)]
pub use std::os::unix::net::{UnixListener as Listener, UnixStream as Stream};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{connect_timeout, endpoint_exists, pipe_name, PipeListener, PipeStream};
#[cfg(windows)]
pub type Stream = PipeStream;
#[cfg(windows)]
pub type Listener = PipeListener;
