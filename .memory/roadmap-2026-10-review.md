# Recall remediation roadmap — 2026-10 review

## Purpose

This is the implementation order derived from the 2026-10-09 review of all
tickets, commits, architecture, and tests. It supplements rather than replaces
the ticket tracker: a ticket is the source of acceptance criteria and status.

Recall's core product is complete: local SQLite storage, hybrid retrieval,
session and markdown ingestion, migration, scheduled `sync`, deployment, and
runtime hardening. The next plan protects data correctness and restores a
reliable engineering baseline before adding richer memory features.

## Phase 0 — make the baseline trustworthy

1. **#076 — update archive suffix mapping.** Fix the Windows `.tar.xz` unit
   failure by deriving the binary name from archive format, then add the
   asset-format regression matrix.
2. **#082 — decide the health JSON nullability contract.** Choose `null`, `0`,
   or omission deliberately; do not accept a snapshot merely to turn CI green.
3. **#081 — hermetic CLI tests.** Remove home-directory, network, model-cache,
   and DLL-install dependencies from the normal test tier. It consumes #082's
   chosen JSON contract.

Exit gate: `cargo test`, `cargo clippy --all-targets -- -D warnings`, and
`cargo fmt --check` pass on a machine with an empty user runtime cache and no
network access.

## Phase 1 — protect corpus integrity

1. **#077 — migration force semantics.** Do not use `recall migrate --force`
   against a populated destination until it deletes exactly its own provenance.
2. **#078 — model compatibility barrier.** Reject model-mismatched writes and
   unequal-length vector comparisons before they poison a corpus.
3. **#079 — scan cache correctness.** Implement the advertised stat-then-hash
   gate with precise metadata and no missed same-size content revisions.
4. **#080 — import-root identity.** Scope source keys, orphan cleanup, and
   force behavior to an import root, not only a wing. Until then, use one
   canonical `.memory` root per wing and avoid subdirectory imports.
5. **#055 — lock `forget`.** Make every corpus-mutating command participate in
   the same process-lock protocol and expose lock state through health.

Exit gate: new regression/integration tests prove no duplication, mixed-model
vectors, false scan skips, or cross-root import deletions.

## Phase 2 — make operation portable and observable

1. **#037 — unified configuration and project discovery.** Deliver
   `import.roots`/`import.exclude` first and replace the three hard-coded root
   scans with one shared implementation.
2. **#060 — setup.** Build an idempotent, inspectable scheduler installer around
   `recall sync`, using Phase 2 configuration and reporting coverage without
   failing a valid empty installation.
3. **#070 / #071.** Replace the unmaintained lock dependency and make lock-skip
   telemetry distinguishable from successful work.
4. **#039 / #038.** Establish version/changelog discipline and package the
   assistant skill only after the operational contract is stable.

## Phase 3 — measured improvements, not speculative complexity

- **#057:** expand legacy JSONL traversal only after confirming a nested Kiro
  layout that the SQLite source does not already cover.
- **#058:** benchmark at the current corpus, then activate vector-index work at
  100K chunks or >2 s warm search.
- **#040–047:** assess summaries, structured state, temporal decay, and learned
  preferences only after the correctness and setup phases are complete. These
  are product experiments, not prerequisites for Recall's semantic-memory MVP.

## Current validation facts

- `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` passed
  during the review.
- The library suite had one known failure: #076.
- Golden queries (10), CLI error tests (17), and wing-migration tests (6)
  passed.
- Health snapshot and prime contract testing exposed #082 and #081,
  respectively; no snapshot change was accepted.
