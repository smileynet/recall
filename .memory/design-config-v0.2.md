# Design: User-Tunable Configuration (v0.2)

Status: decided (ticket 37, 2026-10-21)
Consumers: #060 (setup), #080 (import-root identity), #087 (local-state ownership)

## Problem

recall configuration is ad hoc and partly broken:

- Three env vars (`RECALL_DB`, `RECALL_MODEL`, `FASTEMBED_CACHE_DIR`) resolved
  independently in `store`, `embed`, `ingest`.
- `~/.recall/config.toml` is read by exactly two modules (`telemetry`, `update`),
  each with its own hand-rolled line parser.
- **Live data-loss bug:** `TelemetryConfig::save()` rewrites the whole file with
  only `[telemetry]`, silently erasing `[update]` and any other section
  (confirmed in `src/telemetry.rs::save`, read against `src/update.rs` which
  separately reads `[update]` from the same file).
- Import-root discovery is copy-pasted in three sites (`cmd_import_all`,
  `cmd_sync` phase 2, `discover_project_coverage`) and hardcodes `~/code` +
  `D:/code`.

## Decisions

### D1 — Format & parsing: adopt `toml_edit`

Keep `~/.recall/config.toml` as the file, but parse/edit it with the `toml_edit`
crate instead of the two hand-rolled parsers.

Rationale:
- The hand-rolled parsers cannot do lossless round-trips. Preserving unknown
  keys on a read/modify/write requires a real TOML serializer; reimplementing
  one is strictly worse than taking the idiomatic crate (same pattern as
  `mise.toml`, `rustfmt.toml`; prior art in `godot_helper` ADR 0002
  "configuration split").
- `toml_edit` preserves key order, unknown sections, AND comments across a
  targeted section update — a strictly stronger "lossless" guarantee than the
  plain `toml` crate's `Table` (which reorders and drops comments). That matches
  what #087 needs from the config boundary.

Rejected: extend the hand-rolled parser. It is the direct cause of the live
data-loss bug and cannot preserve unknown keys without becoming a TOML library.

Rejected: plain `toml` crate. Lossy on comments/ordering; `toml_edit` is the
format-preserving variant maintained by the same project.

### D2 — CLI: `recall config` subcommands (deferred to a follow-up, not v0.2)

Target surface (documented now, implemented later):
```
recall config list              # print effective config (merged)
recall config get <key>         # e.g. recall config get import.roots
recall config set <key> <val>   # lossless targeted write
recall config path              # print config file location
```

v0.2 ships the **library primitive** (typed load + lossless edit) and the
`import.*` keys wired into discovery. The `config get/set/list` CLI is a thin
follow-up on top of the primitive — it is NOT required to unblock #060/#080/#087,
so it is deferred to keep this ticket scoped. Users edit `config.toml` directly
in v0.2 (documented).

### D3 — Precedence: env var > config.toml > built-in default

Highest wins. Env vars stay the escape hatch for one-off overrides and tests;
`config.toml` is the persistent user preference; defaults preserve today's
behavior. Where a key has an existing env var (`RECALL_DB`, `RECALL_MODEL`,
`FASTEMBED_CACHE_DIR`), the env var continues to win.

For `import.roots` specifically: if unset in config, the default is the current
behavior (`$HOME/code` + `D:/code` when they exist), so unconfigured installs
are byte-for-byte unchanged.

### D4 — Unknown keys: preserve, don't error

Unknown sections/keys are **preserved** on read/modify/write and **ignored** on
load (not an error, not a warning by default). Rationale: forward compatibility.
An older binary must not corrupt a newer binary's config, and a schema addition
must not break `recall telemetry disable` on an old install. A future
`recall config lint` can warn about unrecognized keys opt-in.

### D5 — Lossless update semantics (consumed by #087)

Config writes are **targeted section updates**, never full-file rewrites:

1. Read the existing file into a `toml_edit::DocumentMut` (empty doc if absent).
2. Replace/insert only the target section (e.g. `[telemetry]`).
3. Write the whole document back (comments, ordering, and unknown sections
   survive because `toml_edit` edits in place).

Invariant: updating section A preserves every other known section AND every
unknown section/key. This is proven by a round-trip test (telemetry edit
preserves `[update]` + an unknown `[future]` section).

### D6 — Ownership boundary (the one #087 adopts)

A single `config` module owns:
- Typed load of the whole document (`Config` struct) with unknown-key capture.
- The lossless targeted-edit primitive (D5).
- Project discovery driven by `import.roots` / `import.exclude`.

This is deliberately the ONE config owner. #087 ("Centralize recall local-state
paths and lossless configuration") extends THIS module with the remaining path
resolution (DB, logs, model cache, markers) rather than introducing a second
parser — satisfying #087 AC6 ("#37 uses this owner rather than introducing
another config parser or path resolver"). #087 remains responsible for moving
`recall_dir()` path policy out of `telemetry`/`update`/`store`/`embed`; this
ticket only centralizes the *config document* contract and import discovery.

## v0.2 keyset (what ships now)

| Key | Default | Precedence source | Tier |
|-----|---------|-------------------|------|
| `import.roots` | `[$HOME/code, D:/code]` (existing dirs) | config > default | **ship-now** |
| `import.exclude` | `[]` | config > default | **ship-now** |
| `telemetry.enabled` | `false` | config (env suppress) | existing, re-homed |
| `telemetry.crash_reporting` | `false` | config | existing, re-homed |
| `update.check` | `true` | config | existing, re-homed |
| `update.interval_hours` | `24` | config | existing, re-homed |

"Re-homed" = already exists, now read/written through the lossless `config`
module so edits stop clobbering each other.

## Priority ranking (deferred keys — NOT in v0.2)

Ranked by actual user need vs. implementation cost and cross-ticket collision:

1. `search.max_results` (default result count) — low cost, high visibility.
   Currently a CLI flag default of 5; config would set the default. **Next.**
2. `display.quiet` / `display.color` — low cost, affects scripted use. **Next.**
3. `sync.import_interval` (run import every Nth sync) — needed by #060 scheduling
   polish, modest cost.
4. `ingest.active_threshold_secs`, `ingest.chunk_size` — tuning knobs; chunk_size
   changes invalidate embeddings, so gate behind a re-embed warning. Lower need.
5. `search.rrf_k`, `search.min_score` — search-quality tuning; needs the golden-
   query harness to validate changes. Expert-only; **last**.
6. `ingest.session_dir` — overlaps the env var `RECALL_DB` story and #087 path
   ownership; defer until #087 lands to avoid a second path resolver.

Deferred because: each adds surface that #087 (ownership) and #080 (root
identity) are actively reshaping. Shipping `import.*` first delivers the
prerequisite #060/#080 need without pre-committing the rest.

## Non-goals (this ticket)

- No `recall config get/set/list` CLI (D2 — follow-up).
- No migration of `store`/`embed` path resolution (that's #087).
- No change to import-root *identity*/orphan-scoping (that's #080); this ticket
  only de-hardcodes and de-triplicates *discovery*.
