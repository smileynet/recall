---
id: "39"
title: "Adopt versioning, changelog, release, and crates workflows from tkt"
status: open
blocked_by: ["19", "34", "35", "36", "37", "38"]
priority: low
---

# Adopt versioning, changelog, release, and crates workflows from tkt

## What to build

Adapt the versioning, changelog, release, and crates publishing workflows from `~/code/tkt` to this project. Study tkt's approach to:

- Version management (how versions are bumped, where the source of truth lives)
- Changelog generation/maintenance (format, automation, discipline)
- Release workflow (tagging, building, publishing steps)
- Crates.io publishing (if applicable — recall already uses cargo-dist for GitHub releases)

Adopt what fits recall's single-binary model. Adapt or skip what doesn't apply.

## Reference

- Source: `~/code/tkt` — examine their `mise.toml`, release scripts, changelog, and CI workflows

## Acceptance criteria

- [ ] Studied tkt's release infrastructure and documented what applies
- [ ] Versioning approach adopted (single source of truth for version)
- [ ] Changelog workflow adopted (format + update discipline)
- [ ] Release workflow adapted (tagging + publish steps)
- [ ] Crates.io publishing evaluated (adopt or document why skipped)
- [ ] `mise.toml` updated with any new release tasks
