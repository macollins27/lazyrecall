# recall

Lazygit-style TUI for browsing, searching, and resuming Claude Code sessions. The memory layer Claude Code is missing.

## Why

Every `claude` process writes a JSONL transcript at `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl`, and `claude --resume <id>` brings the entire conversation + agent state back. The data is there; the UX is hidden behind a flag and a UUID. recall is the missing front end: browse every session across every project, see auto-generated summaries inline, one-keystroke resume.

## Status

V1 feature-complete (2026-04-30). Compiles clean with zero warnings. **Not yet manually validated end-to-end** — the per-feature `cargo check` greens are necessary but not sufficient. First-run smoke testing pending.

## V1 features

- Three-pane TUI (`projects` / `sessions` / `preview`) with lazygit-style focus + bold border on active pane
- Tab cycles focus, `j/k` (or arrows) navigate, Enter on Sessions pane invokes `claude --resume <id>` in the host terminal via Unix `exec()`
- Persistent SQLite index at `~/.recall/index.db` with a versioned schema (`schema_version` table from day 1 so V2+ migrations don't paint us into a corner)
- Background Haiku 4.5 summarizer that runs when `ANTHROPIC_API_KEY` is set; processes sessions where `summary IS NULL`, writes one-line summaries back to the index. Uses tail-truncated transcripts (last 30K chars).
- Live FS watcher (notify / FSEvents) on `~/.claude/projects/` so new sessions appear in the status counts without restart
- Status bar: `N sessions · M summarized · K pending · summarizer working|idle|no ANTHROPIC_API_KEY`
- Preview pane renders the last 6 events with role-coded labels: `[user]`, `[claude]`, `[tool]`, `[result]`, `[system]` — sidechain (subagent) and meta events filtered out

## Roadmap

- **V1** — TUI browser + auto-summarize + `--resume` + watcher [DONE 2026-04-30]
- **V1.5** — cross-session ripgrep with summary-aware ranking; `/` to fuzzy-filter
- **V2** — tags, pins, "since-we-last-talked" context injector on resume
- **V3** — embeddings + topic clusters, branch-graph view of forked sessions
- **V4** — Tauri GUI ("claude.ai for local sessions")
- **V5** — open-source release; optional cloud sync as the commercial layer

## Architecture

```
crates/
├── recall-core/   library: discovery, parser, index, summarizer, watcher
└── recall-tui/    binary `recall`: ratatui frontend
```

Library + binary split so a future Tauri GUI reuses the core without a rewrite. The summarizer worker and FS watcher each run on their own `std::thread` with their own `Index` (rusqlite Connection) connection — `Connection` is `Send` not `Sync`, but multiple connections on the same DB file are fine.

## Stack

Rust 2021. `ratatui` + `crossterm` (TUI). `rusqlite` with bundled SQLite (index). `notify` (file watcher). `reqwest` + `serde` (Anthropic Messages API for Haiku 4.5).

## Run

First time:

```bash
cd ~/Developer/recall
cargo run --release -p recall-tui
```

The release build takes ~1-2 minutes the first time (deps compile). Subsequent runs are instant. Add `~/Developer/recall/target/release/recall` to your `PATH` or set up an alias so `recall` works from anywhere.

For summaries, export your Anthropic API key:

```bash
export ANTHROPIC_API_KEY=sk-ant-...
```

Without the key, recall still works — it just falls back to relative timestamps in the sessions list.

## Keys

| Key | Action |
|-----|--------|
| `j` / down | Move down in focused pane |
| `k` / up | Move up in focused pane |
| `Tab` | Cycle focus: projects → sessions → preview |
| `Enter` (on Projects) | Move focus to Sessions |
| `Enter` (on Sessions) | Quit TUI and `claude --resume <id>` |
| `q` / `Esc` | Quit |

## Data

- `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl` — Claude Code's source of truth; recall reads only.
- `~/.recall/index.db` — recall's SQLite cache (session metadata + Haiku-generated summaries). Safe to delete; regenerates on next run.

## License

MIT OR Apache-2.0
