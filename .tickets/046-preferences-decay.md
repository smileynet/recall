---
id: "046"
title: "Preference decay and health reporting"
status: open
blocked_by: ["045"]
priority: medium
---

# Preference decay and health reporting

## What to build

Preferences unused for >90 days are "stale":

- Stale preferences get reduced boost (e.g. 1.0× — no boost, same as facts)
- `recall health` reports: "N preferences, M stale (unused >90d)"
- No auto-deletion — demotion only, user can `recall forget` if wanted

## Acceptance criteria

- [ ] Stale preferences (last_used_at > 90 days ago) receive no boost in search
- [ ] `recall health` output includes preference/stale counts
- [ ] `recall health --json` includes `stale_preferences` field
- [ ] No data deletion — only ranking demotion
