---
id: "39"
title: "Adopt versioning, changelog, and crates workflows from tkt (release pipeline already exists)"
status: open
blocked_by: ["37"]
priority: low
---

# Adopt versioning, changelog, and crates workflows from tkt

## Status note (2026-10-09 review — RE-SCOPED)

This ticket was partially overtaken by events. The **GitHub release pipeline it set out
to "adopt" already exists in-repo** via cargo-dist:

- `dist-workspace.toml` — cargo-dist 0.32.0, `ci = "github"`, 5 target triples
- `.github/workflows/release.yml` — full tag-triggered pipeline (plan -> build-local ->
  build-global -> host -> announce)

So "adopt a release workflow" is **done by other means**. The genuinely-missing pieces
are narrower (see below). Dropped `38`, `046`, `047` from `blocked_by` — they were not
real dependencies for the remaining changelog/versioning work. Kept `37` (config) because
version-source-of-truth decisions interact with the config module design.

## Remaining work (the actual gap)

Study `~/code/tkt` and adopt only what recall still lacks:

- **Version single-source-of-truth** — today the version is in `Cargo.toml`; the shipped
  binary reports 0.1.0 while ticket 38 drafts assume 0.2.0 (drift). Decide the canonical
  source and whether anything else must read from it.
- **Changelog discipline** — no `CHANGELOG.md` exists at repo root. Adopt a format
  (Keep a Changelog / conventional) and an update rule (per-PR or at release cut).
- **crates.io publishing** — go/no-go decision. recall is a local-first single binary with
  a relocated model cache; publishing to crates.io may not fit. Document the decision
  either way.
- **mise tasks** — no `mise.toml` at root. Optional: add release/changelog helper tasks if
  they reduce friction (not required if cargo-dist + a changelog rule suffice).

## Reference

- `~/code/tkt` — examine `mise.toml`, changelog, and any release helper scripts
- Existing in-repo: `dist-workspace.toml`, `.github/workflows/release.yml` (do NOT
  re-create these — they already work)

## Acceptance criteria

- [ ] Version single-source-of-truth decided and documented (reconcile 0.1.0 vs 0.2.0 drift)
- [ ] `CHANGELOG.md` created with a chosen format + an update-discipline note in AGENTS.md
- [ ] crates.io publish decision made and documented (adopt or record why skipped)
- [ ] (Optional) `mise.toml` release/changelog tasks added if they add value
- [ ] Explicitly confirm the existing cargo-dist release pipeline is NOT duplicated

## Out of scope

- Re-implementing the GitHub release pipeline (already present via cargo-dist)
