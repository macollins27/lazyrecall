# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this project is

`recall` is a lazygit-style TUI for browsing, searching, and resuming Claude Code sessions. It reads (read-only) the JSONL transcripts that every `claude` process writes under `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl`, indexes them into a local SQLite cache, auto-summarizes each session via Haiku 4.5, and one-keystroke-resumes via `claude --resume <id>`.

V1 is feature-complete (per README) but not yet manually validated end-to-end — `cargo check` greens are necessary but not sufficient. Treat first-run smoke testing as still pending.

## Common commands

```bash
# Run the TUI (debug build, fastest to iterate)
cargo run -p recall-tui

# Release build (~1-2 min first time, ships the `recall` binary)
cargo run --release -p recall-tui

# Type-check the whole workspace without producing a binary
cargo check

# Run all tests (parser tests live in crates/recall-core/src/parser.rs)
cargo test

# Run a single test by name
cargo test -p recall-core metadata_extracts_cwd_counts_and_filters_meta_and_sidechain

# Lint
cargo clippy --all-targets
```

For the summarizer to run, either `export ANTHROPIC_API_KEY=...` or write the key (one line) to `~/.recall/api-key`. Without a key the TUI works fine — summaries just stay blank.

## Architecture

Two-crate Cargo workspace:

- `crates/recall-core` — library: `discovery`, `parser`, `index`, `summarizer`, `summarizer_worker`, `watcher`. Re-exports the public surface from `lib.rs`.
- `crates/recall-tui` — binary named `recall`: ratatui frontend in a single `main.rs`.

The split exists so a future Tauri GUI (V4 on the roadmap) can reuse `recall-core` without a rewrite. Keep TUI-specific logic out of `recall-core`.

### Threading model (load-bearing)

Three threads run concurrently, each with its own `rusqlite::Connection` to the same `~/.recall/index.db`:

1. **Main / TUI thread** — owns the ratatui event loop and an `Index`.
2. **Summarizer worker** — `std::thread::spawn` in `main.rs` that builds a current-thread tokio runtime and runs `summarizer_worker::run`. Polls `index.missing_summaries()`, calls Haiku, writes back via `index.set_summary()`.
3. **FS watcher** — `std::thread::spawn` running `watcher::run` over `~/.claude/projects/` recursively (notify / FSEvents). On any new/modified `.jsonl`, calls `index.touch_session()`.

The reason each thread opens its own `Index`: `rusqlite::Connection` is `Send` but not `Sync`, so it can't be shared across threads. Multiple connections to the same SQLite file are fine. **Don't try to wrap the index in `Arc<Mutex<…>>` and share it** — open another connection instead.

There is no cross-thread channel. Workers write to the index; the TUI re-reads the index on a periodic tick (`TICKS_PER_REFRESH = 25` at 200 ms poll → every ~5 s) via `App::refresh_index_state`.

### Resume flow

The TUI does **not** spawn `claude` as a child. On Enter in the Sessions pane, it sets `app.resume_request`, exits the run loop, restores the terminal, and then calls `Command::new("claude").arg("--resume").arg(id).exec()` (Unix `exec`) so the `claude` process **replaces** the recall process. This is why the resume call lives at the end of `main()` after terminal cleanup, not inside the event loop.

### JSONL parsing rules

Claude Code's JSONL has many event types; recall only cares about `user`, `assistant`, and `system`. Two filters are applied everywhere events are surfaced:

- `isSidechain: true` events (subagent traffic) are **skipped**.
- `isMeta: true` events (meta noise) are **skipped**.

Three tests in `crates/recall-core/src/parser.rs` lock this schema in. If you change the parser, update those tests in the same change — they are the V1 contract.

### encoded-cwd is lossy

The directory name `~/.claude/projects/{encoded-cwd}/` is the original cwd with `/` replaced by `-`. This is **not reversible** (a real path can contain `-`). Always recover the real cwd by reading the `cwd` field from the JSONL events themselves (see `discovery::inspect_newest_session` and `parser::parse_metadata`). Never try to "decode" the directory name.

### Index schema is versioned from day 1

`crates/recall-core/src/index.rs` has a `schema_version` table and a `SCHEMA_VERSION` const set to `1`. When you change the schema, bump the const and add a migration step in `Index::migrate` — don't just edit `SCHEMA_V1`. The day-1 versioning is deliberate so V2+ migrations don't paint us into a corner.

### Summarizer input is tail-truncated

Sessions can be hundreds of MB. `summarizer_worker::truncate_tail` keeps only the last `MAX_INPUT_CHARS = 30_000` characters before sending to Haiku, on the theory that the tail best represents what was achieved. Don't switch to head-truncation without thinking about this.

## Stack

Rust 2021. `ratatui` 0.28 + `crossterm` 0.28 (TUI). `rusqlite` 0.31 with bundled SQLite (so no system libsqlite dep). `notify` 6 (file watcher). `reqwest` 0.12 with `rustls-tls` (Anthropic Messages API; no OpenSSL dep). `tokio` 1 multi-thread runtime, but only the summarizer worker thread actually uses async.

## Data locations

- `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl` — Claude Code's source of truth. **Read-only** from recall's perspective; never write here.
- `~/.recall/index.db` — recall's SQLite cache. Safe to delete; regenerates on next run.
- `~/.recall/api-key` — optional fallback API key file (one line).
