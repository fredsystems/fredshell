# fredshell spec corpus — reference toolchain

This file pins the **oracle** versions of `bash` and the GNU `coreutils`
that the bash-compat spec corpus is recorded and compared against
(per `PLAN_05` §4.5). It is consumed by `cargo xtask spec versions`,
which verifies that the toolchain configured in `flake.nix` matches
the values declared here and reports drift versus the floating
`nixos-unstable` channel as advisory output.

The host system's `bash` (e.g. macOS bash 3.2) is **never** used as
an oracle. Only the pinned versions below are authoritative.

## Pin (machine-readable)

The block below is parsed verbatim by `xtask::spec::versions`. Do
not reflow, reformat, or rename keys. Update the values when the
pin bumps; bumps are deliberate and are accompanied by re-recording
any affected fixtures in the same commit.

```toml
[reference]
bash = "5.3p9"
coreutils = "9.10"
nixpkgs_rev = "d233902339c02a9c334e7e593de68855ad26c4cb"
nixpkgs_input = "nixpkgs-reference"
pinned_on = "2026-05-21"
```

## Why pin

`nixos-unstable` rolls forward continuously. Without an explicit
pin, every checkout of fredshell would record fixtures against
whatever bash/coreutils happened to be current that week, and the
spec corpus would silently drift. The pin makes the oracle
reproducible: `nix build .#bashReference` at any future commit
yields exactly the bash that recorded the fixtures at the same
commit.

## Upgrade policy

The pin tracks upstream — the goal is "compat with the latest bash
and coreutils", not "compat with one frozen version forever". The
policy is therefore:

1. **Bumps are intentional.** Edit the `[reference]` block in this
   file _and_ the `nixpkgs-reference.url` rev in `flake.nix` in the
   same commit. `nix flake update nixpkgs-reference` will refresh
   the lock entry; capture the new rev here.
2. **Bumps re-record fixtures.** After the bump, run
   `cargo xtask spec record --all` (subcommand lands in PLAN_05
   subtask 05.7) and commit the regenerated fixtures alongside the
   pin change. Any case whose recorded output changes between
   versions is investigated, not silently overwritten — a fixture
   diff means real behavior changed in bash or coreutils.
3. **Drift is surfaced, not enforced.** `cargo xtask spec versions`
   compares the pinned versions to the floating `nixpkgs` input
   and prints `advisory: nixos-unstable has bash X (pinned: Y)`
   when they differ. CI does not fail on drift; the advisory is a
   reminder, not a gate.
4. **macOS bash is irrelevant.** Even on Darwin, the spec harness
   resolves bash through `FREDSHELL_REFERENCE_BASH` (exported by
   the nix devshell) so the system bash is bypassed. Outside the
   devshell, the harness fails loudly rather than falling back.

## Where the pin lives at runtime

The nix devshell exports the following environment variables
consumed by `cargo xtask spec versions` and (eventually) the
`fredshell-spec-runner` crate:

| Variable                                | Source                                | Purpose                                       |
| --------------------------------------- | ------------------------------------- | --------------------------------------------- |
| `FREDSHELL_REFERENCE_BASH`              | `nixpkgs-reference.bash`              | Absolute path to the pinned `bash` binary.    |
| `FREDSHELL_REFERENCE_COREUTILS`         | `nixpkgs-reference.coreutils`         | Absolute path to the pinned coreutils `bin/`. |
| `FREDSHELL_REFERENCE_BASH_VERSION`      | `nixpkgs-reference.bash.version`      | Version string for verification.              |
| `FREDSHELL_REFERENCE_COREUTILS_VERSION` | `nixpkgs-reference.coreutils.version` | Version string for verification.              |
| `FREDSHELL_FLOATING_BASH_VERSION`       | `nixpkgs.bash.version`                | Floating channel version (for drift report).  |
| `FREDSHELL_FLOATING_COREUTILS_VERSION`  | `nixpkgs.coreutils.version`           | Floating channel version (for drift report).  |

Outside the devshell the variables are absent; `cargo xtask spec
versions` refuses to run in that case. This is intentional: the
spec harness has no business operating against an unpinned oracle.

## Related plan sections

- `PLAN_05` §4.5 — reference toolchain rationale.
- `PLAN_05` §11.2 — coreutils manifest (versioned by the pin above).
- `PLAN_05` §5.4.7 — `cargo xtask spec record` (will respect this
  pin once it lands).
