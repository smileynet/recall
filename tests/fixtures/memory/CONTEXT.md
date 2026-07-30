---
type: glossary
title: "Test Context"
---

# Context

## Architecture

The system uses a layered architecture with SQLite for storage and fastembed for local embeddings.

## Decisions

We chose Rust for performance and single-binary distribution. The embedding model is BGE-base-en-v1.5 at 768 dimensions.
