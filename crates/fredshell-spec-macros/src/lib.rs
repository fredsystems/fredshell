// Copyright (C) 2026 Fred Clausen
// Use of this source code is governed by an MIT-style
// license that can be found in the LICENSE file or at
// https://opensource.org/licenses/MIT.

//! Proc-macros tying fredshell builtins to spec sheets
//! (`PLAN_07` §8.2).
//!
//! The crate exposes a single macro, [`refuse!`], which a builtin
//! uses to refuse a behaviour a spec sheet classifies as `wontfix` or
//! `defer:N`. The macro reads the referenced spec sheet **at compile
//! time** and:
//!
//! * fails to compile if the sheet does not exist,
//! * fails to compile if the named §3 row does not exist,
//! * fails to compile if the row's classification does not match the
//!   refusal form (`wontfix` row for `refuse!(wontfix, …)`, `defer:N`
//!   row for `refuse!(defer, …)`), and
//! * otherwise expands to a `::fredshell_core::Refusal` struct literal
//!   carrying the row's summary and the rendered `See:` path.
//!
//! This is the compile-time link between prose and code described in
//! `PLAN_07` §8.2: a misspelled row id, a deleted row, or a row whose
//! classification flipped all fail at the call site rather than in CI.
//!
//! # Grammar
//!
//! ```ignore
//! // Permanent refusal:
//! refuse!(wontfix, "cd", "3.7")
//!
//! // Temporary refusal. The milestone number N is read from the
//! // sheet's `defer:N` classification; the milestone name and
//! // workaround live in §6 prose (not machine-extractable), so they
//! // are passed as named arguments:
//! refuse!(
//!     defer, "cd", "3.9",
//!     milestone_name = "filesystem-touch builtins",
//!     workaround = "Use `cd && ls` for now"
//! )
//! ```
//!
//! Both forms expand to an expression of type
//! `::fredshell_core::Refusal`; the caller decides whether to `return`
//! it or wrap it.

use std::fs;
use std::path::{Path, PathBuf};

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token};

mod sheet;

/// Parsed form of a `refuse!` invocation.
enum RefuseInput {
    Wontfix {
        sheet_id: LitStr,
        row: LitStr,
    },
    Defer {
        sheet_id: LitStr,
        row: LitStr,
        milestone_name: LitStr,
        workaround: LitStr,
    },
}

impl Parse for RefuseInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let kind: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let sheet_id: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let row: LitStr = input.parse()?;

        match kind.to_string().as_str() {
            "wontfix" => Ok(Self::Wontfix { sheet_id, row }),
            "defer" => {
                input.parse::<Token![,]>()?;
                let (mut milestone_name, mut workaround) = (None, None);
                // Parse the two `key = "value"` named arguments in any
                // order.
                for _ in 0..2 {
                    let key: Ident = input.parse()?;
                    input.parse::<Token![=]>()?;
                    let value: LitStr = input.parse()?;
                    match key.to_string().as_str() {
                        "milestone_name" => milestone_name = Some(value),
                        "workaround" => workaround = Some(value),
                        other => {
                            return Err(syn::Error::new(
                                key.span(),
                                format!(
                                    "unknown named argument `{other}`; expected \
                                     `milestone_name` or `workaround`"
                                ),
                            ));
                        }
                    }
                    if input.peek(Token![,]) {
                        input.parse::<Token![,]>()?;
                    }
                }
                let milestone_name = milestone_name.ok_or_else(|| {
                    syn::Error::new(
                        kind.span(),
                        "`refuse!(defer, …)` requires a `milestone_name = \"…\"` argument",
                    )
                })?;
                let workaround = workaround.ok_or_else(|| {
                    syn::Error::new(
                        kind.span(),
                        "`refuse!(defer, …)` requires a `workaround = \"…\"` argument",
                    )
                })?;
                Ok(Self::Defer {
                    sheet_id,
                    row,
                    milestone_name,
                    workaround,
                })
            }
            other => Err(syn::Error::new(
                kind.span(),
                format!("expected `wontfix` or `defer`, found `{other}`"),
            )),
        }
    }
}

/// Refuse a spec-sheet behaviour, validating the sheet row and
/// classification at compile time. See the crate docs for the grammar.
#[proc_macro]
pub fn refuse(input: TokenStream) -> TokenStream {
    let parsed = syn::parse_macro_input!(input as RefuseInput);
    match expand(&parsed) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// The classification a refusal form requires of its §3 row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum WantClass {
    Wontfix,
    Defer,
}

