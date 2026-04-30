# recall

Lazygit-style TUI for browsing, searching, and resuming Claude Code sessions. The memory layer Claude Code is missing.

## Why

Every `claude` process writes a JSONL transcript at `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl`, and `claude --resume <id>` brings the entire conversation + agent state back. The data is there; the UX is hidden behind a flag and a UUID. recall is the missing front end: browse every session across every project, see auto-generated summaries inline, one-keystroke resume.

## Roadmap

- **V1** (week 1): TUI browser + auto-summarize on session inactivity + `claude --resume <id>` invocation
- **V1.5**: cross-session ripgrep with summary-aware ranking
- **V2**: tags, pins, "since-we-last-talked" context injector on resume
- **V3**: embeddings + topic clusters, branch-graph view of forked sessions
- **V4**: Tauri GUI — the "claude.ai for local sessions" framing
- **V5**: open-source release; optional cloud sync as the commercial layer

## Architecture

```
crates/
├── recall-core/   library: discovery, parser, index, summarizer
└── recall-tui/    binary (`recall`): ratatui frontend
```

Library + binary split so a future Tauri GUI reuses the core without a rewrite.

## Stack

Rust 2021. ratatui + crossterm (TUI). rusqlite (SQLite index at `~/.recall/index.db`). notify (JSONL watcher). reqwest + serde (Anthropic Messages API for Haiku 4.5 summaries).

## Run

```bash
cargo run -p recall-tui
```

Requires `ANTHROPIC_API_KEY` in env once the summarizer is wired into the watcher loop.

## Status

V1 in progress. Day 1 scaffolding committed 2026-04-30.
