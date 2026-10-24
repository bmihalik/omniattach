# Changelog

## 0.1.0

- Implemented portable facade: explicit routes, format/goal planning, builder, runtime context and owned result envelope.
- Added conservative detection, strict text encodings, Markdown preservation, source hashes and validated artifact export.
- Added signature-neutral DocumentAdapter and explicit two-step ArtifactPipeline.
- Added optional sibling namespace re-exports and optional backend 0.1 integration source/tests.
- Added portable verification harness, 39 integration tests, examples and Mermaid architecture.

Known integration gap: actual sibling crate source APIs were unavailable; core consolidation/document default registration and optional backend compilation require the user's real workspace. No full-workspace build claim is made.
