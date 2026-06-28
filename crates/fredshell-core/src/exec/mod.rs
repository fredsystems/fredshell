// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Command execution: stub `run_source` / `run_script` dispatcher.
//!
//! v0 implements the public entry points specified by `PLAN_11` §2.3
//! by walking [`Script::source`] line by line and shelling out to
//! `/bin/sh -c` for any non-builtin line (§3). The real parser and
//! executor land with `PLAN_11`; this module exists so `PLAN_05`'s
//! spec harness and the binary REPL can share one code path today.
//!
//! ## Module layout
//!
//! - [`error`] — result/error envelopes (`RunResult`, `RunError`,
//!   `ExecError`, `ExitStatus`).
//! - [`env`] — [`ExecEnv`] and its constructors.
//! - [`builtin`] — [`Tier2Builtin`] trait shape (definitions only).
//! - `testing` (test-only) — crate-internal capture helper that owns
//!   a pair of [`Vec<u8>`] buffers and exposes them as [`Write`] sinks
//!   for the `PLAN_05` spec harness. Gated to `#[cfg(test)]` until
//!   the harness crate (05.4) promotes it behind a cargo feature.
//!
//! See `PLAN_11` §3 for the stub dispatcher's contract and `PLAN_05`
//! §5.2 for the stdio plumbing it routes through.

pub mod builtin;
pub mod env;
pub mod error;
#[cfg(test)]
pub(crate) mod testing;

pub use builtin::{Tier2Builtin, Tier2Ctx, Tier2Error};
pub use env::{ExecEnv, ExternalCommandPolicy};
pub use error::{ExecError, ExitStatus, NoExternalExecutorReason, RunError, RunResult};

use std::io::Write;
use std::process::{Command, Stdio};

use crate::builtins::{self, BuiltinOutcome};
use crate::parser::{Script, parse};
use crate::{CoreError, CoreResult};

/// Execute a command string via `/bin/sh -c` and propagate its exit
/// code by calling [`std::process::exit`].
///
/// Used by the binary's one-shot path (`fredshell -c …`). The
/// interactive REPL and the spec harness use [`run_source`] instead,
/// which returns the exit code as a value rather than aborting the
/// process.
///
/// # Errors
///
/// Returns [`CoreError::SpawnShell`] if `/bin/sh` cannot be spawned
/// (e.g. missing binary, permission denied). A non-zero exit from the
/// spawned shell is **not** an error here: the function calls
/// [`std::process::exit`] with the shell's exit code so one-shot mode
/// (`fredshell -c …`) propagates it.
pub fn run_via_sh(command: &str) -> CoreResult<()> {
    let status = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .status()
        .map_err(|source| CoreError::SpawnShell {
            command: command.to_owned(),
            source,
        })?;

    if !status.success() {
        // Propagate the exit code for one-shot mode (`fredshell -c ...`).
        if let Some(code) = status.code() {
            std::process::exit(code);
        }
    }
    Ok(())
}

/// Parse and execute a source string in one call.
///
/// Convenience wrapper around [`parse`] + [`run_script`]. Used by the
/// `PLAN_05` spec harness and the binary REPL.
///
/// # Errors
///
/// Returns [`RunError::Parse`] if `source` fails to parse (v0 only
/// rejects NUL bytes — see [`crate::parser::parse`]), or
/// [`RunError::Exec`] if the executor itself fails. A script that
/// exits non-zero is **not** a `RunError`; the exit status is carried
/// in [`RunResult::status`].
pub fn run_source(source: &str, env: &mut ExecEnv) -> Result<RunResult, RunError> {
    let script = parse(source)?;
    run_script(&script, env)
}

/// Execute a pre-parsed [`Script`].
///
/// The binary REPL uses this when it has already parsed user input
/// (e.g. to validate before recording in history). The harness uses
/// [`run_source`] instead.
///
/// v0 implementation per `PLAN_11` §3: walks `script.source` line
/// by line, dispatches each non-empty line to the builtin path or to
/// `/bin/sh -c`. The exit status of the last executed line becomes
/// [`RunResult::status`]; an `exit` builtin short-circuits with its
/// requested code.
///
/// # Errors
///
/// Returns [`RunError::Exec`] if the executor cannot run a line at
/// all (e.g. `/bin/sh` cannot be spawned). Per-line non-zero exit
/// codes propagate via [`RunResult::status`].
pub fn run_script(script: &Script, env: &mut ExecEnv) -> Result<RunResult, RunError> {
    dispatch_script(script, env)
}

