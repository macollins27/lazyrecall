# lazyrecall

> The memory layer Claude Code is missing. A lazygit-style TUI for browsing, searching, and resuming every session you've ever had with `claude`.

[![CI](https://github.com/macollins27/lazyrecall/actions/workflows/ci.yml/badge.svg)](https://github.com/macollins27/lazyrecall/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)

![demo](docs/demo.gif)

## Why

Every `claude` process writes a JSONL transcript to `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl`, and `claude --resume <id>` brings the entire conversation + agent state back. The data is there; the UX is hidden behind a flag and a UUID.

`lazyrecall` is the missing front end:

- Browse every session across every project, sorted by recency
- See an auto-generated one-line summary inline next to each session
- Press `Enter` and you're dropped right back into the conversation, in the right working directory

## Install

### One-line (recommended)

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/macollins27/lazyrecall/releases/latest/download/lazyrecall-installer.sh | sh
```

### Homebrew

```bash
brew install macollins27/lazyrecall/lazyrecall
```

### Cargo

```bash
cargo install --git https://github.com/macollins27/lazyrecall lazyrecall-tui
```

### From source

```bash
git clone https://github.com/macollins27/lazyrecall
cd lazyrecall
cargo install --path crates/lazyrecall-tui
```

## Quick start

```bash
lazyrecall
```

For automatic session summaries (powered by Claude Haiku 4.5), set your Anthropic API key:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

…or write the key to `~/.lazyrecall/api-key` (one line). Without it, `lazyrecall` works fine — summaries just stay blank.

## Keys

| Key                        | Action                                |
| -------------------------- | ------------------------------------- |
| `j` / `↓`                  | Move down in focused pane             |
| `k` / `↑`                  | Move up in focused pane               |
| `Tab` / `l` / `→`          | Cycle focus forward                   |
| `Shift-Tab` / `h` / `←`    | Cycle focus back                      |
| `Enter` (Projects)         | Move focus to Sessions                |
| `Enter` (Sessions)         | Quit TUI and `claude --resume <id>`   |
| `q` / `Esc`                | Quit                                  |

## Features

- **Three-pane layout** — projects, sessions, preview. Bold border on focused pane.
- **Inline summaries** — a background worker calls Haiku on every session that doesn't have one yet and writes the result back to the index. Subsequent launches are instant.
- **Live FS watcher** — `~/.claude/projects/` is watched recursively (notify / FSEvents). New sessions appear in the status bar without restart.
- **Resume in the right cwd** — `claude --resume` is scoped to its cwd. lazyrecall captures each session's recorded `cwd` from the transcript and `chdir`s before exec.
- **Persistent SQLite index** at `~/.lazyrecall/index.db` with a versioned schema from day one.
- **Tail-truncated summaries** — sessions can be hundreds of MB. The worker keeps the last 30K chars (most representative of what was achieved) before sending to Haiku.

## Architecture

```
crates/
├── lazyrecall-core/   library: discovery, parser, index, summarizer, watcher
└── lazyrecall-tui/    binary `lazyrecall`: ratatui frontend
```

A library + binary split, so a future GUI (Tauri, web, anything) can reuse the core without a rewrite.

Three threads, three SQLite connections (rusqlite's `Connection` is `Send` but not `Sync`):

1. **Main / TUI thread** — ratatui event loop, owns the index for reads.
2. **Summarizer worker** — polls for sessions where `summary IS NULL`, calls Haiku concurrently, writes back.
3. **FS watcher** — debounced (200 ms) recursive watch on `~/.claude/projects/`.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full tour.

## Roadmap

- **V1** — TUI browser + auto-summarize + `--resume` + watcher ✅
- **V1.5** — cross-session ripgrep with summary-aware ranking; `/` to fuzzy-filter
- **V2** — tags, pins, "since-we-last-talked" context injector on resume
- **V3** — embeddings + topic clusters, branch-graph view of forked sessions
- **V4** — Tauri GUI ("claude.ai for local sessions")

## Stack

Rust 2021 · `ratatui` + `crossterm` (TUI) · `rusqlite` with bundled SQLite (zero system deps) · `notify` + `notify-debouncer-mini` (file watcher) · `reqwest` + rustls (no OpenSSL) · `tokio` (only the summarizer uses async).

## Data locations

| Path | Purpose |
| ---- | ------- |
| `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl` | Claude Code's source of truth. lazyrecall reads only — never writes. |
| `~/.lazyrecall/index.db` | SQLite cache: session metadata + Haiku summaries. Safe to delete; regenerates on next run. |
| `~/.lazyrecall/api-key` | Optional fallback API key file (one line). |
| `~/.lazyrecall/log` | Recent error log for debugging. |

## Contributing

PRs welcome. See [CONTRIBUTING.md](CONTRIBUTING.md). The architecture is small enough to read in a sitting — start with [ARCHITECTURE.md](ARCHITECTURE.md) and `crates/lazyrecall-core/src/lib.rs`.

## License

Dual-licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
