---
id: "021"
title: "Spike: verify Rust binary PATH installation on Windows"
status: done
priority: high
type: spike
blocked_by: []
estimate: 15min
---

# Spike: Binary Installation on PATH

## Question

What's the best way to make `recall.exe` available on PATH for:
1. Interactive shell use (`recall search "..."`)
2. Scheduled task execution
3. Profile hook invocation

## Options

A. **Copy to ~/.cargo/bin/** — already on PATH for Rust users
B. **Copy to ~/.local/bin/** — may need PATH addition
C. **cargo install** from source — builds fresh, puts in ~/.cargo/bin/
D. **Symlink** from build dir — fragile if you rebuild
E. **Leave in build dir, use full path** — works for task, ugly for interactive

## What to do

1. Check what's currently on PATH (`where recall`)
2. Check if `~/.cargo/bin/` is on PATH
3. Decide installation location
4. Test that scheduled task can find it

## Success criteria

- [x] Chosen location documented
- [x] Binary accessible from: PowerShell, cmd, Git Bash, Task Scheduler
