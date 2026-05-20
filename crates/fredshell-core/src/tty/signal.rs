// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Signal handling: `sigaction` installation and the self-pipe.
//!
//! Owns the signal-handler registration table from `PLAN_04` §4 and
//! the self-pipe used to wake [`super::TerminalSession::wait`] on
//! signal delivery (subtask 04.4). The full implementation lands in
//! 04.4; this stub provides the [`Signal`] tag carried by
//! [`super::WaitEvent::Signal`].

/// Identifies which signal woke [`super::TerminalSession::wait`].
///
/// Only signals fredshell installs handlers for appear here. POSIX
/// signal numbers that we explicitly ignore (`SIGTTOU`, `SIGTTIN`)
/// or leave at default disposition are not represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Signal {
    /// `SIGINT` — user pressed `Ctrl-C`.
    Int,
    /// `SIGQUIT` — user pressed `Ctrl-\`.
    Quit,
    /// `SIGTERM` — graceful termination request.
    Term,
    /// `SIGHUP` — controlling terminal hung up.
    Hup,
    /// `SIGWINCH` — terminal window resized.
    WinCh,
    /// `SIGCHLD` — a child process changed state.
    Chld,
    /// `SIGALRM` — timer expired (used by `read -t`, capability probe).
    Alrm,
    /// `SIGUSR1` — reserved for user-defined use.
    Usr1,
    /// `SIGUSR2` — reserved for user-defined use.
    Usr2,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::Signal;

    #[test]
    fn signal_is_copy() {
        const _: fn() = || {
            fn assert_copy<T: Copy>() {}
            assert_copy::<Signal>();
        };
    }

    #[test]
    fn signal_variants_are_distinct() {
        assert_ne!(Signal::Int, Signal::Quit);
        assert_ne!(Signal::WinCh, Signal::Chld);
    }
}
