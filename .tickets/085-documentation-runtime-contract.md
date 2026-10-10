---
id: "085"
title: "Reconcile documentation with runtime and deployment contracts"
status: open
blocked_by: []
env: either
priority: medium
---

# Reconcile documentation with runtime and deployment contracts

## Context

The review found drift across README, AGENTS.md, ticket history, and runtime
behavior. Examples: README says no network at runtime even though first model
use downloads artifacts and interactive runs can check GitHub for updates;
README/AGENTS claim 81 tests while the library has 116; and scheduler cadence /
timeout descriptions differ across handoff material. The review environment did
not contain the documented `RecallIngest` task, so local deployment claims must
be distinguishable from portable product instructions.

#060 owns the setup feature and #039 owns release/version discipline; this ticket
owns accurate cross-document statements once those decisions are known.

## What to build

1. Define the local-first/network contract precisely: no server/API keys for
   memory operations; initial model/runtime provisioning and optional/automatic
   update checks have bounded network behavior and controls.
2. Reconcile README, AGENTS.md, CLI help/comments, deploy scripts, and relevant
   ADRs with actual commands, supported platforms, MSRV, test invocation, and
   scheduler semantics.
3. Replace volatile local corpus/task claims with reproducible verification
   commands where practical; label deployment-specific examples as such.
4. Add a lightweight documentation verification checklist to release/deploy
   work so command and behavior drift is caught before the next release.

## Acceptance criteria

- [ ] No user-facing document makes a claim contradicted by current CLI/runtime
      behavior for networking, installation, scheduling, model cache, or tests.
- [ ] Product defaults and environment/config opt-outs for update checks are
      discoverable from the documentation.
- [ ] Deployment instructions distinguish Windows Task Scheduler, systemd,
      launchd, and uninstalled/no-scheduler states accurately.
- [ ] Version/MSRV/test-count claims are either current or replaced by commands
      that derive the truth.

## Validation criteria

- A clean-machine documentation walk-through reaches `recall health` without
  undocumented prerequisites.
- Review README/AGENTS against `recall --help`, Cargo.toml, deploy scripts, and
  the installed scheduler configuration before closing.
