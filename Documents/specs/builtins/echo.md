# `echo` — write arguments to standard output

> Status: draft
> Owner: `PLAN_07`
> Tier: 1
> Sources: bash 5.3 manual §"Bash Builtins" (`echo`); POSIX.1-2024 §"echo"
> Corpus: `tests/spec/builtins_tier1/echo_*.case.toml` (one per support row)
> Last updated: 2026-06-28

## 1. Synopsis

`echo [-neE] [arg ...]`

## 2. Description

`echo` writes its arguments to standard output, separated by single
spaces and followed by a newline. With no arguments it writes just a
newline. It is the most-used output builtin and the simplest one
whose behaviour is genuinely option-sensitive.

Three options control the output. `-n` suppresses the trailing
newline. `-e` enables interpretation of a fixed set of backslash
escape sequences (`\t`, `\n`, `\\`, and others). `-E` explicitly
disables escape interpretation, which is fredshell's default, so
`-E` only matters to override a contrary default. The options may be
bundled (`-ne`), and option parsing stops at the first argument that
is not a valid option bundle: any word that is not a run of `n`,
`e`, and `E` after a single leading `-` is treated as the first
literal argument, so `--`, `--help`, and `-x` are all printed
verbatim rather than acted on.

`echo` deliberately does _not_ honour `--` as an end-of-options
marker and does _not_ treat `--help` specially. This is bash's
documented behaviour and the reason robust scripts prefer `printf`
when an argument might begin with `-`.

This sheet describes fredshell's default (non-`--posix`,
`xpg_echo` off) `echo` behaviour. Execution is owned by PLAN_12
(Phase B); the `support` corpus cases below are recorded against
bash 5.3p9 and carry `status = "deferred:PLAN_12"` until the
implementation lands.

## 3. Support matrix

| #    | Behaviour                                                                                        | Classification | Corpus                                          |
| ---- | ------------------------------------------------------------------------------------------------ | -------------- | ----------------------------------------------- |
| 3.1  | `echo a b c` writes its arguments separated by single spaces and a trailing newline              | support        | `builtins_tier1/echo_basic.case.toml`           |
| 3.2  | `echo` with no arguments writes a single newline                                                 | support        | `builtins_tier1/echo_empty.case.toml`           |
| 3.3  | `echo -n x` suppresses the trailing newline                                                      | support        | `builtins_tier1/echo_n_flag.case.toml`          |
| 3.4  | `echo -e "a\tb"` interprets backslash escapes when `-e` is given                                 | support        | `builtins_tier1/echo_e_flag.case.toml`          |
| 3.5  | `echo "a\tb"` leaves backslash sequences literal by default (`-E` is the default)                | support        | `builtins_tier1/echo_default_literal.case.toml` |
| 3.6  | `echo -ne x` parses bundled options as a single `-ne` group                                      | support        | `builtins_tier1/echo_bundled_flags.case.toml`   |
| 3.7  | `echo -nx hi` stops option parsing at an invalid bundle and prints `-nx` literally               | support        | `builtins_tier1/echo_invalid_bundle.case.toml`  |
| 3.8  | `echo -- --help -x` prints `--`, `--help`, and `-x` verbatim (no end-of-options / help handling) | support        | `builtins_tier1/echo_literal_dashes.case.toml`  |
| 3.9  | `echo -e` interprets `\t \n \r \\ \a \b \e \f \v` to their control characters                    | support        | `builtins_tier1/echo_e_escapes.case.toml`       |
| 3.10 | `echo -e "keep\cdrop"` halts output at `\c`, suppressing the rest and the trailing newline       | support        | `builtins_tier1/echo_e_suppress.case.toml`      |
| 3.11 | `echo -e "\0101"` interprets a leading-zero octal escape `\0nnn`                                 | support        | `builtins_tier1/echo_e_octal.case.toml`         |
| 3.12 | `echo -e "\x41"` interprets a hexadecimal escape `\xHH`                                          | support        | `builtins_tier1/echo_e_hex.case.toml`           |
| 3.13 | `echo -e "\u00e9"` / `\UHHHHHHHH` interpret Unicode escapes (locale-dependent)                   | defer:5        | n/a                                             |
| 3.14 | `echo "a\tb"` interprets escapes by default when the `xpg_echo` shell option is set              | defer:4        | n/a                                             |

