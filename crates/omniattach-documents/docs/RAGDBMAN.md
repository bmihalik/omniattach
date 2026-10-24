# RagDBMan integration

Reference boundaries inspected in the project: `src/ragdbman/models.py`, `extract/text.py`, `extract/office.py` and `extract/__init__.py`.

This crate is independent. There is no Python extension or RagDBMan patch in the archive.

| Rust record | Suggested RagDBMan projection |
|---|---|
| Heading block | `Block(text, is_heading=True, heading=..., section_path=...)` |
| Paragraph/code/list block | `Block(text, heading=..., section_path=...)` |
| SourceSpan page/slide/sheet | Existing `page`, `slide`, `sheet` fields |
| SourceSpan line positions | Existing `line_start`, `line_end` |
| Workbook table row | Tab-joined cell display values, `sheet` and real cell row |
| `keep_together` | `atomic` |
| Diagnostics | Document warnings, while retaining structured JSON separately |

Prefer a subprocess wrapper around the example or a later thin binding during evaluation. Pass canonical records directly to a projection layer and render sidecars independently; do not replace precise source metadata with locations reconstructed from Markdown.

Rust `parent_id`/table/cell structures are richer than existing Python `Block`; retain canonical JSON if the projection cannot carry them. Keep current chunking, embedding, source graphs, collections and indexing downstream. Source-code conversion is a whole-file code block, **not** parity with RagDBMan symbol-aware source extraction.

Before switching a format, run both extractors on a representative real corpus and compare semantic content, source locations and warnings. Do not hide failures through automatic Python fallback during acceptance. Keep existing high-quality PDF/OCR/media routes where this crate intentionally provides no equivalent.
