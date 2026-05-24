// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! The execution environment passed to the executor.
//!
//! [`ExecEnv`] is the v0 envelope owned by the host (binary REPL or
//! `PLAN_05` spec harness) and threaded through `run_source` /
//! `run_script`. See `PLAN_11` §2.2 for the contract and §7 for the
//! v0 deviations from `PLAN_02` §4.2 (notably: env map keyed by
//! `String`, no stdio/shell/builtins fields yet).
//!
//! [`ExecEnv::from_process`] inherits the calling process's working
//! directory and environment variables; the binary uses this.
//! [`ExecEnv::sandboxed`] constructs an empty environment rooted at
//! an explicit directory; the harness uses this for hermetic tests.

use std::collections::HashMap;
use std::env;
use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;

use super::error::ExecError;

/// Serialises tests that mutate process-global state
/// (`env::set_var`, `env::set_current_dir`). Shared between
/// [`mod@tests`] and the dispatcher tests in `exec/mod.rs` so the
/// parallel test runner cannot observe one test's cwd swap during
/// another test's `from_process` call.
#[cfg(test)]
pub(crate) static GLOBAL_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Whether the dispatcher may fall back to `/bin/sh -c` for commands
/// the native executor does not yet handle.
///
/// Introduced by `PLAN_05` §4.2 ("strict execution mode"). v0's
/// dispatcher is a stub: it handles a handful of builtins natively
/// and would historically have shelled out to `/bin/sh` for
/// everything else. The spec harness has always run [`Strict`] so it
/// measures fredshell-as-itself rather than fredshell-plus-bash;
/// since the scream-test recalibration (see `Documents/decisions/`
/// 0004 — strict-default execution), the binary REPL also defaults
/// to [`Strict`]. Silent divergence from bash is exactly the class
/// of bug strict-default eliminates: when fredshell cannot handle a
/// command, the user hears about it immediately rather than having
/// bash silently paper over the gap.
///
/// An opt-in escape hatch is provided for interactive dogfooding
/// while the native executor is incomplete — see
/// [`ExecEnv::from_process`] for the `FREDSHELL_ALLOW_SH_FALLBACK`
/// environment variable. The escape hatch is temporary and will be
/// removed before v1.0.
///
/// `PLAN_11` removes the policy field once native execve lands and
/// the fallback goes away. The enum will be retained (as a no-op for
/// callers that name it explicitly) but the [`Strict`] variant
/// becomes the only behavior.
///
/// [`Strict`]: ExternalCommandPolicy::Strict
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ExternalCommandPolicy {
    /// Refuse to spawn `/bin/sh`. The dispatcher raises
    /// [`ExecError::NoExternalExecutor`](super::error::ExecError::NoExternalExecutor)
    /// for any command that is not a builtin handled natively.
    /// The default for both the binary REPL and the spec harness.
    #[default]
    Strict,
    /// Fall back to `/bin/sh -c` for any non-builtin command.
    /// Opt-in only via the `FREDSHELL_ALLOW_SH_FALLBACK=1`
    /// environment variable (see [`ExecEnv::from_process`]) or by
    /// mutating [`ExecEnv::external_command_policy`] after
    /// construction. Temporary; removed before v1.0.
    FallbackToSh,
}

/// The environment a script executes in.
///
/// Constructed by the host (binary or harness) and passed by mutable
/// reference to [`crate::run_source`] / [`crate::run_script`]. The
/// executor mutates the working directory on `cd` and (in `PLAN_11`)
/// mutates the environment on `export`.
///
/// `#[non_exhaustive]` because `PLAN_11` adds fields: `stdin` (boxed
/// reader), `shell` (the `ShellState`), `builtins` (the
/// `BuiltinRegistry`), `path_policy`, `signal_policy`. See `PLAN_02`
/// §4.2.
///
/// `PLAN_05` 05.2 moved `stdout` and `stderr` from a sibling
/// [`super::Capture`] enum onto this struct as boxed [`Write`]
/// trait objects so the harness can inject buffer sinks without a
/// dispatcher-level carve-out. The writers must be `Send` so a future
/// parallel harness (open in `PLAN_05` §10.2) can drive cases on
/// worker threads without a refactor; the binary REPL's threading
/// shape (a single executor thread per process) is unaffected.
#[non_exhaustive]
pub struct ExecEnv {
    /// Working directory.
    ///
    /// The executor mutates this on `cd`. v0 holds the directory as
    /// owned `PathBuf`; the host is responsible for keeping it
    /// resolvable on the host filesystem.
    pub cwd: PathBuf,

