---
id: "080"
title: "Make imports safe across roots and subdirectories"
status: open
blocked_by: []
env: either
priority: high
---

# Make imports safe across roots and subdirectories

## Context

Import source keys and manifest paths are relative only to the directory passed
to `recall import`. Orphan cleanup then compares that relative set with every
manifest entry in the wing. Importing a subdirectory, or two separate roots,
into one wing can therefore delete chunks from the other root; matching relative
paths can also collide. The `--force` warning in ticket 072 does not protect the
ordinary non-force path.

Until this is fixed, the safe operational rule is one canonical `.memory` root
per wing and no subdirectory imports into an already populated wing.

## What to build

1. Model a canonical import-root identity separately from the wing and relative
   path. Use it in both `import_sources` identity and chunk source/provenance.
2. Scope orphan detection and `--force` deletion to the selected root unless a
   deliberately explicit whole-wing operation is requested.
3. Add a backward-compatible migration for existing manifests/chunk source
   keys. Define a safe ambiguity policy rather than silently deleting data.
4. Coordinate root discovery/configuration with #37, but do not make this data
   integrity fix depend on the broader configuration design.

## Acceptance criteria

- [ ] Two roots imported into one wing coexist, including identical relative
      filenames, without source-key collisions.
- [ ] Re-importing a subdirectory cannot delete chunks from its sibling/root.
- [ ] Removing a file deletes only the matching root's chunks.
- [ ] `--force` reports and obeys its precise root/wing blast radius.
- [ ] Existing one-root imports migrate without silent loss.

## Validation criteria

- Integration tests cover two roots, nested imports, duplicate relative paths,
  root-local orphan removal, and force behavior.
- `recall health`/status counts remain consistent after migration.
