# Architecture

A short tour of the codebase for human contributors. The whole thing fits in your head — read this and you'll know where to start.

## The shape

```
crates/
├── lazyrecall-core/           library (everything except the UI)
│   └── src/
│       ├── lib.rs              public re-exports
│       ├── error.rs            typed error enum (thiserror)
│       ├── log.rs              append-only error log at ~/.lazyrecall/log
│       ├── discovery.rs        list projects + sessions on disk
│       ├── parser.rs           JSONL → SessionMetadata + Event stream
│       ├── index.rs            SQLite cache (versioned schema)
│       ├── summarizer.rs       Anthropic Messages API client (Haiku 4.5)
│       ├── summarizer_worker.rs   background loop, bounded-concurrent
│       └── watcher.rs          debounced FS watch on ~/.claude/projects/
└── lazyrecall-tui/            binary `lazyrecall` (TUI front-end)
    └── src/
        ├── main.rs             entry point + thread spawning
        ├── app.rs              App state, focus + pane management
        ├── ui.rs               drawing (ratatui Frame → screen)
        ├── input.rs            event loop + key handling
        ├── format.rs           pure formatting helpers
        └── theme.rs            color palette + reusable styles
```

The split exists so a future GUI (Tauri, web, anything) can reuse `lazyrecall-core` without a rewrite.

## How a frame is produced

1. `main()` opens the index, lists projects, spawns the watcher + summarizer threads, builds an `App`, and enters `input::run_loop`.
2. Each tick (200 ms): `ui::draw` is called with `&mut App`. It reads from `App.metadata_cache`, `App.recent_cache`, `App.summary_cache` — all populated lazily on first access.
3. Every ~5 s the loop calls `App::refresh_index_state` to pick up new summaries the worker has written.
4. On Enter in the Sessions pane, `App::request_resume` captures the session id + recorded cwd; the run loop returns; we tear down the terminal; then `Command::new("claude").arg("--resume").arg(id).current_dir(cwd).exec()` _replaces_ this process. (Unix `exec`, no child.)

## Threading

Three threads, three SQLite connections to the same `~/.lazyrecall/index.db`. The reason each thread opens its own `Index`: `rusqlite::Connection` is `Send` but not `Sync`, so it can't be shared. Multiple connections to the same DB file are fine.

```
            ┌─────────────┐
            │ index.db    │
            └──────┬──────┘
                   │ (3 separate Connections)
   ┌───────────────┼───────────────┐
   │               │               │
┌──▼──┐       ┌────▼────┐       ┌──▼──┐
│ TUI │       │ Worker  │       │Watch│
└─────┘       └─────────┘       └─────┘
   reads      reads + writes    writes only
   index      summaries         (touch_session)
```

There is no cross-thread channel. Workers write to the index; the TUI re-reads on its periodic tick.

## Schema migrations

`index.rs` has a `schema_version` table and a `SCHEMA_VERSION` const. V1 (idempotent CREATE TABLE IF NOT EXISTS) always runs on open; V2+ migrations (ALTER TABLE) run only if the recorded version is below them. Bump `SCHEMA_VERSION` and add a `SCHEMA_VN` block in `migrate()` for each new schema change. Don't edit a previous version's block.

## The two filter rules

Claude Code's JSONL has many event types. lazyrecall only cares about `user`, `assistant`, and `system`. Two filters apply everywhere events are surfaced:

- `isSidechain: true` events (subagent traffic) are skipped.
- `isMeta: true` events (meta noise) are skipped.

Three tests in `crates/lazyrecall-core/src/parser.rs` lock this in. If you change the parser, update those tests in the same change — they are the V1 contract.

## The `encoded-cwd` lossiness

`~/.claude/projects/{encoded-cwd}/` is the original cwd with `/` replaced by `-`. This is **not reversible** (a real path can contain `-`). Always recover the real cwd by reading the `cwd` field from the JSONL events themselves. Never try to "decode" the directory name.

`discovery::inspect_newest_session` does this: it picks the newest session, peeks ~10 lines, and pulls out the `cwd` field.

## Summarizer specifics

- **Tail-truncation**: sessions can be hundreds of MB. We keep the last 30K chars before sending to Haiku, on the theory that the tail represents what was achieved. Don't switch to head-truncation without thinking about this.
- **Concurrency**: 6 in-flight Haiku calls at once (`buffer_unordered`). The bottleneck is end-to-end latency, not rate-limiting.
- **Failure handling**: each session has a `summary_attempts` counter; after `MAX_SUMMARY_ATTEMPTS` failures it stops being retried so a malformed session can't wedge the worker.

## Watcher specifics

- **Debouncing**: claude writes JSONL line-by-line during streaming responses. Without debouncing, a 200-message session would generate 200 watch events. We use `notify-debouncer-mini` at 200 ms to coalesce.
- **Filter**: only `.jsonl` files are touched.

## Where things live on disk

| Path | Purpose | Writer |
| ---- | ------- | ------ |
| `~/.claude/projects/{encoded-cwd}/{session-uuid}.jsonl` | Source of truth | `claude` (we read only) |
| `~/.lazyrecall/index.db` | SQLite cache | All three threads |
| `~/.lazyrecall/api-key` | Optional API key fallback | User |
| `~/.lazyrecall/log` | Error log (debugging) | `lazyrecall_core::log::error` |

## What changes day-to-day

- New schema column → bump `SCHEMA_VERSION`, add `SCHEMA_VN` block.
- New JSONL event type → add a variant to `EventKind` and handle it in `extract_*` in `parser.rs`. Update parser tests.
- New keybinding → `input.rs`.
- New visual element → `ui.rs` + `theme.rs` if it needs color.
- New error path → variant in `error.rs`, then propagate.
