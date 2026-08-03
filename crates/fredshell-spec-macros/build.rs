// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Build script for `fredshell-spec-macros`.
//!
//! Its sole job is to expose the crate's test-fixture spec tree to
//! the integration tests. `refuse!` reads spec sheets at *expansion*
//! time via `std::env::var("FREDSHELL_SPECS_ROOT")` (or, in
//! production, by ascending to the workspace `Documents/specs/`). The
//! integration tests under `tests/` need the macro to resolve against
//! the fixture sheets in `tests/specs/` instead. `cargo:rustc-env`
//! sets an environment variable for the `rustc` invocations that
//! compile this crate and its test targets, which is exactly the
//! environment the proc-macro observes when it expands `refuse!` in a
//! test file.
//!
//! A build script cannot tell a test build from a non-test build, so
//! the variable is emitted unconditionally. This only affects builds
//! of `fredshell-spec-macros` itself (and its own test targets);
//! dependent crates such as `fredshell-core` do not run this script,
//! so their `refuse!` expansions fall back to the workspace ascend
//! and resolve against the real `Documents/specs/` sheets.

use std::path::Path;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let fixture_root = Path::new(&manifest_dir).join("tests").join("specs");
    println!("cargo:rerun-if-changed=tests/specs");
    println!(
        "cargo:rustc-env=FREDSHELL_SPECS_ROOT={}",
        fixture_root.display()
    );
}
