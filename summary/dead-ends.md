# Dead Ends

Approaches tried or seriously considered and abandoned — read before re-exploring. Each:
what, why it failed, what to do instead.

## Dependency / build

- **Upgrade `ort` by itself.** Impossible — `fastembed` exact-pins `ort`/`ort-sys`. You
  must bump `fastembed`; `ort`'s version is a function of it. *(ticket 049 premise, proven
  wrong by reading `fastembed`'s `Cargo.toml`.)*
- **Pin ONNX Runtime to 1.24.** Dead 404 — Microsoft never released 1.24 (series skips
  1.23→1.25). The `api-24` feature is a floor, not a shipped version. Target 1.28. *(075)*
- **Use `[patch]` to control a transitive version.** Wrong tool — `[patch]` replaces a
  crate's *source*, not its version-within-a-range; the patched version must still satisfy
  the range. Use a direct `=` dependency. *(068)*
- **Rely on a committed `Cargo.lock` to pin a transitive dep.** Lockfile-only — does not
  survive a fresh resolve (`cargo install`, lockfile-free CI, library consumers). Pin in
  the manifest. *(068)*
- **`cargo update --precise` as the fix.** Same flaw — writes the lockfile only. *(068)*

## Native library handling

- **`immutable=1` to read a live SQLite DB read-only.** Promises the file won't change →
  stale/corrupt snapshots when it does. Use `SQLITE_OPEN_READ_ONLY` + `busy_timeout`
  instead. *(ADR 0001)*
- **Catch `ort`'s bad-DLL failure caller-side with `?`.** `ort` panics internally (lazy,
  at `.commit()`), not returns `Err` — unhandleable that way, and uncatchable at all under
  `panic=abort`. Pre-validate with `libloading` first. *(068)*
- **`catch_unwind` as the primary graceful-init mechanism.** Only works on unwind builds,
  the panic hook still prints to stderr first, and it's fragile. Keep it at most as a
  backstop; the pre-flight is primary. *(068 research)*
- **A `>1MB` size check as DLL-integrity guard.** A large wrong-version or wrong-file
  (sidecar) passes it; it also masked a real extraction bug. Use a pinned SHA-256 verified
  before extraction. *(064)*

## Data model

- **Strict `>` watermark for incremental ingest.** Silently drops rows that share the max
  millisecond — a permanent, self-hiding gap. Use `>=` + an idempotent sink. *(ADR 0002)*
- **Dedup across sources by surrogate/auto IDs.** Unreliable — IDs get reissued on
  delete-recreate and collide across separate namespaces. Use a stable natural key or a
  normalized content fingerprint (no timestamps/mutable fields). *(074 research)*
- **Assume JSONL filename UUID == SQLite `conversation_id` UUID.** Not a guaranteed shared
  id space. Namespace sources distinctly and prefer-one-when-present. *(074)*

## Partially open (needs a build/measurement, not abandoned)
- Whether an ONNX RT bump ever drags in a re-exported `model.onnx` or a changed default
  graph-opt level (would escalate parity risk to near-model-change). Verified *not* the
  case for 1.20→1.28; re-check on future bumps with the parity harness.
