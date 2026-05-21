// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Tier-2 builtin trait, invocation context, and error envelope.
//!
//! v0 ships **definitions only**. No tier-2 builtin is registered
//! today; the registry that consumes this trait lands with `PLAN_09`.
//! The shapes are nailed down now so:
//!
//! - `PLAN_06b` can wire the dispatcher against a stable trait object.
//! - `PLAN_09` can implement individual builtins without re-litigating
//!   the calling convention.
//!
//! See `PLAN_06a` §2.6 for the contract.
//!
//! ## v0 type choices
//!
//! `args` uses `&[String]` and `env` uses `&HashMap<String, String>`
//! to match [`crate::exec::ExecEnv`]'s v0 representation. `PLAN_06b`
//! migrates both to `OsString` together with `ExecEnv::env` (see
//! `PLAN_02` §4.2). The migration is a coordinated change; callers
//! today are tests only.
//!
//! ## Object safety
//!
//! [`Tier2Builtin`] is object-safe. The dispatcher will own
//! `Box<dyn Tier2Builtin>` entries in the registry. A compile-time
//! check lives in the test module to guard against accidental
//! breakage of object safety in future edits.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::error::ExitStatus;

/// Borrowed invocation context handed to a tier-2 builtin.
///
/// All fields are short-lived borrows from the dispatcher's stack
/// frame: a builtin must not stash them past the `invoke` call.
///
/// `#[non_exhaustive]` because `PLAN_06b` adds further fields
/// (typed redirection state, signal masks, `ShellState` access).
#[non_exhaustive]
pub struct Tier2Ctx<'a> {
    /// Argument vector. `args[0]` is the builtin's invocation name
    /// (which may differ from `Tier2Builtin::name` if an alias was
    /// used); `args[1..]` are the operands.
    pub args: &'a [String],

    /// Working directory at invocation. Builtins that need to change
    /// it (e.g. `cd`) do so via the future `ShellState` handle, not
    /// by mutating this borrow.
    pub cwd: &'a Path,

    /// Environment visible to the builtin. v0 type per the module
    /// docs; `PLAN_06b` migrates to `OsString`.
    pub env: &'a HashMap<String, String>,

    /// Standard input. Wrapped as a trait object so the dispatcher
    /// can hand the builtin a pipe end, a file, or `/dev/null`
    /// without parameterizing the trait.
    pub stdin: &'a mut dyn io::Read,

    /// Standard output. See [`Self::stdin`] for the trait-object
    /// rationale.
    pub stdout: &'a mut dyn io::Write,

    /// Standard error. See [`Self::stdin`] for the trait-object
    /// rationale.
    pub stderr: &'a mut dyn io::Write,

    /// Cooperative cancellation flag. Set by the dispatcher when
    /// the user interrupts (SIGINT) or the script times out. Long-
    /// running builtins must poll this between units of work and
    /// return early if it transitions to `true`.
    pub cancellation: &'a AtomicBool,
}

/// Failure produced by a tier-2 builtin.
///
/// A builtin that simply wants to return a non-zero exit code
/// returns `Ok(ExitStatus(n))` — that is a *script-level* failure,
/// not a [`Tier2Error`]. This enum is reserved for failures of the
/// builtin's host machinery: I/O on the provided streams, or a
/// detected invariant violation.
///
/// `#[non_exhaustive]` for the same reason as
/// [`crate::exec::ExecError`].
#[derive(Debug)]
#[non_exhaustive]
pub enum Tier2Error {
    /// I/O on `stdin` / `stdout` / `stderr` failed, or a builtin
    /// that opens additional host resources failed at the host
    /// boundary.
    HostIo(io::Error),

    /// The builtin reached a state it considers a bug. Never
    /// produced in normal operation; surfaced for tests.
    InternalInvariant {
        /// Short static description of the violated invariant.
        what: &'static str,
    },
}

impl std::fmt::Display for Tier2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostIo(_) => f.write_str("host I/O failure"),
            Self::InternalInvariant { what } => {
                write!(f, "internal invariant violated: {what}")
            }
        }
    }
}

impl std::error::Error for Tier2Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::HostIo(source) => Some(source),
            Self::InternalInvariant { .. } => None,
        }
    }
}

impl From<io::Error> for Tier2Error {
    fn from(err: io::Error) -> Self {
        Self::HostIo(err)
    }
}

