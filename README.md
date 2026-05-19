# fredshell

An opinionated, batteries-included Rust shell. The goal is a daily-driver
replacement for zsh with a baked-in starship-style prompt, fzf-powered
history and completion, `lsd` semantics for `ls`, and optional AI helpers.

> **Status:** very early. The REPL currently shells out to `/bin/sh -c`
> for everything it can't yet handle natively.

## Quick start

With Nix + direnv:

```bash
direnv allow
cargo run -p fredshell
```

Without Nix:

```bash
cargo run -p fredshell
```

## Layout

- `crates/fredshell` — binary entrypoint, CLI, reedline glue.
- `crates/fredshell-core` — builtins, exec, REPL state.
- `crates/fredshell-prompt` — starship-style prompt renderer.
- `xtask` — project automation (`cargo xtask check`, `coverage`, ...).
- `nix/` — overlay + home-manager module.
- `flake.nix` — dev shell + package + checks.

## Roadmap

See the conversation that bootstrapped this repo. MVP target:

1. Reedline REPL with starship-subset prompt.
2. Builtins (`cd`, `exit`, `export`, `alias`, `history`).
3. Native fork/exec for simple pipelines, `/bin/sh -c` fallback otherwise.
4. fzf-style history (Ctrl-R) backed by nucleo.
5. `lsd` built in as the default `ls`.
6. Nix flake + home-manager module (already present in skeleton form).
7. Optional AI helpers (natural-language → command, error explanation).

## License

MIT — see [LICENSE](LICENSE).
