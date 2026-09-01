---
id: "073"
title: "embed: extract macOS infix-versioned ONNX RT dylib from tgz"
status: open
blocked_by: []
priority: high
validation_criteria:
  - "ort_lib_entry_matches accepts libonnxruntime.<ver>.dylib and rejects the zero-size symlink and .dSYM debug copy"
  - "recall ingest initializes ONNX RT on Darwin arm64 (no 'Could not find libonnxruntime.dylib' error)"
---

# embed: extract macOS infix-versioned ONNX RT dylib from tgz

## What to build

TBD

## Acceptance criteria

- [ ] TBD