fn expand(input: &RefuseInput) -> syn::Result<proc_macro2::TokenStream> {
    match input {
        RefuseInput::Wontfix { sheet_id, row } => {
            let resolved = resolve(sheet_id, row, WantClass::Wontfix)?;
            let sheet_id_s = resolved.sheet_id;
            let row_s = resolved.row;
            let summary = resolved.summary;
            let sheet_path = resolved.sheet_path;
            Ok(quote! {
                ::fredshell_core::Refusal {
                    sheet_id: #sheet_id_s.to_owned(),
                    row: #row_s.to_owned(),
                    summary: #summary.to_owned(),
                    sheet_path: #sheet_path.to_owned(),
                    section: #row_s.to_owned(),
                    kind: ::fredshell_core::RefusalKind::Wontfix,
                }
            })
        }
        RefuseInput::Defer {
            sheet_id,
            row,
            milestone_name,
            workaround,
        } => {
            let resolved = resolve(sheet_id, row, WantClass::Defer)?;
            let milestone = resolved.milestone.ok_or_else(|| {
                syn::Error::new(
                    row.span(),
                    "internal: defer row resolved without a milestone number",
                )
            })?;
            let sheet_id_s = resolved.sheet_id;
            let row_s = resolved.row;
            let summary = resolved.summary;
            let sheet_path = resolved.sheet_path;
            let milestone_name_s = milestone_name.value();
            let workaround_s = workaround.value();
            Ok(quote! {
                ::fredshell_core::Refusal {
                    sheet_id: #sheet_id_s.to_owned(),
                    row: #row_s.to_owned(),
                    summary: #summary.to_owned(),
                    sheet_path: #sheet_path.to_owned(),
                    section: #row_s.to_owned(),
                    kind: ::fredshell_core::RefusalKind::Defer {
                        milestone: #milestone.to_owned(),
                        milestone_name: #milestone_name_s.to_owned(),
                        workaround: #workaround_s.to_owned(),
                    },
                }
            })
        }
    }
}

/// The fields extracted from a sheet for a single `refuse!` call.
struct Resolved {
    sheet_id: String,
    row: String,
    summary: String,
    sheet_path: String,
    /// `Some(N)` for a `defer:N` row, `None` for `wontfix`.
    milestone: Option<String>,
}

/// Locate the sheet, parse it, find the row, validate the
/// classification, and extract the fields. All failures surface as
/// `syn::Error` anchored at the relevant literal's span so the
/// diagnostic points at the offending argument.
fn resolve(sheet_id: &LitStr, row: &LitStr, want: WantClass) -> syn::Result<Resolved> {
    let id = sheet_id.value();
    let row_no = row.value();

    let (sheet_path, body) = read_sheet(&id).map_err(|e| syn::Error::new(sheet_id.span(), e))?;

    let table = sheet::parse_support_matrix(&body);
    let parsed_row = table.iter().find(|r| r.number == row_no).ok_or_else(|| {
        syn::Error::new(
            row.span(),
            format!("spec sheet `{sheet_path}` has no §3 row `{row_no}`"),
        )
    })?;

    match (&parsed_row.classification, want) {
        (sheet::Classification::Wontfix, WantClass::Wontfix) => Ok(Resolved {
            sheet_id: id,
            row: row_no,
            summary: parsed_row.summary.clone(),
            sheet_path,
            milestone: None,
        }),
        (sheet::Classification::Defer(n), WantClass::Defer) => Ok(Resolved {
            sheet_id: id,
            row: row_no,
            summary: parsed_row.summary.clone(),
            sheet_path,
            milestone: Some(n.clone()),
        }),
        (actual, _) => Err(syn::Error::new(
            row.span(),
            format!(
                "spec sheet `{sheet_path}` row `{row_no}` is classified `{}`, \
                 but `refuse!({}, …)` requires a `{}` row",
                actual.label(),
                match want {
                    WantClass::Wontfix => "wontfix",
                    WantClass::Defer => "defer",
                },
                match want {
                    WantClass::Wontfix => "wontfix",
                    WantClass::Defer => "defer:N",
                },
            ),
        )),
    }
}

/// Find and read the sheet for `id`, returning the workspace-relative
/// path and the body. Looks under `builtins/` then `features/`.
fn read_sheet(id: &str) -> Result<(String, String), String> {
    let specs_root = find_specs_root()
        .ok_or_else(|| "could not locate `Documents/specs/` from CARGO_MANIFEST_DIR".to_owned())?;

    for subdir in ["builtins", "features"] {
        let candidate = specs_root.join(subdir).join(format!("{id}.md"));
        if candidate.is_file() {
            let body = fs::read_to_string(&candidate)
                .map_err(|e| format!("failed to read `{}`: {e}", candidate.display()))?;
            let rel = format!("Documents/specs/{subdir}/{id}.md");
            return Ok((rel, body));
        }
    }
    Err(format!(
        "no spec sheet for `{id}` under Documents/specs/builtins/ or \
         Documents/specs/features/"
    ))
}

/// Locate the `Documents/specs/` tree.
///
/// If `FREDSHELL_SPECS_ROOT` is set, it is used verbatim as the specs
/// root (it must point at the directory that directly contains
/// `builtins/` and `features/`). This override exists so the macro is
/// hermetically testable against a fixture tree and so out-of-tree
/// embedders can relocate the sheets.
///
/// Otherwise we ascend from `CARGO_MANIFEST_DIR` looking for a
/// directory that contains `Documents/specs/`. Proc-macros cannot
/// reliably know the workspace root, so we walk up until we find the
/// specs tree.
fn find_specs_root() -> Option<PathBuf> {
    if let Some(override_root) = std::env::var_os("FREDSHELL_SPECS_ROOT") {
        let path = PathBuf::from(override_root);
        return path.is_dir().then_some(path);
    }
    let start = std::env::var_os("CARGO_MANIFEST_DIR")?;
    let mut dir: &Path = Path::new(&start);
    loop {
        let candidate = dir.join("Documents").join("specs");
        if candidate.is_dir() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}
