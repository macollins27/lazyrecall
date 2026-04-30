# Contributing

PRs welcome. The codebase is small; read [ARCHITECTURE.md](ARCHITECTURE.md) first — that's the guided tour.

## Setup

Requirements: Rust stable (1.75+).

```bash
git clone https://github.com/macollins27/lazyrecall
cd lazyrecall
cargo build
cargo test
```

## Before opening a PR

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test
```

CI will run these on macOS + Linux. If they pass locally, they'll pass in CI.

## Commit style

- One logical change per commit; small commits over big ones.
- Subject in imperative mood ("add", "fix", "rename"), not past tense.
- If a commit fixes a bug, explain *why* the bug existed in the body, not just what changed.
- No bot tags; sign-off optional.

## Where to start

Good first issues, in order of size:

1. **A new keybinding** — `lazyrecall-tui/src/input.rs`. One match arm.
2. **A new color or status indicator** — `lazyrecall-tui/src/theme.rs` + the relevant `draw_*` function.
3. **A new test for an existing function** — pick a function in `parser.rs`, `index.rs`, or `discovery.rs` that doesn't have one and add it.
4. **A V1.5 feature from the README roadmap** — open an issue first to scope.

## Code conventions

- **No comments that restate code.** Only add a comment when *why* is non-obvious (a constraint, a workaround, a subtle invariant).
- **Default to no `unwrap()`.** Use `?` and the typed error enum. The TUI keeps `anyhow` for ergonomics; the library uses `lazyrecall_core::Error`.
- **Tests are part of the contract**, not a chore. The three parser tests lock in the JSONL filter rules — if you change the parser, update them in the same commit.
- **Schema changes = migrations.** Never edit `SCHEMA_V1`. Bump `SCHEMA_VERSION` and add a `SCHEMA_VN` block. See `ARCHITECTURE.md`.

## Reporting bugs

Include:
- The output of `lazyrecall --version` (or commit SHA if from source).
- The relevant lines from `~/.lazyrecall/log`.
- Whether `ANTHROPIC_API_KEY` is set or you're falling back to `~/.lazyrecall/api-key`.
- The OS + terminal emulator (Alacritty, iTerm2, etc.).