## 4. Bash quirks

1. **`echo` does not recognise `--` or `--help`.** Unlike most
   builtins, `echo` has no end-of-options marker and no help option:
   `echo --help` prints the string `--help` (rows 3.8). Option
   parsing stops at the first word that is not a bundle of `n`, `e`,
   `E` after a single `-`, so `echo -nx hi` prints `-nx hi`
   literally (row 3.7). Scripts that need to print an arbitrary
   leading-dash string reach for `printf '%s\n'` instead.
2. **`-E` is the default, so it is usually a no-op.** fredshell, like
   bash with `xpg_echo` off, does not interpret escapes unless `-e`
   is given (row 3.5). `-E` exists only to re-assert that default
   when something (an alias, `xpg_echo`, or a `--posix`-leaning
   environment) would otherwise enable escapes.
3. **`\c` truncates the entire output.** Within `-e` interpretation,
   `\c` stops processing immediately: everything after it is dropped
   and the trailing newline is suppressed (row 3.10). It is the one
   escape that affects output structure rather than emitting a
   character.
4. **Octal escapes require a leading zero.** `echo -e` spells an
   octal byte as `\0nnn` (e.g. `\0101` is `A`), distinct from
   `printf`, whose octal escape is `\nnn` with no leading zero
   (row 3.11). Mixing the two conventions up is a common bug.

## 5. Wontfix rationale

None. Every row in §3 is classified `support` or `defer`.

## 6. Deferred rows

- **3.13 — `\u` / `\U` Unicode escapes.** Deferred to milestone 5
  (UTF-8 / locale correctness). The behaviour is well defined, but
  it is locale-dependent: bash only emits the multibyte UTF-8
  encoding under a UTF-8 `LC_CTYPE`, and in the `C`/`POSIX` locale it
  prints the escape verbatim. The spec recorder runs with a cleared
  environment (`env_clear()`, effectively the `C` locale; PLAN_05
  §4), so a hermetic `support` case asserting multibyte output cannot
  yet be recorded — these escapes belong to the UTF-8/locale
  category (PLAN_07 §2.2), whose cases live in
  `tests/spec/utf8_locale/` and are recorded under an explicit UTF-8
  locale once the runner supports it. Workaround: use `printf` with
  an explicit byte sequence, or `$'\u00e9'` ANSI-C quoting, when
  Unicode output is required before milestone 5.
- **3.14 — `xpg_echo` default escape interpretation.** Deferred to
  milestone 4. Requires the `shopt -s xpg_echo` option to be
  implemented first (owned by the `shopt` sheet, batch 1). When
  `xpg_echo` is set, `echo` interprets escapes without `-e`, matching
  XPG/SysV `echo`. Workaround: pass `-e` explicitly to interpret
  escapes regardless of the option's state.

## 7. POSIX divergence

POSIX `echo` does not define the `-n`, `-e`, or `-E` options at all:
under a strict POSIX implementation, `echo -n` would print the
string `-n`. Bash in its default mode honours `-n`/`-e`/`-E` as
options (rows 3.3–3.6); under `set -o posix` _and_ `xpg_echo`, bash
shifts toward the XPG behaviour where escapes are interpreted by
default and options are not recognised. fredshell follows bash's
default (non-`--posix`, `xpg_echo` off) behaviour, which is the
contract here. `--posix` mode is not a v1 target (PLAN_07 §2.4);
this subsection is informational.

## 8. References

- Bash reference manual §"Bash Builtins" / `help echo` (bash 5.3p9).
- POSIX.1-2024 ("Issue 8") shell command language, `echo` utility.
- Owning implementation: `Documents/PLAN_12_exec_phase_b.md`
  (Phase B executor; `echo` builtin).
- `Documents/PLAN_07_spec_drafting.md` §5 (classification semantics),
  §2.2 (UTF-8/locale category for `\u`/`\U`).
