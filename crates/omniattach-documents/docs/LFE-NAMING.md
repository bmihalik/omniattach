# LFE-STEP-2: deterministic element names

With feature pdf, deunicode transliterates Unicode basenames to ASCII. Both slash conventions are accepted; the final extension is removed. At most 256 input stem characters are transliterated, and the slug is lowercased, restricted to ASCII alphanumerics/hyphens, collapsed/trimmed and bounded to 64 bytes. Unmapped characters become separators. Empty slugs use document; reserved Windows device basenames get document-. This is approximate transliteration, not translation or language/context-sensitive pronunciation. Original document text and Source metadata remain Unicode and unchanged.

Naming format: <slug>-<source-hash-prefix>-fe-<index padded to at least six digits>. The default source hash prefix has eight lowercase hex digits; the full 64-digit SHA-256 is retained in the plan. The helper validates hash syntax, not the original bytes; the inspection API computes the actual source hash. Call inspect_pdf_bytes with the original logical document name, not a temporary input.pdf name. A naming helper cannot recover an original filename that its caller discarded.

```rust,ignore
let report = inspect_pdf("Műszaki.pdf", &Options::default())?;
let plan = report.naming_plan()?;
for element in plan.elements() {
    println!("{} -> {}", element.element_sequence(), element.image_path("png")?);
}
```

One counter covers Image, EquationCandidate and TableCandidate records in inspection order. Text runs do not consume indices. Entries retain the original inspection sequence and candidate kind. Overlapping equation/tag/grid hints are not guessed to be the same physical element: names are provisional candidate names until later recognition resolves structure. Repeated image occurrences get separate names; byte deduplication is not implemented here. Derivatives of one entry share its stem, e.g. image_path("png") and file_name("tex").

image_path returns a relative images/ path for supported raster extensions. file_name accepts a short ASCII alphanumeric extension without a leading dot. These helpers do not validate codec bytes, render images, recognize formulas, write files or insert Markdown links. STEP-3 must choose extensions matching actual output and use create-new/no-overwrite export behavior. No extra equation/table sidecars are mandatory in this lightweight step.

The output-only LfeNamingPlan serializes source identity, generator version, inspection schema, prefix policy and mappings. Use check_collisions before combining plans in one export namespace: identical plans are reusable, but a conflicting nonempty plan with the same prefix is rejected. from_inspection_with_hash_len supports 8..=64 hash digits for explicit collision handling. Separate namespaces may still be needed when distinct logical names collapse and source bytes are identical. Short hashes and transliteration are not mathematical uniqueness guarantees. No existing files are scanned by this pure naming API.

Names are deterministic for a fixed report, dependency/version and prefix policy, not guaranteed unchanged across parser, recognition, transliteration-table or configuration changes. The ordinary inspection JSON and default conversion output are unchanged; request naming separately or use:

```sh
cargo run -p omniattach-documents --features pdf --example pdf-inspect -- --names book.pdf
```

Validation: cargo fmt --all; cargo test -p omniattach-documents --features pdf --test lfe_names; cargo test -p omniattach-documents --features pdf --test pdf_inspection; cargo test --workspace --all-features; cargo check --workspace --all-features --all-targets; cargo clippy --workspace --all-features --all-targets -- -D warnings. Cargo must update Cargo.lock for deunicode and the updated feature graph; it is not hand-edited. Tests are prepared, not executed in the assistant environment. This commit changes no versions, merges nothing and publishes nothing.