    /// Environment variables visible to the script.
    ///
    /// v0 uses `HashMap<String, String>` for test ergonomics. Keys
    /// or values containing non-UTF-8 bytes are dropped by
    /// [`Self::from_process`] (see that constructor's docs). `PLAN_11`
    /// migrates to `HashMap<OsString, OsString>` per `PLAN_02` §4.2.
    pub env: HashMap<String, String>,

    /// Standard output sink.
    ///
    /// Every byte the executor emits on behalf of the script goes
    /// through this writer. The binary REPL leaves it pointed at
    /// [`io::stdout`]; the spec harness swaps in a buffer sink (see
    /// [`super::testing`]).
    ///
    /// In v0, only the `spawn_via_sh` path actually writes here —
    /// builtins (`cd`, `exit`) do not produce stdout. `PLAN_11`
    /// rewrites builtins to route through this field too, at which
    /// point this docstring's "every byte" claim becomes literally
    /// true.
    pub stdout: Box<dyn Write + Send>,

    /// Standard error sink. Mirror of [`Self::stdout`].
    pub stderr: Box<dyn Write + Send>,

    /// Whether the dispatcher may shell out to `/bin/sh -c` for
    /// commands it cannot handle natively.
    ///
    /// Both constructors default to [`ExternalCommandPolicy::Strict`]
    /// (see decision record `0004 — strict-default execution`).
    /// [`Self::from_process`] additionally honours the
    /// `FREDSHELL_ALLOW_SH_FALLBACK=1` environment variable as a
    /// temporary escape hatch while the native executor is incomplete.
    /// The field is `pub` so any host can override the default by
    /// mutating it after construction.
    pub external_command_policy: ExternalCommandPolicy,
}

impl fmt::Debug for ExecEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `Box<dyn Write>` is not `Debug`; render placeholders so the
        // struct as a whole stays printable for `assert!` diagnostics
        // and `dbg!`.
        f.debug_struct("ExecEnv")
            .field("cwd", &self.cwd)
            .field("env", &self.env)
            .field("stdout", &"<dyn Write>")
            .field("stderr", &"<dyn Write>")
            .field("external_command_policy", &self.external_command_policy)
            .finish()
    }
}

impl ExecEnv {
    /// Construct an [`ExecEnv`] from the calling process.
    ///
    /// `cwd` is initialised from [`std::env::current_dir`] and `env`
    /// from [`std::env::vars_os`]. Non-UTF-8 variables are dropped
    /// silently in v0; `PLAN_11` migrates the env map to `OsString`
    /// keys/values and preserves them verbatim.
    ///
    /// [`Self::external_command_policy`] defaults to
    /// [`ExternalCommandPolicy::Strict`]. As a temporary escape hatch
    /// while the native executor is incomplete, setting the
    /// environment variable `FREDSHELL_ALLOW_SH_FALLBACK=1` (exact
    /// match, à la `RUST_BACKTRACE=1`) selects
    /// [`ExternalCommandPolicy::FallbackToSh`] instead. Any other
    /// value — including empty, `0`, `true`, or unset — leaves the
    /// policy at [`ExternalCommandPolicy::Strict`]. The escape hatch
    /// is removed before v1.0; see decision record
    /// `0004 — strict-default execution`.
    ///
    /// # Errors
    ///
    /// Returns [`ExecError::HostIo`] if [`std::env::current_dir`]
    /// fails (e.g. the cwd was deleted out from under the process,
    /// or the caller lacks permission to read it).
    pub fn from_process() -> Result<Self, ExecError> {
        let cwd = env::current_dir().map_err(ExecError::HostIo)?;
        let env: HashMap<String, String> = env::vars_os()
            .filter_map(|(k, v)| {
                let key = k.into_string().ok()?;
                let value = v.into_string().ok()?;
                Some((key, value))
            })
            .collect();
        let external_command_policy = if env
            .get("FREDSHELL_ALLOW_SH_FALLBACK")
            .is_some_and(|v| v == "1")
        {
            ExternalCommandPolicy::FallbackToSh
        } else {
            ExternalCommandPolicy::Strict
        };
        Ok(Self {
            cwd,
            env,
            stdout: Box::new(io::stdout()),
            stderr: Box::new(io::stderr()),
            external_command_policy,
        })
    }

