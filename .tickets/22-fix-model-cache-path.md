---
id: "22"
title: "Fix model cache path — use ~/.recall/models/ instead of CWD-relative"
status: done
priority: high
blocked_by: []
estimate: 15min
---

# Fix Model Cache Path

## Problem

fastembed-rs defaults to `.fastembed_cache` **relative to CWD**. This means:
- Running `recall search` from different directories downloads the model multiple times
- Scheduled task CWD is unpredictable — may download to system32 or user home
- 83MB model duplicated per directory is wasteful

## Fix

Set explicit cache directory in `Embedder::with_model()`:

```rust
let cache_dir = home_dir().join(".recall").join("models");
let model = TextEmbedding::try_new(
    InitOptions::new(which.fastembed_model())
        .with_cache_dir(cache_dir)
        .with_show_download_progress(true)
)?;
```

## Acceptance criteria

- [x] Model always downloads to `~/.recall/models/`
- [x] Works regardless of CWD
- [x] First run downloads once, subsequent runs use cache
- [x] FASTEMBED_CACHE_DIR env var still works as override (if user wants custom)
