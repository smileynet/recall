---
id: "015"
title: "Spike: verify doctor.sh compatibility with Rust health --json"
status: done
priority: high
type: spike
blocked_by: []
estimate: 30min
---

# Spike: doctor.sh Compatibility

## Question

Does the Rust `recall health --json` output satisfy `tools/generator/doctor.sh` parsing (lines 342-434)?

## What to do

1. Read `crew-research/tools/generator/doctor.sh` recall health check section
2. Compare JSON field names, types, and structure against Rust output
3. Run doctor.sh against the Rust binary's output (pipe test)
4. Identify any missing fields or type mismatches

## Key fields doctor.sh checks (from research):
- `total_chunks` (number)
- `wing_count` (number)
- `covered_projects` (number)
- `discoverable_projects` (number)
- `last_ingest_ts` (number or null)
- `duplicates` (array)
- `missing_projects` (array)
- `wings` (object)

## Success criteria

- [x] Document any fields Rust is missing
- [x] Document any type mismatches
- [x] Confirm doctor.sh passes or list exact fixes needed
