# `cd` — change the working directory

> Status: draft
> Owner: `PLAN_07`
> Tier: 1
> Sources: bash 5.3 manual §"cd"; POSIX.1-2024 §"cd"
> Corpus: `tests/spec/builtins_tier1/cd_*.case.toml` (one per support row)
> Last updated: 2026-06-28

## 1. Synopsis

`cd [-L|[-P [-e]] [-@]] [dir]`

## 2. Description

`cd` changes the shell's current working directory. With no argument
it changes to the directory named by `$HOME`; with the single
argument `-` it changes to `$OLDPWD` and prints the new directory.

When `dir` does not begin with a slash, `cd` searches the
colon-separated directories named by `$CDPATH` **before** falling back
to interpreting `dir` relative to `$PWD`. The entries are tried in
order, and the first one that contains `dir` wins — so a `$CDPATH`
entry shadows a same-named subdirectory of the current directory. An
empty entry, and an explicit `.` entry, both denote the current
directory, which is how a user restores the "current directory first"
behaviour. If no `$CDPATH` entry matches, `cd` falls back to the plain
relative interpretation. An absolute `dir` bypasses `$CDPATH`
entirely.

By default `cd` follows symbolic links logically (`-L`): symbolic
links in `dir` are kept in `$PWD` and `..` is processed textually.
`-P` instead resolves symbolic links physically before processing
`..`. `cd` updates `$PWD` and `$OLDPWD` on success.

This sheet describes fredshell's default (non-`--posix`) `cd`
behaviour. Execution is owned by PLAN_12 (Phase B); the `support`
corpus cases below are recorded against bash 5.3p9 and carry
`status = "deferred:PLAN_12"` until the implementation lands.

## 3. Support matrix

| #    | Behaviour                                                                             | Classification | Corpus                                           |
| ---- | ------------------------------------------------------------------------------------- | -------------- | ------------------------------------------------ |
| 3.1  | `cd dir` changes the working directory to `dir`                                       | support        | `builtins_tier1/cd_to_dir.case.toml`             |
| 3.2  | `cd` with no argument changes to `$HOME`                                              | support        | `builtins_tier1/cd_no_args_home.case.toml`       |
| 3.3  | `cd -` changes to `$OLDPWD`                                                           | support        | `builtins_tier1/cd_dash_oldpwd.case.toml`        |
| 3.4  | `cd ..` moves to the parent directory                                                 | support        | `builtins_tier1/cd_parent.case.toml`             |
| 3.5  | `cd` into a nonexistent directory fails with exit status 1 and a diagnostic           | support        | `builtins_tier1/cd_nonexistent.case.toml`        |
| 3.6  | `cd a b` with too many arguments fails with exit status 2 and a diagnostic            | support        | `builtins_tier1/cd_too_many_args.case.toml`      |
| 3.7  | `cd dir` searches `$CDPATH` before `$PWD`, so a `$CDPATH` entry shadows `./dir`       | support        | `builtins_tier1/cd_cdpath.case.toml`             |
| 3.13 | An empty `$CDPATH` entry (as in `CDPATH=:/other`) denotes the current directory       | support        | `builtins_tier1/cd_cdpath_empty_entry.case.toml` |
| 3.8  | `cd -L dir` follows symbolic links logically (the default)                            | defer:3        | n/a                                              |
| 3.9  | `cd -P dir` resolves symbolic links physically                                        | defer:3        | n/a                                              |
| 3.10 | `cd -e` (with `-P`) exits non-zero when the physical cwd cannot be determined         | defer:3        | n/a                                              |
| 3.11 | `cd -@` presents a file's extended attributes as a directory                          | wontfix        | n/a — see §5                                     |
| 3.12 | `cd word` treats `word` as a variable name when the `cdable_vars` shell option is set | defer:4        | n/a                                              |

## 4. Bash quirks

1. **`cd -` prints the destination.** Unlike a plain `cd dir`, the
   `cd -` form writes the resolved directory to stdout (rows 3.3).
   POSIX permits this; bash always does it. Scripts that capture
   `cd -` output rely on it.
2. **`$CDPATH` echoes the resolved path.** When a target is found via
   `$CDPATH` rather than relative to `$PWD`, bash prints the resolved
   absolute path to stdout (row 3.7). The corpus case suppresses that
   line so the assertion is sandbox-independent, but the quirk is
   real and load-bearing for interactive use.
3. **`$CDPATH` shadows the current directory, which surprises people.**
   Because `$CDPATH` is searched _before_ the plain relative
   interpretation (row 3.7), setting `CDPATH=/somewhere` silently
   changes what `cd subdir` means inside a project that has its own
   `subdir`. This is why `$CDPATH` is conventionally written with a
   leading colon (`CDPATH=:/somewhere`), whose empty first entry
   restores current-directory-first behaviour (row 3.13). Scripts that
   inherit a user's `$CDPATH` and then `cd` to a relative path are
   relying on this ordering whether they know it or not, which is the
   usual argument for `cd -- "$dir"` with an absolute path in scripts.
4. **Too-many-arguments is a usage error, not a no-op.** `cd a b`
   exits 2 with a diagnostic (row 3.6), distinct from the exit-1
   "directory not found" failure (row 3.5).

## 5. Wontfix rationale

`cd -@` (row 3.11) presents a file's extended attributes as a
pseudo-directory on systems that support it. fredshell will not
implement it: the feature is platform-specific (it depends on the
host's extended-attribute surface), it is exercised by effectively no
real-world scripts, and emulating it would couple `cd` to a
filesystem capability fredshell otherwise has no reason to model.
Users who need extended-attribute access should use a dedicated tool
(`getfattr`, `attr`) rather than `cd`.

## 6. Deferred rows

- **3.8 / 3.9 — `-L` / `-P` symbolic-link resolution.** Deferred to
  milestone 3. The behaviours are well defined, but the spec corpus
  cannot yet exercise them: the spec-runner's sandbox skeleton copier
  (`copy_dir_recursive`) intentionally skips symbolic links in v0
  (a documented limitation, revisited under PLAN_11), so a hermetic
  `support` case with a pre-existing symlink cannot be recorded. The
  rows flip to `support` with corpus cases once the runner gains
  symlink-skeleton support. Until then, `cd dir` without a symlink in
  the path (row 3.1) behaves identically under `-L` and `-P`.
- **3.10 — `cd -e`.** Deferred to milestone 3. Requires detecting
  that the physical working directory cannot be determined after a
  `-P` resolve, an edge that depends on the same physical-resolution
  machinery as 3.9. Workaround: run `cd -P dir && pwd -P` and check
  the exit status of `pwd -P` for now.
- **3.12 — `cdable_vars`.** Deferred to milestone 4. Requires the
  `shopt -s cdable_vars` option to be implemented first (owned by the
  `shopt` sheet, batch 1). Workaround: expand the variable explicitly
  with `cd "$var"`.

## 8. References

- Bash reference manual §"Bourne Shell Builtins" / `help cd`
  (bash 5.3p9).
- POSIX.1-2024 ("Issue 8") shell command language, `cd` utility.
- Owning implementation: `Documents/PLAN_12_exec_phase_b.md`
  (Phase B executor; `cd` builtin).
- `Documents/PLAN_07_spec_drafting.md` §5 (classification semantics).
