// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! The execution environment passed to the executor.
//!
//! [`ExecEnv`] is the v0 envelope owned by the host (binary REPL or
//! `PLAN_05` spec harness) and threaded through `run_source` /
//! `run_script`. See `PLAN_06a` §2.2 for the contract and §7 for the
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
/// and shells out to `/bin/sh` for everything else. The binary REPL
/// wants that fallback during the v0 → v1 transition so users have a
/// working shell; the spec harness wants the *opposite*, because
/// every fallback hides a missing feature behind bash and the harness
/// exists to measure what fredshell-as-itself supports.
///
/// `PLAN_06b` removes the policy field once native execve lands and
/// the fallback goes away. The enum will be retained (as a no-op for
/// callers that name it explicitly) but the `Strict` variant becomes
/// the only behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ExternalCommandPolicy {
    /// Fall back to `/bin/sh -c` for any non-builtin command. The
    /// binary REPL's path; preserves the v0 shell-out behavior.
    #[default]
    FallbackToSh,
    /// Refuse to spawn `/bin/sh`. The dispatcher raises
    /// [`ExecError::NoExternalExecutor`](super::error::ExecError::NoExternalExecutor)
    /// for any command that is not a builtin handled natively.
    /// The spec harness's path.
    Strict,
}

/// The environment a script executes in.
///
/// Constructed by the host (binary or harness) and passed by mutable
/// reference to [`crate::run_source`] / [`crate::run_script`]. The
/// executor mutates the working directory on `cd` and (in `PLAN_06b`)
/// mutates the environment on `export`.
///
/// `#[non_exhaustive]` because `PLAN_06b` adds fields: `stdin` (boxed
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
    /// [`Self::from_process`] (see that constructor's docs). `PLAN_06b`
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
    /// builtins (`cd`, `exit`) do not produce stdout. `PLAN_06b`
    /// rewrites builtins to route through this field too, at which
    /// point this docstring's "every byte" claim becomes literally
    /// true.
    pub stdout: Box<dyn Write + Send>,

    /// Standard error sink. Mirror of [`Self::stdout`].
    pub stderr: Box<dyn Write + Send>,

    /// Whether the dispatcher may shell out to `/bin/sh -c` for
    /// commands it cannot handle natively.
    ///
    /// Defaults differ per constructor:
    /// [`Self::from_process`] picks
    /// [`ExternalCommandPolicy::FallbackToSh`] (the binary REPL's
    /// path); [`Self::sandboxed`] picks
    /// [`ExternalCommandPolicy::Strict`] (the spec harness's path).
    /// Either default can be overridden by mutating this field after
    /// construction.
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
    /// silently in v0; `PLAN_06b` migrates the env map to `OsString`
    /// keys/values and preserves them verbatim.
    ///
    /// [`Self::external_command_policy`] defaults to
    /// [`ExternalCommandPolicy::FallbackToSh`] so the binary REPL
    /// keeps working during the v0 → v1 transition.
    ///
    /// # Errors
    ///
    /// Returns [`ExecError::HostIo`] if [`std::env::current_dir`]
    /// fails (e.g. the cwd was deleted out from under the process,
    /// or the caller lacks permission to read it).
    pub fn from_process() -> Result<Self, ExecError> {
        let cwd = env::current_dir().map_err(ExecError::HostIo)?;
        let env = env::vars_os()
            .filter_map(|(k, v)| {
                let key = k.into_string().ok()?;
                let value = v.into_string().ok()?;
                Some((key, value))
            })
            .collect();
        Ok(Self {
            cwd,
            env,
            stdout: Box::new(io::stdout()),
            stderr: Box::new(io::stderr()),
            external_command_policy: ExternalCommandPolicy::FallbackToSh,
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
    /// [`ExternalCommandPolicy::Strict`] so the harness measures
    /// fredshell-as-itself rather than fredshell-plus-bash. Tests
    /// that want to exercise the v0 fallback can flip the field to
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
    fn external_command_policy_default_is_fallback() {
        // The Default impl is the binary-REPL default. Constructors
        // override it when they have a more specific default.
        let policy = ExternalCommandPolicy::default();
        assert_eq!(policy, ExternalCommandPolicy::FallbackToSh);
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
        // Take a snapshot to compare against. `current_dir` is
        // process-global so this test cannot be parallelised against
        // a `set_current_dir` test.
        let expected_cwd = env::current_dir().expect("test harness has a cwd");
        let env_var_count = env::vars_os().count();

        let exec = ExecEnv::from_process().expect("from_process succeeds when cwd is valid");
        assert_eq!(exec.cwd, expected_cwd);
        assert_eq!(
            exec.external_command_policy,
            ExternalCommandPolicy::FallbackToSh,
            "from_process must default to FallbackToSh"
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
