// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Termios state and the raw-mode RAII guard.
//!
//! Owns the `tcgetattr` / `tcsetattr` pair that enters raw mode and
//! the [`RawModeGuard`] that restores the saved cooked-mode termios
//! when dropped (see `PLAN_04` §3 and subtask 04.5). Stub today.

use std::os::fd::RawFd;

/// RAII guard that restores cooked-mode termios on drop.
///
/// Constructed by [`super::TerminalSession::enter_raw_mode`] in
/// subtask 04.5. The guard owns a copy of the termios captured
/// before raw mode was applied and the fd it was applied to; on
/// drop it calls `tcsetattr(TCSAFLUSH)` to restore.
///
/// Today this is an opaque placeholder; fields are added in 04.5.
#[derive(Debug)]
pub struct RawModeGuard {
    /// Fd whose termios will be restored on drop. Populated by 04.5.
    #[allow(dead_code)] // wired up in 04.5
    fd: RawFd,
}
