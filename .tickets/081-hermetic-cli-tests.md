---
id: "081"
title: "Make CLI contract tests hermetic"
status: open
blocked_by: []
env: either
priority: high
---

# Make CLI contract tests hermetic

## Context

The 2026-10-09 review ran `cli_contract::prime_exit_code_zero` in a fresh
environment. The test attempted a real 75 MB ONNX Runtime download and then
failed to persist the DLL with `Access is denied`. The contract suite therefore
depends on the user profile, network, and mutable global runtime cache rather
than only its temporary test database.

The focused failure is environment-specific, but the test design is not:
contract/snapshot tests must not require model downloads or mutate `~/.recall`.
Coordinate the expected health JSON representation with #082.

## What to build

1. Identify every test path that can initialize FastEmbed/ORT or read/write a
   user-global cache.
2. Refactor command seams so empty-corpus and output-contract paths can run
   without embedding, and provide a controlled fake/preseeded dependency for
   tests that require retrieval behavior.
3. Isolate all test environment variables and runtime/cache paths in temporary
   directories; ensure parallel tests cannot race on a shared DLL/model file.
4. Keep a small, explicit integration tier for real-model verification rather
   than making normal `cargo test` depend on network/runtime installation.

## Acceptance criteria

- [ ] `cargo test` performs no network request and does not write under the
      developer's home directory.
- [ ] CLI contract and snapshot tests pass with no cached model or ORT runtime.
- [ ] Tests needing real embeddings use an explicit isolated fixture/tier.
- [ ] Parallel test runs cannot contend for a runtime DLL.
- [ ] #082's selected JSON contract is covered by a stable snapshot.

## Validation criteria

- Run the test suite with empty temporary HOME/USERPROFILE and
  FASTEMBED_CACHE_DIR; it passes without network access.
- Verify the user profile's `.recall` directory remains unchanged.
