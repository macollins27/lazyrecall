# lazyrecall

Lazygit-style TUI for browsing, searching, and resuming Claude Code sessions. The memory layer Claude Code is missing.

## Why

Every `claude` process writes a JSONL transcript at `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl`, and `claude --resume <id>` brings the entire conversation + agent state back. The data is there; the UX is hidden behind a flag and a UUID. lazyrecall is the missing front end: browse every session across every project, see auto-generated summaries inline, one-keystroke resume.

## Status

V1 feature-complete. Compiles clean with zero warnings. **Not yet manually validated end-to-end** — the per-feature `cargo check` greens are necessary but not sufficient. First-run smoke testing pending.

## V1 features

- Three-pane TUI (`projects` / `sessions` / `preview`) with lazygit-style focus + bold border on active pane
- Tab cycles focus, `j/k` (or arrows) navigate, Enter on Sessions pane invokes `claude --resume <id>` in the host terminal via Unix `exec()`
- Persistent SQLite index at `~/.lazyrecall/index.db` with a versioned schema (`schema_version` table from day 1 so V2+ migrations don't paint us into a corner)
- Background Haiku 4.5 summarizer that runs when `ANTHROPIC_API_KEY` is set; processes sessions where `summary IS NULL`, writes one-line summaries back to the index. Uses tail-truncated transcripts (last 30K chars).
- Live FS watcher (notify / FSEvents) on `~/.claude/projects/` so new sessions appear in the status counts without restart
- Status bar: `N sessions · M summarized · K pending · summarizer working|idle|no ANTHROPIC_API_KEY`
- Preview pane renders the last 6 events with role-coded labels: `[user]`, `[claude]`, `[tool]`, `[result]`, `[system]` — sidechain (subagent) and meta events filtered out

## Roadmap

- **V1** — TUI browser + auto-summarize + `--resume` + watcher [DONE]
- **V1.5** — cross-session ripgrep with summary-aware ranking; `/` to fuzzy-filter
- **V2** — tags, pins, "since-we-last-talked" context injector on resume
- **V3** — embeddings + topic clusters, branch-graph view of forked sessions
- **V4** — Tauri GUI ("claude.ai for local sessions")

## Architecture

```
crates/
├── lazyrecall-core/   library: discovery, parser, index, summarizer, watcher
└── lazyrecall-tui/    binary `lazyrecall`: ratatui frontend
```

Library + binary split so a future Tauri GUI reuses the core without a rewrite. The summarizer worker and FS watcher each run on their own `std::thread` with their own `Index` (rusqlite Connection) connection — `Connection` is `Send` not `Sync`, but multiple connections on the same DB file are fine.

## Stack

Rust 2021. `ratatui` + `crossterm` (TUI). `rusqlite` with bundled SQLite (index). `notify` (file watcher). `reqwest` + `serde` (Anthropic Messages API for Haiku 4.5).

## Install

From source:

```bash
git clone https://github.com/macollins27/lazyrecall.git
cd lazyrecall
cargo install --path crates/lazyrecall-tui
```

This installs the `lazyrecall` binary into `~/.cargo/bin/` (make sure that's on your `PATH`).

For summaries, export your Anthropic API key:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

Or write it (one line) to `~/.lazyrecall/api-key`. Without the key, lazyrecall still works — summaries just stay blank.

## Run

```bash
lazyrecall
```

## Keys

| Key | Action |
|-----|--------|
| `j` / down | Move down in focused pane |
| `k` / up | Move up in focused pane |
| `Tab` / `l` / right | Cycle focus forward |
| `Shift-Tab` / `h` / left | Cycle focus back |
| `Enter` (on Projects) | Move focus to Sessions |
| `Enter` (on Sessions) | Quit TUI and `claude --resume <id>` |
| `q` / `Esc` | Quit |

## Data

- `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl` — Claude Code's source of truth; lazyrecall reads only.
- `~/.lazyrecall/index.db` — lazyrecall's SQLite cache (session metadata + Haiku-generated summaries). Safe to delete; regenerates on next run.
- `~/.lazyrecall/api-key` — optional fallback API key file (one line).

## License

MIT OR Apache-2.0
