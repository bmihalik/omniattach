# LFE-STEP-5: first executable slice

This is an explicit-grid native-text PDF table converter, not automatic table recognition.
It does not alter default extraction, equations, provisioning or canonical Document.

```sh
# Coordinates are illustrative; replace with a real table's PDF user-space boundaries.
cargo run -p omniattach-documents --features pdf --example pdf-table -- input.pdf 1 50,150,250 100,130,160
cargo test -p omniattach-documents --features pdf --example pdf-table
cargo fmt --all
```

PAGE is one-based. X and Y boundaries must be finite and ascending, with gaps greater
than 0.5 PDF units. Output is top-to-bottom Markdown on stdout; source/page notes go
to stderr. First row is the header. Empty cells are preserved. Text crossing a cell
boundary is an error, including ambiguous estimated bounds. Text ordering uses top,
then left, then operation sequence; a fixed 1-unit line tolerance is intentionally
limited. Only ordinary axis-aligned rectangular cells with decoded text are supported.
The engine caps boundaries, cells and accepted text bytes. Existing inspection limits
still apply. PDF inspection geometry is not normalized for CropBox/Rotate/UserUnit.

Pending: preserve detected ruling coordinates, explicit candidate-ID selection,
public typed API/facade and main CLI integration, synthetic PDF end-to-end fixtures,
scoped diagnostics and source spans. No automatic grid capture is claimed here.
Multi-page continuation, borderless/image tables, merged cells and rotation remain
out of scope. This commit contains engine unit tests but has not been compiled or
executed in the preparation environment. Test locally before relying on the example.
