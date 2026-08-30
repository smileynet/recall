---
id: "065"
title: "deploy-local.ps1 fails under Windows PowerShell 5.1 (non-ASCII em-dash, no BOM)"
status: done
blocked_by: []
priority: high
validation_criteria:
  - "scripts/deploy-local.ps1 contains no bytes > 0x7F (pure ASCII)"
  - "powershell -NoProfile -File scripts/deploy-local.ps1 parses without ParserError"
---

# deploy-local.ps1 fails under Windows PowerShell 5.1 (non-ASCII em-dash, no BOM)

## Context

`scripts/deploy-local.ps1` was authored for PowerShell 7 (`#!/usr/bin/env pwsh`
shebang) and contains five U+2014 EM DASH characters (lines 2, 30, 46, 50, 67)
in comments and string literals. The file has **no UTF-8 BOM**.

Windows PowerShell 5.1 (`powershell.exe`), when reading a BOM-less file, decodes
it with the system ANSI codepage rather than UTF-8. Each em-dash (UTF-8 bytes
`E2 80 94`) is mis-decoded into `â€"`, and the stray double-quote/paren bytes
corrupt the token stream. Result: the script fails to parse before running.

Discovered 2026-08-30 during a deploy: `powershell -File deploy-local.ps1`
aborted with cascading `ParserError` / `UnexpectedToken` on line 30
(`throw "Tests failed — aborting deploy"`). Re-running under `pwsh` (7.6.5)
succeeded because PS7 defaults to UTF-8. AGENTS.md documents the deploy command
as `./scripts/deploy-local.ps1` without specifying the interpreter, so a user
on the default `powershell` shell hits this.

## What to build

- [ ] Replace all five U+2014 em-dashes in `scripts/deploy-local.ps1` with ASCII
      ` - ` so the file is pure ASCII and parses under both Windows PowerShell
      5.1 and PowerShell 7 without relying on a BOM.

## Acceptance criteria

- [x] `scripts/deploy-local.ps1` contains no bytes above 0x7F
- [x] Script parses cleanly under Windows PowerShell 5.1 (`powershell -NoProfile`)
- [x] Script still parses/runs under PowerShell 7 (`pwsh`)

## Validation criteria

- Byte scan: no byte > 0x7F in the file
- `powershell -NoProfile -Command "[ScriptBlock]::Create((Get-Content -Raw scripts/deploy-local.ps1))"` → no ParserError

## Resolution (2026-08-30)

Made deploy-local.ps1 pure ASCII by replacing em-dashes (lines 2,30,46,50,67) with hyphens. Windows PowerShell 5.1 decodes BOM-less files as ANSI and mangled the em-dashes; ASCII parses under both 5.1 and pwsh 7. Diff is only the 5 substitutions, LF preserved.

### Verification
1. ✓ scripts/deploy-local.ps1 contains no bytes > 0x7F (pure ASCII) — "Byte scan via pwsh ReadAllBytes: 'bytes>0x7F: 0' — file is pure ASCII after replacing 5 U+2014 with hyphens"
2. ✓ powershell -NoProfile -File scripts/deploy-local.ps1 parses without ParserError — "Parse check on both interpreters: 'PARSE OK on PS 5.1.26100.9278' and 'PARSE OK on PS 7.6.5' via [ScriptBlock]::Create(Get-Content -Raw); no ParserError"
