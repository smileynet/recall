---
id: "087"
title: "Centralize recall local-state paths and lossless configuration"
status: open
blocked_by: ["37"]
env: either
priority: high
---

# Centralize recall local-state paths and lossless configuration

## Context

Recall has no single owner for its local application state. `store`, `embed`,
`ingest`, `logging`, `telemetry`, and `update` independently resolve `HOME` or
`USERPROFILE` and construct parts of `~/.recall`. This spreads path policy and
configuration parsing across unrelated modules.

There is also a current data-loss defect: `TelemetryConfig::save()` rewrites
`config.toml` with only `[telemetry]`, while `UpdateConfig` separately reads
`[update]` from the same file. Running `recall telemetry enable` or `disable`
can silently erase update settings and any future configuration sections.

This follows #37's decisions about format and precedence. It is an
implementation boundary, not a generic abstraction: Recall has one local-state
backend, so a concrete value/module is preferable to an unneeded trait.

## What to build

1. Add one concrete local-state owner that resolves Recall's DB-adjacent paths:
   database, config, logs, telemetry/crash data, update marker, model cache,
   and runtime library location.
2. Move full-config read/modify/write into that owner. Updating one section
   must preserve other known sections and unknown future keys.
3. Resolve environment-derived paths at command composition, then pass the
   resolved local-state value to consumers. Do not let individual subsystems
   repeatedly inspect process-global environment variables.
4. Give tests an explicit temporary local-state root. Model-cache policy must
   not leak through process-global `HF_HOME` or similar variables.

## Acceptance criteria

- [ ] One implementation owns all Recall local-state path resolution.
- [ ] Telemetry changes preserve `[update]`, import settings, and unknown
      configuration sections/keys.
- [ ] Production environment lookup occurs at the command boundary; unit tests
      can use a supplied temporary root without changing global environment.
- [ ] Model-cache/runtime configuration does not leak into unrelated in-process
      tests.
- [ ] Tests prove telemetry updates preserve unrelated configuration and no
      writes occur outside the supplied temporary root.
- [ ] #37 uses this owner rather than introducing another config parser or path
      resolver.

## Validation criteria

- Unit tests round-trip a config containing telemetry, update, import, and an
  unknown future section through each supported targeted update.
- `cargo test` passes with a temporary local-state root and without changes to
  the developer's home directory.
