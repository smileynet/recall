# Methodology

How work was tracked, decisions captured, and changes scoped. Portable process lessons
with evidence `(ticket / SHA)`.

## Work tracking — the `tkt` frontier model

**Dependencies are data, so the ready-set is mechanical.** Each ticket carries
`blocked_by: [ids]`; `tkt ready` computes the frontier (lowest-number-first, priority
jumps). You never hand-maintain "what's next." *(ticket 12 `blocked_by:["11","17","22"]`;
068 `blocked_by:["064"]`.)*

**Claim → work → close as three separate pushed commits.** `chore(tickets): claim NNN` /
`feat(scope): … (ticket NNN)` / `chore(tickets): close NNN`. The claim is a visible WIP
marker; the close is **gated** — config requires a `## Resolution` + per-criterion
evidence, so you can't close without citing how each AC was verified. *(SHAs
8d1f9f1/b5d5d0f/625098f for 075.)*

**Fold same-surface duplicates; split different-surface scope out.** 049 folded into 068
(same ort/ABI surface — folding avoided re-deciding the version twice); 068 spun 075 out
as a new ticket (the larger uplift is a different surface). The fold happened *after*
research proved 049's prescribed fix impossible. *(068 title "folds 049"; 075 blocked_by
068.)*

## Decision capture — three channels, non-overlapping

| Channel | Owns | Test |
|---------|------|------|
| **ADR** (`.memory/adr/`) | Hard-to-reverse decisions + why + alternatives | "Would a future session re-propose the rejected option?" |
| **CONTEXT.md glossary** | Which-*meaning* of an ambiguous term only | "Could two people mean different things by this word?" |
| **AGENTS.md Constraints** | Project-wide operational traps | "Would any workflow hit this?" |
| **recall write-back** | Durable decisions, searchable across sessions | hard-to-reverse OR matters beyond next session |
| **`.scratch/`** | Ephemeral working notes | "Will this matter next week?" |

Example of the discipline: the "three meanings of v3" and "two different SQLite databases"
confusions went to CONTEXT.md (disambiguation); the ort coupling went to an ADR
(tradeoff); the DLL-sidecar extraction trap went to AGENTS.md Constraints (operational
trap); raw research stayed in `.scratch/`.

## Change scoping

**One logical change per commit; never sweep in pre-existing dirty files.** When unrelated
`cargo fmt` reformats sat in the working tree, they were left uncommitted (and flagged in
the handoff as a standalone `style:` chore) rather than bundled into feature commits.
Conventional-commit prefixes throughout (`feat`/`fix`/`docs`/`chore`/`style`).

## The research loop — dispatch → synthesize → verify → then build

Before any big or risky change: **fan out parallel research/review subagents** (web +
codebase), each writing raw findings to `.scratch/{research,subagent-raw}/`; synthesize
into the *ticket* (not the code); and **verify premises against primary source before
writing any code.** This repeatedly overturned stated plans:
- 049: "rc.9→rc.13" — impossible; fastembed pins ort. *(read fastembed's Cargo.toml)*
- 074: "rusqlite is a new dependency" — it wasn't. *(read Cargo.toml)*
- 068: the "init returns Err" premise — wrong; ort panics. *(read ort source)*
- 075: "ONNX RT 1.24" — never released. *(404 probe)*

**Lesson: a confidently-worded ticket is a hypothesis, not a spec. Verify the load-bearing
facts from L1 sources (crate metadata, library source, a real HTTP probe) before building
on them.**

## Spikes as first-class tickets

Pure decision/benchmark work gets its own numbered ticket with no code deliverable
(ticket 01 = "compare embedding models → keep bge-base"). Keeps the decision traceable
and the frontier honest.
