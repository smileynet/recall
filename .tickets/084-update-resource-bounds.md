---
id: "084"
title: "Bound self-update download and archive resource use"
status: open
blocked_by: []
env: either
priority: medium
---

# Bound self-update download and archive resource use

## Context

Ticket #051 added request timeouts and digest verification, and #063 bounds
decompressed tar reads. The review found remaining allocation exposure:
`download_asset` uses unbounded `read_to_end` before digest verification, and
the zip helper reads a matching entry without a size cap. A malformed or
unexpectedly huge response can therefore exhaust memory before checksum failure
or archive validation runs.

This is defense in depth for an explicitly requested self-update, not a claim
that the GitHub release source is untrusted.

## What to build

1. Define documented, conservative compressed-download and extracted-binary
   limits suitable for the release artifacts.
2. Stream/count update downloads and fail before allocating beyond the limit;
   use `Content-Length` only as an early rejection, not as the sole guard.
3. Apply equivalent per-entry and total limits to zip extraction, matching the
   tar/tar.xz safety posture. Keep regular-file and path handling strict.
4. Ensure failures leave the running binary untouched and retain the existing
   checksum-before-replacement guarantee.

## Acceptance criteria

- [ ] Oversized or streaming-without-content-length downloads fail within the
      configured bound without unbounded allocation.
- [ ] Oversized zip entries and excessive decompression fail before extraction.
- [ ] Normal current release archives still update successfully.
- [ ] A rejected update never renames, overwrites, or removes the current binary.

## Validation criteria

- Unit tests use bounded readers and in-memory archives to prove each limit.
- Existing digest, tar safety, and replacement rollback tests continue to pass.
