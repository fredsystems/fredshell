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
use std::path::PathBuf;

use super::error::ExecError;

/// The environment a script executes in.
///
/// Constructed by the host (binary or harness) and passed by mutable
/// reference to [`crate::run_source`] / [`crate::run_script`]. The
/// executor mutates the working directory on `cd` and (in `PLAN_06b`)
/// mutates the environment on `export`.
///
/// `#[non_exhaustive]` because `PLAN_06b` adds fields:
/// `stdin`/`stdout`/`stderr` (boxed I/O handles), `shell` (the
/// `ShellState`), `builtins` (the `BuiltinRegistry`), `path_policy`,
/// `signal_policy`. See `PLAN_02` §4.2.
#[derive(Debug)]
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
}

impl ExecEnv {
    /// Construct an [`ExecEnv`] from the calling process.
    ///
    /// `cwd` is initialised from [`std::env::current_dir`] and `env`
    /// from [`std::env::vars_os`]. Non-UTF-8 variables are dropped
    /// silently in v0; `PLAN_06b` migrates the env map to `OsString`
    /// keys/values and preserves them verbatim.
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
        Ok(Self { cwd, env })
    }

    /// Construct an empty [`ExecEnv`] rooted at `cwd` with no
    /// inherited environment variables.
    ///
    /// Used by the `PLAN_05` spec harness for hermetic tests: no host
    /// `$PATH`, `$HOME`, or other ambient state leaks into the
    /// script. The harness is responsible for setting any variables
    /// the test requires (typically by mutating
    /// [`ExecEnv::env`] after construction).
    #[must_use]
    pub fn sandboxed(cwd: PathBuf) -> Self {
        Self {
            cwd,
            env: HashMap::new(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::fs;
    use std::sync::Mutex;

    /// Serializes tests that mutate process-global state
    /// (`env::set_var`, `env::set_current_dir`). Without this, the
    /// parallel test runner can observe one test's cwd swap during
    /// another test's `from_process` call.
    static GLOBAL_ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn sandboxed_starts_with_given_cwd_and_empty_env() {
        let cwd = PathBuf::from("/tmp/fredshell-test-sandbox");
        let env = ExecEnv::sandboxed(cwd.clone());
        assert_eq!(env.cwd, cwd);
        assert!(env.env.is_empty());
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
    }
}
