# `cd` — change the working directory

> Status: draft
> Owner: PLAN_07
> Tier: 1
> Sources: bash 5.3 manual §"cd"; POSIX.1-2024 §"cd"
> Corpus: tests/spec/builtins/cd_dir.case.toml (one per support row)
> Last updated: 2026-06-28

## 1. Synopsis

`cd [-L|[-P [-e]] [-@]] [dir]`

## 2. Description

Test fixture sheet for the `refuse!` macro integration tests. Not a
real spec sheet; it exists only to give the proc-macro a sheet to read
at compile time. The real `cd` sheet lands with PLAN_07 subtask 08.2.

## 3. Support matrix

| #   | Behaviour                                                                      | Classification | Corpus                      |
| --- | ------------------------------------------------------------------------------ | -------------- | --------------------------- |
| 3.1 | `cd dir` changes to `dir`                                                      | support        | `builtins/cd_dir.case.toml` |
| 3.7 | option `-@` (extended attributes) is not supported and will not be implemented | wontfix        | n/a — see §5                |
| 3.9 | option `-e`                                                                    | defer:3        | n/a                         |

## 4. Bash quirks

Fixture — no quirks documented.

## 5. Wontfix rationale

Fixture — `-@` is refused because the extended-attribute walk is a
platform-specific surface fredshell does not replicate.

## 6. Deferred rows

Fixture — `-e` is deferred to milestone 3 (filesystem-touch builtins).
Use `cd && ls` for now.

## 8. References

- Fixture sheet; no external references.