/// A tier-2 builtin: a builtin implemented inside `fredshell` that
/// participates in the same dispatch loop as external commands.
///
/// Tier-2 builtins are **not** the small set of builtins that must
/// run inside the shell process for correctness (`cd`, `exec`,
/// `export`, etc.) — those are tier-1 and live in
/// [`crate::builtins`]. Tier-2 covers convenience builtins shipped
/// by fredshell for ergonomic reasons (e.g. a richer `ls`, `pwd`
/// formatting). The split is purely organisational; the dispatcher
/// treats both the same.
///
/// The trait is `Send + Sync` so a single registry can be shared
/// across threads if the binary ever grows one.
///
/// # Object safety
///
/// This trait is object-safe and is intended to be held as
/// `Box<dyn Tier2Builtin>` in the registry. The
/// `object_safety_compile_check` test enforces this.
pub trait Tier2Builtin: Send + Sync {
    /// Canonical name of the builtin (matched against `args[0]`
    /// after alias resolution).
    fn name(&self) -> &'static str;

    /// Aliases under which the builtin may also be invoked. The
    /// default `&[]` means "no aliases".
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    /// Run the builtin against the given context.
    ///
    /// # Errors
    ///
    /// Returns [`Tier2Error`] only for failures of the host
    /// machinery (I/O on the provided streams, detected invariant
    /// violations). A builtin that wants to report script-level
    /// failure returns `Ok(ExitStatus(n))` with non-zero `n`.
    fn invoke(&self, ctx: Tier2Ctx<'_>) -> Result<ExitStatus, Tier2Error>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::io;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    /// Compile-time check: `Tier2Builtin` must remain object-safe.
    /// If this line stops compiling, an edit broke object safety
    /// (e.g. added a generic method, returned `Self`, etc.).
    #[test]
    fn object_safety_compile_check() {
        fn _accepts_dyn(_b: &dyn Tier2Builtin) {}
        // Storing as Box<dyn _> exercises the vtable layout the
        // dispatcher will use.
        let _: Option<Box<dyn Tier2Builtin>> = None;
    }

    /// Compile-time check: `Tier2Builtin` is `Send + Sync`.
    #[test]
    fn trait_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn Tier2Builtin>();
    }

    /// A minimal builtin used to exercise the trait surface.
    struct EchoArgs;

    impl Tier2Builtin for EchoArgs {
        fn name(&self) -> &'static str {
            "echo-args"
        }

