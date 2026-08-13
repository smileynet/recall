---
id: "014"
title: "Update crew-research: replace Python recall references with Rust binary"
status: done
blocked_by: ["013"]
estimate: 1h
---

# Update crew-research References

## What to update

### Priority 1: Skills that agents use every session
- `atomics/skills/recall/SKILL.md` — update install instructions (no uv)
- `atomics/skills/recall/references/cli-reference.md` — update to Rust binary, remove Python-specific
- `atomics/skills/cheatsheet/SKILL.md` — update install one-liner

### Priority 2: Steering
- `.kiro/steering/user-setup-guide.md` — replace `uv tool install` with binary install
- `AGENTS.md` — update install, testing, troubleshooting sections

### Priority 3: Scripts
- `tools/recall/Invoke-RecallIngestAll.ps1` — simplify to direct `recall.exe ingest`
- `tools/recall/profile-hook.ps1` — simplify (no venv activation needed)
- `tools/recall/ingest-all.sh` — update to call Rust binary
- `tools/recall/bashrc-hook.sh` — simplify

### Priority 4: Mise tasks
- `mise.toml` — update `recall:ingest`, `recall:status` tasks
- Remove or update `test:recall` (now `cargo test` in the recall repo)

### Skip for now
- Proofs/evals that import Python library — keep Python recall for these until rewritten
- ADRs/specs — these are historical records, don't modify

## Acceptance criteria

- [ ] No skill/steering file references Python-specific install (uv, pip, venv)
- [ ] Doctor.sh still works with Rust health --json output
- [ ] Profile hooks work with Rust binary (tested manually)
- [ ] `mise run recall:status` works