/// Stub dispatcher shared by [`run_script`] and the harness's capture
/// helper in the `testing` module (test-only).
///
/// Walks the script's source line by line, skipping blank lines,
/// delegating each surviving line to [`dispatch_line`]. The exit
/// status of the last executed line is the script's status; an
/// `exit` builtin short-circuits with its requested code.
pub(crate) fn dispatch_script(script: &Script, env: &mut ExecEnv) -> Result<RunResult, RunError> {
    let mut status = ExitStatus::SUCCESS;
    let mut exit_requested = false;
    for raw_line in script.source().split('\n') {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        match dispatch_line(line, env)? {
            LineOutcome::Continue(s) => status = s,
            LineOutcome::Exit(s) => {
                status = s;
                exit_requested = true;
                break;
            }
        }
    }
    Ok(if exit_requested {
        RunResult::exit(status)
    } else {
        RunResult::new(status)
    })
}

/// What [`dispatch_line`] reports back to [`dispatch_script`].
#[derive(Debug, Clone, Copy)]
enum LineOutcome {
    /// Line executed; the loop should keep going.
    Continue(ExitStatus),
    /// `exit` builtin requested termination. The loop should stop
    /// and surface this status.
    Exit(ExitStatus),
}

/// Execute one line: builtin lookup first, fall back to `/bin/sh -c`.
///
/// The fallback step is governed by [`ExecEnv::external_command_policy`]
/// (introduced by `PLAN_05` §4.2). When the policy is
/// [`ExternalCommandPolicy::Strict`] the dispatcher refuses to spawn
/// `/bin/sh` and surfaces [`ExecError::NoExternalExecutor`] instead.
///
/// Child stdout/stderr is routed through [`ExecEnv::stdout`] /
/// [`ExecEnv::stderr`] (`PLAN_05` §5.2). The binary REPL leaves those
/// pointed at process stdio; the spec harness points them at capture
/// buffers.
fn dispatch_line(line: &str, env: &mut ExecEnv) -> Result<LineOutcome, RunError> {
    // Tokenise for builtin dispatch only. The full line is handed
    // verbatim to `/bin/sh -c` on fallback so the shell does its
    // own quoting/expansion.
    let argv: Vec<String> = match shell_words::split(line) {
        Ok(v) => v,
        Err(_) => {
            // Tokeniser failed (e.g. unterminated quote). In
            // FallbackToSh mode v0 hands it straight to `/bin/sh`
            // so its error reporting is authoritative; in Strict
            // mode the executor refuses with UnparsableArgv.
            // PLAN_11 reports the parse failure via ParseError
            // and the carve-out goes away.
            return match env.external_command_policy {
                ExternalCommandPolicy::FallbackToSh => {
                    spawn_via_sh(line, env).map(LineOutcome::Continue)
                }
                ExternalCommandPolicy::Strict => {
                    Err(RunError::Exec(ExecError::NoExternalExecutor {
                        command: line.to_owned(),
                        reason: NoExternalExecutorReason::UnparsableArgv,
                    }))
                }
            };
        }
    };

    match builtins::try_run(&argv).map_err(|e| RunError::Exec(core_error_to_exec(e)))? {
        Some(BuiltinOutcome::Exit(code)) => Ok(LineOutcome::Exit(ExitStatus(code))),
        Some(BuiltinOutcome::Handled(code)) => Ok(LineOutcome::Continue(ExitStatus(code))),
        None => match env.external_command_policy {
            ExternalCommandPolicy::FallbackToSh => {
                spawn_via_sh(line, env).map(LineOutcome::Continue)
            }
            ExternalCommandPolicy::Strict => Err(RunError::Exec(ExecError::NoExternalExecutor {
                command: line.to_owned(),
                reason: NoExternalExecutorReason::PolicyStrict,
            })),
        },
    }
}

/// Spawn `/bin/sh -c <line>`, piping child stdio into the writers on
/// `env`.
///
/// v0 always pipes — even when the writers point at process stdio —
/// so the dispatcher has a single code path. The cost is one extra
/// copy per command, which `PLAN_11` removes by handing the child
/// inherited file descriptors when the writers are known to be the
/// real stdio. See `PLAN_05` §5.2 and `PLAN_11` §9 for the budget.
fn spawn_via_sh(line: &str, env: &mut ExecEnv) -> Result<ExitStatus, RunError> {
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c")
        .arg(line)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = cmd.output().map_err(|e| {
        RunError::Exec(ExecError::HostIo(std::io::Error::new(
            e.kind(),
            format!("failed to spawn /bin/sh: {e}"),
        )))
    })?;

    env.stdout
        .write_all(&output.stdout)
        .map_err(|e| RunError::Exec(ExecError::HostIo(e)))?;
    env.stderr
        .write_all(&output.stderr)
        .map_err(|e| RunError::Exec(ExecError::HostIo(e)))?;

    Ok(ExitStatus(output.status.code().unwrap_or(-1)))
}

