// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Per-case hermetic sandbox.
//!
//! `PLAN_05` §4.2 prescribes a fresh sandbox directory per case, with
//! a scrubbed environment and the working directory set inside the
//! sandbox. 05.4's implementation uses [`tempfile::TempDir`] for
//! automatic teardown on `Drop`; the harness preserves the sandbox on
//! test failure by persisting the temp directory (via an
//! `into_persisted` accessor) before dropping (deferred to 05.5 / 05.6
//! when failure reporting lands).
//!
//! `$SANDBOX` placeholder substitution: case `env` values may contain
//! the literal substring `$SANDBOX`, which is replaced at runtime
//! with the absolute path of the sandbox root. This is the only
//! interpolation the harness supports; bash-style `$VAR` is not
//! expanded.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::case::CaseEnv;
use crate::error::SpecError;

/// A per-case sandbox directory.
///
/// Dropping the struct removes the directory recursively (via
/// [`TempDir`]'s `Drop` impl). The harness keeps the sandbox alive
/// for the duration of the case run and tears it down on success;
/// on failure 05.5 will preserve it under `target/spec-failures/`.
pub struct Sandbox {
    /// Owns the temp directory; drop semantics handle teardown.
    dir: TempDir,
}

impl Sandbox {
    /// Construct a fresh sandbox under the host's temp directory.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError::Sandbox`] if the temp directory cannot
    /// be created.
    pub fn new() -> Result<Self, SpecError> {
        let dir = TempDir::with_prefix("fredshell-spec-").map_err(|source| SpecError::Sandbox {
            path: PathBuf::from("<tempdir-prefix>"),
            source,
        })?;
        Ok(Self { dir })
    }

    /// Absolute path to the sandbox root.
    #[must_use]
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Copy an `<case>.fs/` skeleton directory into the sandbox
    /// root, preserving its tree shape.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError::Sandbox`] for any I/O failure while
    /// copying.
    pub fn materialize_skeleton(&self, skeleton: &Path) -> Result<(), SpecError> {
        copy_dir_recursive(skeleton, self.dir.path()).map_err(|source| SpecError::Sandbox {
            path: skeleton.to_owned(),
            source,
        })
    }

    /// Resolve a [`CaseEnv`] against this sandbox into a flat
    /// `HashMap<String, String>` ready to drop onto
    /// [`fredshell_core::ExecEnv::env`].
    ///
    /// Every `$SANDBOX` substring is replaced with the absolute
    /// sandbox path. `HOME` and `PATH` are folded into the same map
    /// alongside any `extra` entries.
    ///
    /// Returns an empty map if the sandbox path is not valid UTF-8.
    /// In v0 the `env` map is `HashMap<String, String>`; non-UTF-8
    /// paths cannot be represented and the harness treats them as a
    /// fatal setup error caught higher up (see [`run_case`](crate::run_case)).
    #[must_use]
    pub fn resolve_env(&self, case_env: &CaseEnv) -> HashMap<String, String> {
        let Some(root) = self.dir.path().to_str() else {
            return HashMap::new();
        };
        let substitute = |raw: &str| raw.replace("$SANDBOX", root);
        let mut out = HashMap::new();
        if let Some(home) = &case_env.home {
            out.insert("HOME".to_owned(), substitute(home));
        }
        if let Some(path) = &case_env.path {
            out.insert("PATH".to_owned(), substitute(path));
        }
        for (k, v) in &case_env.extra {
            out.insert(k.clone(), substitute(v));
        }
        out
    }

    /// Returns `true` if the sandbox path is valid UTF-8.
    ///
    /// The v0 `ExecEnv` env map is keyed by `String`; if the host
    /// `TMPDIR` is non-UTF-8 the harness must refuse to run the case
    /// rather than silently lose the sandbox path. `PLAN_06`'s
    /// migration to `OsString` removes this restriction.
    #[must_use]
    pub fn root_is_utf8(&self) -> bool {
        self.dir.path().to_str().is_some()
    }
}

fn copy_dir_recursive(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dst = to.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dst)?;
        } else {
            // Symlinks and special files are out of scope for v0.
            // `PLAN_06` may need symlink fixtures (e.g. for `readlink`
            // tests); revisit when a case requires it.
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn new_creates_a_directory_that_exists() {
        let s = Sandbox::new().unwrap();
        assert!(s.root().is_dir());
    }

    #[test]
    fn drop_removes_the_directory() {
        let path = {
            let s = Sandbox::new().unwrap();
            s.root().to_owned()
        };
        // After drop the directory should be gone.
        assert!(!path.exists());
    }

    #[test]
    fn root_is_utf8_on_normal_hosts() {
        let s = Sandbox::new().unwrap();
        assert!(s.root_is_utf8());
    }

    #[test]
    fn resolve_env_substitutes_sandbox_placeholder() {
        let s = Sandbox::new().unwrap();
        let case_env = CaseEnv {
            home: Some("$SANDBOX/home".to_owned()),
            path: Some("$SANDBOX/bin:/usr/bin".to_owned()),
            extra: {
                let mut m = BTreeMap::new();
                m.insert("FOO".to_owned(), "$SANDBOX/data".to_owned());
                m.insert("STATIC".to_owned(), "no-placeholder".to_owned());
                m
            },
        };
        let resolved = s.resolve_env(&case_env);
        let root = s.root().to_str().unwrap();
        assert_eq!(
            resolved.get("HOME").map(String::as_str),
            Some(format!("{root}/home").as_str())
        );
        assert_eq!(
            resolved.get("PATH").map(String::as_str),
            Some(format!("{root}/bin:/usr/bin").as_str())
        );
        assert_eq!(
            resolved.get("FOO").map(String::as_str),
            Some(format!("{root}/data").as_str())
        );
        assert_eq!(
            resolved.get("STATIC").map(String::as_str),
            Some("no-placeholder")
        );
    }

    #[test]
    fn resolve_env_is_empty_when_case_env_is_empty() {
        let s = Sandbox::new().unwrap();
        let resolved = s.resolve_env(&CaseEnv::default());
        assert!(resolved.is_empty());
    }

    #[test]
    fn materialize_skeleton_copies_files_and_subdirs() {
        let src = TempDir::new().unwrap();
        fs::write(src.path().join("top.txt"), b"top-contents").unwrap();
        fs::create_dir(src.path().join("sub")).unwrap();
        fs::write(src.path().join("sub/inner.txt"), b"inner").unwrap();

        let sandbox = Sandbox::new().unwrap();
        sandbox.materialize_skeleton(src.path()).unwrap();

        assert_eq!(
            fs::read(sandbox.root().join("top.txt")).unwrap(),
            b"top-contents"
        );
        assert_eq!(
            fs::read(sandbox.root().join("sub/inner.txt")).unwrap(),
            b"inner"
        );
    }

    #[test]
    fn materialize_skeleton_errors_on_missing_source() {
        let sandbox = Sandbox::new().unwrap();
        let missing = PathBuf::from("/nonexistent-fredshell-skeleton-source-xyz");
        let err = sandbox.materialize_skeleton(&missing).unwrap_err();
        match err {
            SpecError::Sandbox { path, .. } => assert_eq!(path, missing),
            other => panic!("expected Sandbox, got {other:?}"),
        }
    }
}