    /// Construct an empty [`ExecEnv`] rooted at `cwd` with no
    /// inherited environment variables.
    ///
    /// Used by the `PLAN_05` spec harness for hermetic tests: no host
    /// `$PATH`, `$HOME`, or other ambient state leaks into the
    /// script. The harness is responsible for setting any variables
    /// the test requires (typically by mutating
    /// [`ExecEnv::env`] after construction).
    ///
    /// [`Self::external_command_policy`] defaults to
    /// [`ExternalCommandPolicy::Strict`]. Unlike [`Self::from_process`],
    /// this constructor does not consult any environment variables —
    /// hermeticity is the harness's whole point. Tests that want to
    /// exercise the v0 `/bin/sh` fallback can flip the field to
    /// [`ExternalCommandPolicy::FallbackToSh`] after construction.
    ///
    /// [`Self::stdout`] and [`Self::stderr`] default to [`io::stdout`]
    /// and [`io::stderr`]; the harness swaps in [`super::testing`]'s
    /// shared buffer wrappers after construction.
    #[must_use]
    pub fn sandboxed(cwd: PathBuf) -> Self {
        Self {
            cwd,
            env: HashMap::new(),
            stdout: Box::new(io::stdout()),
            stderr: Box::new(io::stderr()),
            external_command_policy: ExternalCommandPolicy::Strict,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;

    use super::GLOBAL_ENV_LOCK;

    #[test]
    fn sandboxed_starts_with_given_cwd_and_empty_env() {
        let cwd = PathBuf::from("/tmp/fredshell-test-sandbox");
        let env = ExecEnv::sandboxed(cwd.clone());
        assert_eq!(env.cwd, cwd);
        assert!(env.env.is_empty());
    }

    #[test]
    fn sandboxed_defaults_to_strict_external_command_policy() {
        let env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        assert_eq!(env.external_command_policy, ExternalCommandPolicy::Strict);
    }

    #[test]
    fn external_command_policy_is_mutable_post_construction() {
        let mut env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        env.external_command_policy = ExternalCommandPolicy::FallbackToSh;
        assert_eq!(
            env.external_command_policy,
            ExternalCommandPolicy::FallbackToSh
        );
    }

    #[test]
    fn external_command_policy_default_is_strict() {
        // Strict is the universal default since the scream-test
        // recalibration (decision 0004). The Default impl reflects
        // this; constructors do not weaken it.
        let policy = ExternalCommandPolicy::default();
        assert_eq!(policy, ExternalCommandPolicy::Strict);
    }

    #[test]
    fn sandboxed_env_is_mutable() {
        let mut env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        env.env.insert("FOO".to_owned(), "bar".to_owned());
        assert_eq!(env.env.get("FOO").map(String::as_str), Some("bar"));
    }

    #[test]
    fn sandboxed_accepts_relative_path() {
        // The constructor does not canonicalize; it stores what it is
        // given. Hermeticity is the harness's concern.
        let env = ExecEnv::sandboxed(PathBuf::from("relative/path"));
        assert_eq!(env.cwd, PathBuf::from("relative/path"));
    }

    #[test]
    fn from_process_returns_current_dir_and_env() {
        let _guard = GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Scrub the escape-hatch env var so this test asserts the
        // documented default rather than whatever the developer
        // happens to have exported in their shell.
        let previous_fallback = env::var_os("FREDSHELL_ALLOW_SH_FALLBACK");
        // SAFETY: serialized via GLOBAL_ENV_LOCK; we restore below.
        unsafe {
            env::remove_var("FREDSHELL_ALLOW_SH_FALLBACK");
        }

        // Take a snapshot to compare against. `current_dir` is
        // process-global so this test cannot be parallelised against
        // a `set_current_dir` test.
        let expected_cwd = env::current_dir().expect("test harness has a cwd");
        let env_var_count = env::vars_os().count();

        let exec = ExecEnv::from_process().expect("from_process succeeds when cwd is valid");

        // Restore before asserting so a failure cannot poison other
        // tests in the suite.
        // SAFETY: serialized via GLOBAL_ENV_LOCK.
        unsafe {
            if let Some(v) = previous_fallback {
                env::set_var("FREDSHELL_ALLOW_SH_FALLBACK", v);
            }
        }

        assert_eq!(exec.cwd, expected_cwd);
        assert_eq!(
            exec.external_command_policy,
            ExternalCommandPolicy::Strict,
            "from_process must default to Strict when the escape-hatch env var is unset"
        );

        // Env map has at most the original count (non-UTF-8 entries
        // are dropped) and at least one entry — every reasonable test
        // environment has at least `PATH` or similar.
        assert!(exec.env.len() <= env_var_count);
        assert!(
            !exec.env.is_empty(),
            "expected at least one UTF-8 env var in the test process"
        );
    }

    /// Helper: run `body` with `FREDSHELL_ALLOW_SH_FALLBACK` set to
    /// `value` (or unset if `None`), restoring the previous value on
    /// exit. Caller must hold [`GLOBAL_ENV_LOCK`].
    fn with_fallback_env_var<R>(value: Option<&str>, body: impl FnOnce() -> R) -> R {
        let previous = env::var_os("FREDSHELL_ALLOW_SH_FALLBACK");
        // SAFETY: caller holds GLOBAL_ENV_LOCK; we restore on exit.
        unsafe {
            match value {
                Some(v) => env::set_var("FREDSHELL_ALLOW_SH_FALLBACK", v),
                None => env::remove_var("FREDSHELL_ALLOW_SH_FALLBACK"),
            }
        }
        let out = body();
        // SAFETY: caller holds GLOBAL_ENV_LOCK.
        unsafe {
            match previous {
                Some(v) => env::set_var("FREDSHELL_ALLOW_SH_FALLBACK", v),
                None => env::remove_var("FREDSHELL_ALLOW_SH_FALLBACK"),
            }
        }
        out
    }

    #[test]
    fn from_process_without_env_var_defaults_to_strict() {
        let _guard = GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let policy = with_fallback_env_var(None, || {
            ExecEnv::from_process()
                .expect("from_process succeeds")
                .external_command_policy
        });
        assert_eq!(policy, ExternalCommandPolicy::Strict);
    }

    #[test]
    fn from_process_with_env_var_set_to_1_returns_fallback() {
        let _guard = GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let policy = with_fallback_env_var(Some("1"), || {
            ExecEnv::from_process()
                .expect("from_process succeeds")
                .external_command_policy
        });
        assert_eq!(policy, ExternalCommandPolicy::FallbackToSh);
    }

    #[test]
    fn from_process_with_env_var_set_to_other_value_stays_strict() {
        let _guard = GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Anything other than the exact string "1" is rejected.
        // `true`, `yes`, `0`, `2`, mixed case — all leave the policy
        // at Strict. This mirrors `RUST_BACKTRACE=1` semantics.
        for value in ["0", "true", "yes", "TRUE", "01", " 1", "1 "] {
            let policy = with_fallback_env_var(Some(value), || {
                ExecEnv::from_process()
                    .expect("from_process succeeds")
                    .external_command_policy
            });
            assert_eq!(
                policy,
                ExternalCommandPolicy::Strict,
                "value {value:?} must not engage the escape hatch"
            );
        }
    }

    #[test]
    fn from_process_with_empty_env_var_stays_strict() {
        let _guard = GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let policy = with_fallback_env_var(Some(""), || {
            ExecEnv::from_process()
                .expect("from_process succeeds")
                .external_command_policy
        });
        assert_eq!(policy, ExternalCommandPolicy::Strict);
    }

    #[test]
    fn from_process_inherits_a_known_variable() {
        let _guard = GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Set a sentinel; verify from_process picks it up.
        let key = "FREDSHELL_TEST_FROM_PROCESS_SENTINEL";
        let value = "06a-2-sentinel";
        // SAFETY: serialized via GLOBAL_ENV_LOCK; key is unique.
        unsafe {
            env::set_var(key, value);
        }
        let exec = ExecEnv::from_process().expect("from_process succeeds");
        // SAFETY: matched set_var above.
        unsafe {
            env::remove_var(key);
        }
        assert_eq!(exec.env.get(key).map(String::as_str), Some(value));
    }

    #[test]
    fn from_process_drops_non_utf8_keys_and_values() {
        let _guard = GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // On Unix, OS strings can contain arbitrary bytes. Construct
        // a non-UTF-8 value via OsStr and confirm it is dropped.
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let key = "FREDSHELL_TEST_NON_UTF8_VALUE";
            let bad_value = OsStr::from_bytes(b"valid\xFFinvalid");
            // SAFETY: serialized via GLOBAL_ENV_LOCK; key is unique.
            unsafe {
                env::set_var(key, bad_value);
            }
            let exec = ExecEnv::from_process().expect("from_process succeeds");
            // SAFETY: matched set_var above.
            unsafe {
                env::remove_var(key);
            }
            assert!(
                !exec.env.contains_key(key),
                "non-UTF-8 value should have been dropped"
            );
        }
        // On non-unix targets the filter still compiles; there is no
        // portable way to inject a non-UTF-8 env var, so the test is
        // a no-op there.
        #[cfg(not(unix))]
        {
            let _ = OsStr::new("placeholder");
        }
    }

    #[test]
    fn from_process_propagates_current_dir_failure() {
        let _guard = GLOBAL_ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Simulate `current_dir` failing by removing the cwd out
        // from under the process. This must run serially against
        // other tests that read the cwd, so we restore before
        // returning.
        let tmp = env::temp_dir().join("fredshell-from-process-rm-cwd");
        fs::create_dir_all(&tmp).expect("create tmp dir");
        let original = env::current_dir().expect("snapshot cwd");

        // SAFETY: serialized via GLOBAL_ENV_LOCK; we restore the cwd
        // before returning.
        env::set_current_dir(&tmp).expect("chdir into tmp");
        fs::remove_dir(&tmp).expect("remove tmp out from under us");

        let result = ExecEnv::from_process();

        // Restore before asserting so a failure does not poison
        // other tests.
        env::set_current_dir(&original).expect("restore original cwd");

        match result {
            Err(ExecError::HostIo(_)) => {}
            Ok(_) => panic!("expected HostIo error when cwd is deleted"),
            Err(other) => panic!("expected HostIo error, got {other:?}"),
        }
    }

    #[test]
    fn debug_impl_is_present() {
        let env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        let s = format!("{env:?}");
        assert!(s.contains("ExecEnv"));
        // Manual Debug impl renders the Write trait objects as a
        // placeholder so the whole struct stays printable.
        assert!(s.contains("<dyn Write>"));
    }

    #[test]
    fn sandboxed_default_stdout_stderr_writers_accept_bytes() {
        // The default writers point at process stdio. We cannot
        // observe what they emit from a unit test, but we can
        // verify `write_all` succeeds — a sanity check that the
        // boxed trait object was constructed correctly. The bytes
        // are sent to the test runner's stdio and discarded.
        let mut env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        env.stdout
            .write_all(b"")
            .expect("default stdout writer accepts an empty slice");
        env.stderr
            .write_all(b"")
            .expect("default stderr writer accepts an empty slice");
    }

    #[test]
    fn stdout_writer_can_be_replaced_post_construction() {
        // The harness swaps the default writers with shared buffers;
        // confirm the field is genuinely mutable, not pinned.
        let mut env = ExecEnv::sandboxed(PathBuf::from("/tmp"));
        let buf: Vec<u8> = Vec::new();
        env.stdout = Box::new(buf);
        env.stdout.write_all(b"hi").expect("custom writer accepts");
    }
}
