// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Typed errors for the spec harness.
//!
//! Per AGENTS.md, library crates do not depend on `anyhow`. Errors
//! are structured enums whose variants document *what went wrong*,
//! not *what to do*.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Top-level error returned by [`crate::run_case`].
#[derive(Debug)]
#[non_exhaustive]
pub enum SpecError {
    /// Failed to load the `.case.toml` file or its sidecars.
    Load(LoadError),
    /// Failed to construct or tear down the per-case sandbox.
    Sandbox {
        /// Path to the sandbox the harness was building.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// The executor itself failed in a way the harness cannot map to
    /// a comparison outcome (e.g., host I/O error mid-run).
    ///
    /// Strict-mode "no native executor" refusals are **not** errors:
    /// they are a legitimate run outcome and surface through
    /// [`crate::CaseOutcome::ExecutorRefused`].
    Executor(fredshell_core::RunError),
}

/// Failure loading a `.case.toml` file (or its sidecar fixtures).
#[derive(Debug)]
#[non_exhaustive]
pub enum LoadError {
    /// The case file could not be read from disk.
    Read {
        /// Path the harness tried to read.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// The case file did not parse as TOML or did not match the
    /// expected schema.
    Schema {
        /// Path of the offending file.
        path: PathBuf,
        /// `toml::de::Error` message; rendered as a string so the
        /// public surface does not leak the `toml` crate's types.
        message: String,
    },
    /// A sidecar fixture (`<case>.stdout` etc.) could not be read.
    Sidecar {
        /// Path the harness tried to read.
        path: PathBuf,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// The recorded `<case>.exit` file did not contain a single
    /// integer line.
    BadExitFixture {
        /// Path of the offending file.
        path: PathBuf,
        /// What the file contained, truncated for diagnostics.
        contents: String,
    },
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(_) => f.write_str("failed to load spec case"),
            Self::Sandbox { path, .. } => {
                write!(f, "sandbox I/O failure at {}", path.display())
            }
            Self::Executor(_) => f.write_str("executor surfaced an error the harness cannot map"),
        }
    }
}

impl std::error::Error for SpecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Load(source) => Some(source),
            Self::Sandbox { source, .. } => Some(source),
            Self::Executor(source) => Some(source),
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, .. } => write!(f, "failed to read case file {}", path.display()),
            Self::Schema { path, message } => {
                write!(f, "schema error in {}: {message}", path.display())
            }
            Self::Sidecar { path, .. } => {
                write!(f, "failed to read fixture {}", path.display())
            }
            Self::BadExitFixture { path, contents } => write!(
                f,
                "exit fixture {} did not contain an integer (got {contents:?})",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Sidecar { source, .. } => Some(source),
            Self::Schema { .. } | Self::BadExitFixture { .. } => None,
        }
    }
}

impl From<LoadError> for SpecError {
    fn from(err: LoadError) -> Self {
        Self::Load(err)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn spec_error_display_load() {
        let err = SpecError::Load(LoadError::Schema {
            path: PathBuf::from("/a/b.toml"),
            message: "missing field `script`".to_owned(),
        });
        assert_eq!(format!("{err}"), "failed to load spec case");
        let inner = std::error::Error::source(&err).expect("has source");
        assert!(inner.to_string().contains("missing field"));
    }

    #[test]
    fn spec_error_display_sandbox() {
        let err = SpecError::Sandbox {
            path: PathBuf::from("/tmp/sb"),
            source: io::Error::other("nope"),
        };
        assert_eq!(format!("{err}"), "sandbox I/O failure at /tmp/sb");
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn spec_error_display_executor() {
        // Build a parse error via the public API.
        let parse_err = fredshell_core::parse("echo \0nope").expect_err("NUL rejects");
        let err = SpecError::Executor(fredshell_core::RunError::Parse(parse_err));
        assert!(format!("{err}").contains("executor"));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn load_error_display_variants() {
        let r = LoadError::Read {
            path: PathBuf::from("x"),
            source: io::Error::other("e"),
        };
        assert!(format!("{r}").contains("failed to read case file"));
        let s = LoadError::Sidecar {
            path: PathBuf::from("x.stdout"),
            source: io::Error::other("e"),
        };
        assert!(format!("{s}").contains("failed to read fixture"));
        let b = LoadError::BadExitFixture {
            path: PathBuf::from("x.exit"),
            contents: "not-a-number".to_owned(),
        };
        assert!(format!("{b}").contains("not-a-number"));
    }

    #[test]
    fn load_error_source_chain() {
        let r = LoadError::Read {
            path: PathBuf::from("x"),
            source: io::Error::other("io"),
        };
        assert!(std::error::Error::source(&r).is_some());
        let s = LoadError::Schema {
            path: PathBuf::from("x"),
            message: "m".to_owned(),
        };
        assert!(std::error::Error::source(&s).is_none());
        let b = LoadError::BadExitFixture {
            path: PathBuf::from("x"),
            contents: "c".to_owned(),
        };
        assert!(std::error::Error::source(&b).is_none());
    }

    #[test]
    fn load_error_converts_into_spec_error() {
        let r = LoadError::Read {
            path: PathBuf::from("x"),
            source: io::Error::other("io"),
        };
        let s: SpecError = r.into();
        assert!(matches!(s, SpecError::Load(_)));
    }
}
