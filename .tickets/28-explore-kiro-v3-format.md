---
id: "28"
title: "Explore kiro-cli v3 session format and identify breaking changes"
status: done
type: spike
blocked_by: []
estimate: 2h
---

# Explore kiro-cli v3 Session Format

## Question

kiro-cli v3 introduces a new session file format. What changed, and what does recall
need to adapt to ingest v3 sessions correctly?

## What to do

1. **Identify the v3 session format**
   - Compare v2 vs v3 session JSONL structure
   - Document field changes, new fields, removed fields
   - Check if file naming or directory layout changed

2. **Identify breaking changes for recall**
   - Does the ingest parser (`parse_session_file`) handle v3?
   - Are there new message types or roles that need chunking rules?
   - Did the session directory path change (`~/.kiro/sessions/cli`)?
   - Are there new metadata fields useful for wing derivation or room classification?

3. **Check hook/integration changes**
   - v3 has standalone hook files in `~/.kiro/hooks/` (per steering docs)
   - Does this affect how recall could be triggered?
   - Any new session lifecycle events recall should react to?

4. **Document adaptation plan**
   - List code changes needed in `src/ingest.rs`
   - Estimate effort
   - Decide: support both v2+v3, or v3-only?

## Success criteria

- [x] v3 format documented (diff from v2)
- [x] Breaking changes identified with affected code paths
- [x] Adaptation plan with effort estimate
- [x] Decision: backward compatibility approach

## Findings (2026-08-06)

### Current state

- **kiro-cli version:** 2.16.1 (v2, not v3)
- **Session wire format:** `"version":"v1"` — this is what all sessions use
- **No v3 format exists on disk yet** — kiro-cli v3 hasn't shipped to this machine

### What recall already handles

The parser (`parse_session_file`) tries three formats in order:
1. `parse_kiro_v3` — expects `payload.type` structure (an older format variant)
2. `parse_kiro_v2` — expects `version:"v1"` + `kind` field (CURRENT format)
3. `parse_codex` — expects codex session metadata

The naming is confusing: recall's "v3 parser" parses an OLDER format, and
recall's "v2 parser" parses the CURRENT `version:v1` format that kiro-cli 2.x produces.

### Decision: NO ACTION NEEDED

- kiro-cli v3 hasn't shipped yet
- Current sessions are ingested correctly by the existing parser
- When kiro-cli v3 ships: re-evaluate by sampling new session files
- If the wire format changes, add a new parser variant (same pattern as existing)

### Reopening criteria

Reopen this ticket when:
- `kiro-cli --version` reports 3.x
- OR session files appear with a different structure than `version:"v1"` + `kind`

## Context

recall currently parses v2 and v3 session formats (see `parse_session_file` in ingest.rs
which handles both). This ticket is about verifying that the *new* v3 format from
kiro-cli's v3 release is still compatible, or if the existing v3 parser needs updates.

Also check if codex format parsing is affected (recall supports codex sessions too).