        fn aliases(&self) -> &'static [&'static str] {
            &["ea", "echoargs"]
        }

        fn invoke(&self, ctx: Tier2Ctx<'_>) -> Result<ExitStatus, Tier2Error> {
            for (i, a) in ctx.args.iter().enumerate() {
                if i > 0 {
                    ctx.stdout.write_all(b" ")?;
                }
                ctx.stdout.write_all(a.as_bytes())?;
            }
            ctx.stdout.write_all(b"\n")?;
            Ok(ExitStatus::SUCCESS)
        }
    }

    fn make_env() -> HashMap<String, String> {
        let mut e = HashMap::new();
        e.insert("FOO".to_owned(), "bar".to_owned());
        e
    }

    #[test]
    fn default_aliases_is_empty() {
        struct Bare;
        impl Tier2Builtin for Bare {
            fn name(&self) -> &'static str {
                "bare"
            }
            fn invoke(&self, _ctx: Tier2Ctx<'_>) -> Result<ExitStatus, Tier2Error> {
                Ok(ExitStatus::SUCCESS)
            }
        }
        assert_eq!(Bare.aliases(), &[] as &[&str]);
    }

    #[test]
    fn aliases_are_returned_in_declaration_order() {
        assert_eq!(EchoArgs.aliases(), &["ea", "echoargs"]);
    }

    #[test]
    fn invoke_via_dyn_trait_object() {
        let b: Box<dyn Tier2Builtin> = Box::new(EchoArgs);
        let args = vec!["echo-args".to_owned(), "hi".to_owned(), "there".to_owned()];
        let cwd = PathBuf::from("/tmp");
        let env = make_env();
        let mut stdin: &[u8] = b"";
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let cancellation = AtomicBool::new(false);

        let status = {
            let ctx = Tier2Ctx {
                args: &args,
                cwd: &cwd,
                env: &env,
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
                cancellation: &cancellation,
            };
            b.invoke(ctx).expect("invoke succeeds")
        };

        assert_eq!(status, ExitStatus::SUCCESS);
        assert_eq!(stdout, b"echo-args hi there\n");
        assert!(stderr.is_empty());
    }

    #[test]
    fn invoke_returns_non_zero_exit_without_error() {
        // Script-level failure: returns Ok with non-zero status,
        // NOT a Tier2Error.
        struct Failing;
        impl Tier2Builtin for Failing {
            fn name(&self) -> &'static str {
                "failing"
            }
            fn invoke(&self, _ctx: Tier2Ctx<'_>) -> Result<ExitStatus, Tier2Error> {
                Ok(ExitStatus(2))
            }
        }
        let args = vec!["failing".to_owned()];
        let cwd = PathBuf::from("/");
        let env = HashMap::new();
        let mut stdin: &[u8] = b"";
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();
        let cancellation = AtomicBool::new(false);

        let status = Failing
            .invoke(Tier2Ctx {
                args: &args,
                cwd: &cwd,
                env: &env,
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
                cancellation: &cancellation,
            })
            .expect("Ok with non-zero status");
        assert_eq!(status, ExitStatus(2));
        assert!(!status.is_success());
    }

    #[test]
    fn cancellation_flag_is_observable_by_builtin() {
        struct CheckCancel;
        impl Tier2Builtin for CheckCancel {
            fn name(&self) -> &'static str {
                "check-cancel"
            }
            fn invoke(&self, ctx: Tier2Ctx<'_>) -> Result<ExitStatus, Tier2Error> {
                if ctx.cancellation.load(Ordering::SeqCst) {
                    Ok(ExitStatus(130))
                } else {
                    Ok(ExitStatus::SUCCESS)
                }
            }
        }
        let args = vec!["check-cancel".to_owned()];
        let cwd = PathBuf::from("/");
        let env = HashMap::new();
        let cancellation = AtomicBool::new(true);
        let mut stdin: &[u8] = b"";
        let mut stdout: Vec<u8> = Vec::new();
        let mut stderr: Vec<u8> = Vec::new();

        let status = CheckCancel
            .invoke(Tier2Ctx {
                args: &args,
                cwd: &cwd,
                env: &env,
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
                cancellation: &cancellation,
            })
            .expect("ok");
        assert_eq!(status, ExitStatus(130));
    }

    #[test]
    fn tier2_error_display_host_io() {
        let err = Tier2Error::HostIo(io::Error::other("pipe burst"));
        assert_eq!(format!("{err}"), "host I/O failure");
        let source = std::error::Error::source(&err).expect("HostIo carries source");
        assert_eq!(source.to_string(), "pipe burst");
    }

    #[test]
    fn tier2_error_display_internal_invariant() {
        let err = Tier2Error::InternalInvariant { what: "args empty" };
        assert_eq!(format!("{err}"), "internal invariant violated: args empty");
        assert!(std::error::Error::source(&err).is_none());
    }

    #[test]
    fn from_io_error_for_tier2_error() {
        let io_err = io::Error::other("nope");
        let t: Tier2Error = io_err.into();
        match t {
            Tier2Error::HostIo(inner) => assert_eq!(inner.to_string(), "nope"),
            other => panic!("expected HostIo, got {other:?}"),
        }
    }

    #[test]
    fn debug_impl_is_present() {
        let _ = format!("{:?}", Tier2Error::InternalInvariant { what: "x" });
        let _ = format!("{:?}", Tier2Error::HostIo(io::Error::other("y")));
    }

    #[test]
    fn invoke_propagates_io_error_via_from() {
        // A builtin that hits an I/O error on stdout returns it via
        // the `?` operator thanks to From<io::Error> for Tier2Error.
        struct AlwaysWrites;
        impl Tier2Builtin for AlwaysWrites {
            fn name(&self) -> &'static str {
                "always-writes"
            }
            fn invoke(&self, ctx: Tier2Ctx<'_>) -> Result<ExitStatus, Tier2Error> {
                ctx.stdout.write_all(b"data")?;
                Ok(ExitStatus::SUCCESS)
            }
        }

        // A writer that always fails.
        struct FailingWriter;
        impl io::Write for FailingWriter {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("write refused"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let args = vec!["always-writes".to_owned()];
        let cwd = PathBuf::from("/");
        let env = HashMap::new();
        let mut stdin: &[u8] = b"";
        let mut stdout = FailingWriter;
        let mut stderr: Vec<u8> = Vec::new();
        let cancellation = AtomicBool::new(false);

        let err = AlwaysWrites
            .invoke(Tier2Ctx {
                args: &args,
                cwd: &cwd,
                env: &env,
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
                cancellation: &cancellation,
            })
            .expect_err("write should fail");
        match err {
            Tier2Error::HostIo(inner) => assert_eq!(inner.to_string(), "write refused"),
            other => panic!("expected HostIo, got {other:?}"),
        }
    }
}
