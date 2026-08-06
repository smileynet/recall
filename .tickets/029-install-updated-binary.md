---
id: 29
title: "Install updated binary locally (load-dynamic + all fixes)"
status: open
priority: high
blocked_by: []
estimate: 5min
---

# Install Updated Binary

## What to do

The deployed `recall.exe` at `~/.cargo/bin/` is stale — it predates load-dynamic,
Codex bug fixes (F1-F4), telemetry, logging, and sync.

Run:
```
cargo install --path . --force
```

Verify with `recall --version` and `recall search "test" --results 1`.

## Acceptance criteria

- [ ] `cargo install --path . --force` succeeds
- [ ] `recall search` works (uses cached ~/.recall/lib/onnxruntime.dll)
- [ ] `recall telemetry status` works
- [ ] `recall sync --skip-import` works
