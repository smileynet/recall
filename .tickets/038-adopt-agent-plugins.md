---
id: "038"
title: "Adopt Agent Plugins standard — own and deploy recall skill from this repo"
status: open
blocked_by: []
---

# Adopt Agent Plugins Standard

## Context

crew-research is formalizing a skill import protocol (crew-research ticket 98) where tool
repos own and deploy their own skills instead of crew-research maintaining them. The Agent
Plugins 1.0 standard (Aug 2026, backed by Amazon/Microsoft/OpenAI/Vercel/Cursor) defines
the portable package shape; crew-research adds a lifecycle manifest on top for version
checking, auto-deploy, and staleness detection.

Currently the `recall` skill lives in crew-research (`atomics/skills/recall/SKILL.md`).
This ticket moves it here — recall owns its own skill, deploys it via symlinks, and
crew-research's copy becomes a fallback.

## References to clone and review

```bash
# Agent Plugins spec (the standard)
gh repo clone agentplugins/agent-plugins-spec ~/code/refs/agent-plugins-spec

# Agent Plugins example (reference implementation)
gh repo clone agentplugins/agent-plugins-example ~/code/refs/agent-plugins-example

# crew-research (the skill import protocol and tooling)
# Already at ~/code/crew-research — see:
#   .tickets/98-skill-import-protocol.md        — the protocol design
#   .scratch/research/agent-plugins-spec.md     — spec analysis
#   .scratch/research/agent-plugins-example.md  — example analysis
#   .references/agent-plugins-spec/             — cloned spec repo
#   .references/agent-plugins-example/          — cloned example repo

# archwright (existing pattern to follow for deploy-skills.sh)
# Already at ~/code/archwright — see:
#   tools/deploy-skills.sh                     — the deploy script to replicate
```

**Key docs:**
- Agent Plugins spec: https://agent-plugins.org/
- Agent Skills format: https://agentskills.io/specification
- Agent Plugins GitHub: https://github.com/agentplugins/agent-plugins-spec

## What to build

### 1. Create `skills/recall/` in this repo

Copy from crew-research:
```
skills/
  recall/
    SKILL.md              # from atomics/skills/recall/SKILL.md
    references/           # from atomics/skills/recall/references/
```

### 2. Add `plugin.json` (Agent Plugins 1.0 compliance)

```json
{
  "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
  "name": "recall",
  "version": "0.2.0",
  "description": "Cross-session semantic memory for AI coding assistants",
  "repository": "https://github.com/smileynet/recall",
  "license": "MIT",
  "keywords": ["memory", "recall", "semantic-search", "sessions"]
}
```

### 3. Add `SKILL_MANIFEST.yaml` (crew-research lifecycle contract)

```yaml
name: recall
version: "0.2.0"
compatibility:
  crew_research: "~> 0.9"
binary:
  name: recall
  version_cmd: "recall --version"
  min_version: "0.2.0"
skills:
  - name: recall
    path: skills/recall
    replaces: "atomics/skills/recall"
deploy:
  method: symlink
  auto: true
  script: "tools/deploy-skills.sh"
```

### 4. Add `tools/deploy-skills.sh`

Follow archwright's pattern (~/code/archwright/tools/deploy-skills.sh):
- Symlink `skills/recall/` into `~/.kiro/skills/recall` (global kiro)
- Support `--tool` flag for claude/codex/agy
- Support `--project` for project-level deploy
- Ownership manifest to avoid overwriting foreign files

### 5. Update recall's AGENTS.md

Document the new skills/ directory and deploy command.

## Acceptance criteria

- [ ] `skills/recall/SKILL.md` exists with correct frontmatter
- [ ] `plugin.json` passes Agent Plugins JSON Schema validation
- [ ] `SKILL_MANIFEST.yaml` present with version matching Cargo.toml
- [ ] `tools/deploy-skills.sh` deploys to kiro/claude/codex paths
- [ ] Deployed skill via symlink takes priority over crew-research fallback
- [ ] `recall --version` output matches manifest `min_version`

## Out of scope

- Updating crew-research's doctor.sh/init.sh (that's crew-research ticket 98)
- recall ticket 36 (learnable preferences) — independent feature work