/// Map a [`CoreError`] surfaced by the builtin layer to an
/// [`ExecError`] for the run-pipeline envelope.
///
/// Today no builtin actually produces a [`CoreError`] (the variants
/// are reserved for `PLAN_13`+ builtins); the match exists so a future
/// surface change does not silently lose information.
fn core_error_to_exec(err: CoreError) -> ExecError {
    match err {
        CoreError::SpawnShell { source, .. } | CoreError::ReplIo(source) => {
            ExecError::HostIo(source)
        }
        CoreError::Terminal(_) | CoreError::RawMode(_) | CoreError::Builtin(_) => {
            ExecError::InternalInvariant {
                what: "builtin surfaced non-IO CoreError",
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::exec::env::GLOBAL_ENV_LOCK;
    use std::env as std_env;
    use std::fs;
    use std::path::PathBuf;

    fn sandbox() -> ExecEnv {
        // The dispatcher tests below were written before strict mode
        // existed; they assert the v0 fallback-to-sh behavior. Opt
        // back into fallback so they keep exercising that surface.
        // Tests that want strict construct their env inline.
        let mut env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        env.external_command_policy = ExternalCommandPolicy::FallbackToSh;
        env
    }

    /// Serialise any test that spawns `/bin/sh` or mutates
    /// process-global cwd/env. The `cd_*` tests below create and
    /// remove a tmp directory and `set_current_dir` into it; without
    /// this lock, a parallel spawn inherits the doomed cwd and
    /// `/bin/sh` writes a `getcwd` warning to stderr.
    fn lock() -> std::sync::MutexGuard<'static, ()> {
        GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn run_source_returns_parse_error_on_nul_byte() {
        let _g = lock();
        let mut env = sandbox();
        let err = run_source("echo \0hidden", &mut env).expect_err("NUL must reject");
        match err {
            RunError::Parse(_) => {}
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    #[test]
    fn exit_builtin_short_circuits_with_status() {
        let _g = lock();
        let mut env = sandbox();
        let captured =
            testing::run_source_capturing("echo before\nexit 42\necho after\n", &mut env)
                .expect("ok");
        assert_eq!(captured.result.status, ExitStatus(42));
        assert!(
            captured.result.exit_requested,
            "exit builtin must set exit_requested"
        );
        // "before" ran; "after" did not.
        assert_eq!(captured.stdout, b"before\n");
    }

    #[test]
    fn exit_builtin_with_no_arg_returns_zero() {
        let _g = lock();
        let mut env = sandbox();
        let captured =
            testing::run_source_capturing("exit\necho unreachable\n", &mut env).expect("ok");
        assert_eq!(captured.result.status, ExitStatus::SUCCESS);
        assert!(captured.result.exit_requested);
        assert!(captured.stdout.is_empty());
    }

    #[test]
    fn non_exit_script_leaves_exit_requested_false() {
        let _g = lock();
        let mut env = sandbox();
        let r = run_source("false\n", &mut env).expect("runs");
        assert_eq!(r.status, ExitStatus(1));
        assert!(
            !r.exit_requested,
            "non-zero exit without `exit` builtin must not request termination"
        );
    }

    #[test]
    fn run_source_succeeds_on_whitespace_only() {
        let _g = lock();
        let mut env = sandbox();
        let r = run_source("   \n\t  \n", &mut env).expect("blank ok");
        assert_eq!(r.status, ExitStatus::SUCCESS);
    }

    #[test]
    fn run_source_external_command_succeeds() {
        let _g = lock();
        let mut env = sandbox();
        let r = run_source("true", &mut env).expect("true runs");
        assert_eq!(r.status, ExitStatus::SUCCESS);
    }

    #[test]
    fn run_source_external_command_propagates_non_zero_exit() {
        let _g = lock();
        let mut env = sandbox();
        let r = run_source("false", &mut env).expect("false runs");
        assert!(!r.status.is_success());
        assert_eq!(r.status, ExitStatus(1));
    }

    #[test]
    fn run_source_captures_stdout_via_testing_hook() {
        let _g = lock();
        let mut env = sandbox();
        let captured = testing::run_source_capturing("echo hi", &mut env).expect("ok");
        assert_eq!(captured.result.status, ExitStatus::SUCCESS);
        assert_eq!(captured.stdout, b"hi\n");
        assert!(captured.stderr.is_empty());
    }

    #[test]
    fn run_source_captures_stderr_via_testing_hook() {
        let _g = lock();
        let mut env = sandbox();
        let captured = testing::run_source_capturing("echo bad 1>&2", &mut env).expect("ok");
        assert_eq!(captured.result.status, ExitStatus::SUCCESS);
        assert!(captured.stdout.is_empty());
        assert_eq!(captured.stderr, b"bad\n");
    }

    #[test]
    fn run_source_command_not_found_propagates_via_sh_exit_127() {
        let _g = lock();
        let mut env = sandbox();
        let captured =
            testing::run_source_capturing("definitely-not-a-real-command-fredshell-test", &mut env)
                .expect("spawns sh fine; sh reports not found");
        // POSIX shells return 127 for command-not-found. The
        // executor itself did not fail — the script exited 127.
        assert_eq!(captured.result.status, ExitStatus(127));
    }

    #[test]
    fn run_script_walks_lines_and_returns_last_status() {
        let _g = lock();
        let mut env = sandbox();
        let captured =
            testing::run_source_capturing("echo a\necho b\necho c\n", &mut env).expect("ok");
        assert_eq!(captured.result.status, ExitStatus::SUCCESS);
        assert_eq!(captured.stdout, b"a\nb\nc\n");
    }

    #[test]
    fn run_script_skips_blank_lines() {
        let _g = lock();
        let mut env = sandbox();
        let captured =
            testing::run_source_capturing("\n\necho x\n\n\necho y\n", &mut env).expect("ok");
        assert_eq!(captured.result.status, ExitStatus::SUCCESS);
        assert_eq!(captured.stdout, b"x\ny\n");
    }

    #[test]
    fn cd_builtin_changes_process_cwd_and_subsequent_pwd_sees_it() {
        // The `cd` builtin mutates process-global cwd; serialise.
        let _guard = lock();

        let tmp = std_env::temp_dir().join("fredshell-06a5-cd-test");
        fs::create_dir_all(&tmp).expect("create tmp");
        let original = std_env::current_dir().expect("snapshot cwd");

        let mut env = sandbox();
        let captured =
            testing::run_source_capturing(&format!("cd {}\npwd\n", tmp.display()), &mut env)
                .expect("ok");

        // Canonicalise the expected path *before* removing the
        // directory: on macOS /var is a symlink to /private/var, and
        // `pwd` emits the resolved form. `canonicalize` requires the
        // path to exist, so it must run before `remove_dir` — doing
        // it afterwards silently fell back to the unresolved path and
        // failed the comparison on macOS.
        let rhs = fs::canonicalize(&tmp).unwrap_or_else(|_| tmp.clone());

        // Restore before asserting so a failure does not poison other
        // tests.
        std_env::set_current_dir(&original).expect("restore cwd");
        fs::remove_dir(&tmp).ok();

        assert_eq!(captured.result.status, ExitStatus::SUCCESS);
        // pwd output should match the tmp directory.
        let pwd_out = String::from_utf8(captured.stdout).expect("utf-8");
        let pwd_path = PathBuf::from(pwd_out.trim());
        let lhs = fs::canonicalize(&pwd_path).unwrap_or(pwd_path);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn cd_to_nonexistent_directory_returns_status_one() {
        let _guard = lock();

        let original = std_env::current_dir().expect("snapshot");
        let mut env = sandbox();
        let captured =
            testing::run_source_capturing("cd /this/path/does/not/exist/fredshell-06a5", &mut env)
                .expect("ok");
        std_env::set_current_dir(&original).expect("restore");

        assert_eq!(captured.result.status, ExitStatus(1));
    }

    #[test]
    fn run_source_with_inherit_capture_still_succeeds() {
        // The default Capture::Inherit path is exercised by run_source
        // (the binary REPL's path). It does not capture output but
        // must still return an accurate ExitStatus.
        let _g = lock();
        let mut env = sandbox();
        let r = run_source("true", &mut env).expect("true");
        assert_eq!(r.status, ExitStatus::SUCCESS);
        let r = run_source("false", &mut env).expect("false");
        assert_eq!(r.status, ExitStatus(1));
    }

    #[test]
    fn run_script_with_prebuilt_script_round_trips() {
        let _g = lock();
        let mut env = sandbox();
        let script = parse("true").expect("parses");
        let r = run_script(&script, &mut env).expect("ok");
        assert_eq!(r.status, ExitStatus::SUCCESS);
    }

    #[test]
    fn core_error_to_exec_maps_io_variants_to_hostio() {
        let e = core_error_to_exec(CoreError::ReplIo(std::io::Error::other("x")));
        assert!(matches!(e, ExecError::HostIo(_)));
    }

    #[test]
    fn core_error_to_exec_maps_other_variants_to_internal_invariant() {
        let e = core_error_to_exec(CoreError::Terminal(
            crate::tty::OpenError::NoControllingTerminal,
        ));
        match e {
            ExecError::InternalInvariant { what } => {
                assert!(what.contains("non-IO"));
            }
            other => panic!("expected InternalInvariant, got {other:?}"),
        }
    }

    // --- PLAN_05 §4.2 strict execution mode ----------------------

    /// Build a strict-mode sandbox: refuses any external command.
    fn strict_sandbox() -> ExecEnv {
        let env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        // Sanity-check the default; if `sandboxed()`'s default
        // changes, the rest of these tests will silently lose their
        // meaning.
        assert_eq!(env.external_command_policy, ExternalCommandPolicy::Strict);
        env
    }

    #[test]
    fn strict_mode_refuses_external_command() {
        let _g = lock();
        let mut env = strict_sandbox();
        let err = run_source("/bin/echo hi\n", &mut env)
            .expect_err("strict must refuse external command");
        match err {
            RunError::Exec(ExecError::NoExternalExecutor { command, reason }) => {
                assert_eq!(command, "/bin/echo hi");
                assert_eq!(reason, NoExternalExecutorReason::PolicyStrict);
            }
            other => panic!("expected NoExternalExecutor, got {other:?}"),
        }
    }

    #[test]
    fn strict_mode_refuses_unknown_command_with_policy_strict_reason() {
        let _g = lock();
        let mut env = strict_sandbox();
        let err = run_source("definitely-not-a-real-command-fredshell-strict\n", &mut env)
            .expect_err("strict must refuse");
        match err {
            RunError::Exec(ExecError::NoExternalExecutor { command, reason }) => {
                assert_eq!(command, "definitely-not-a-real-command-fredshell-strict");
                assert_eq!(reason, NoExternalExecutorReason::PolicyStrict);
            }
            other => panic!("expected NoExternalExecutor, got {other:?}"),
        }
    }

    #[test]
    fn strict_mode_refuses_unparsable_argv_with_unparsable_reason() {
        let _g = lock();
        let mut env = strict_sandbox();
        let err = run_source("echo 'unterminated\n", &mut env)
            .expect_err("strict must refuse unparsable argv");
        match err {
            RunError::Exec(ExecError::NoExternalExecutor { command, reason }) => {
                assert_eq!(command, "echo 'unterminated");
                assert_eq!(reason, NoExternalExecutorReason::UnparsableArgv);
            }
            other => panic!("expected NoExternalExecutor(UnparsableArgv), got {other:?}"),
        }
    }

    #[test]
    fn strict_mode_still_runs_builtins() {
        // `exit` is a native builtin and must not be affected by
        // strict mode.
        let _g = lock();
        let mut env = strict_sandbox();
        let r = run_source("exit 7\n", &mut env).expect("builtin runs under strict");
        assert_eq!(r.status, ExitStatus(7));
        assert!(r.exit_requested);
    }

    #[test]
    fn strict_mode_aborts_on_first_external_command() {
        // The script has a builtin then an external; the dispatcher
        // must run the builtin and then fail on the external rather
        // than silently skipping.
        let _g = lock();
        let mut env = strict_sandbox();
        let err = run_source("cd /tmp\ntrue\n", &mut env)
            .expect_err("strict refuses external after builtin");
        match err {
            RunError::Exec(ExecError::NoExternalExecutor { command, .. }) => {
                assert_eq!(command, "true");
            }
            other => panic!("expected NoExternalExecutor on `true`, got {other:?}"),
        }
    }

    #[test]
    fn fallback_mode_still_falls_back() {
        // Belt-and-braces: when the policy is explicitly
        // FallbackToSh, external commands still spawn /bin/sh.
        let _g = lock();
        let mut env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        env.external_command_policy = ExternalCommandPolicy::FallbackToSh;
        let r = run_source("true\n", &mut env).expect("fallback path runs `true`");
        assert_eq!(r.status, ExitStatus::SUCCESS);
    }
}
