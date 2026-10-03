# recall — Project Learnings

What was **learned** building recall (cross-session semantic memory for AI assistants —
a single Rust binary doing hybrid BM25 + vector search over ingested sessions). This is
the portable distillation: lessons that transfer to other Rust CLIs, local-embedding
tools, and data-ingestion pipelines. It is **not** a description of what recall is — for
that, read the root `README.md` / `AGENTS.md`.

Context: Rust rebuild of a Python original; 221 commits; 57 tickets closed; 3 ADRs.

## Index

| Doc | What's in it |
|-----|--------------|
| [decisions.md](decisions.md) | The 6 architecture choices, why, alternatives, and which hurt |
| [patterns.md](patterns.md) | Reusable code patterns (copy-pasteable shapes) |
| [dead-ends.md](dead-ends.md) | What was tried and abandoned — read before re-exploring |
| [native-dependency-pinning.md](native-dependency-pinning.md) | The ort/ONNX saga — the deepest lesson of the project |
| [tools-and-setup.md](tools-and-setup.md) | Build profiles, deploy script, test tooling |
| [methodology.md](methodology.md) | How work was tracked, decisions captured, changes scoped |

## The three highest-value takeaways

1. **Test the path your users actually take.** A committed `Cargo.lock` kept
   `cargo build --release` green for months while `cargo install` (a fresh resolve) was
   broken — a transitive pre-release auto-upgraded across an ABI break. The daily dev
   loop hid the breakage. See [native-dependency-pinning.md](native-dependency-pinning.md).

2. **Design out "self-hiding gaps."** The worst failure class is one that reports success
   while doing nothing — an empty ingest that still writes the freshness marker, a
   strict `>` watermark that silently drops equal-timestamp rows, a 14KB stub DLL that
   passes a size check. recall names these as first-class risks and guards each. See
   [decisions.md](decisions.md) and [patterns.md](patterns.md).

3. **Don't trust a native dependency's error path — pre-validate the artifact.** `ort`
   panics internally on a bad ONNX RT dylib (uncatchable under `panic=abort`). recall
   dlopens and version-checks the lib itself before handing it over. See
   [patterns.md](patterns.md) #1.
