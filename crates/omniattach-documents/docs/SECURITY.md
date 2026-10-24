# Security boundary

This crate is a local synchronous library, not an OS sandbox.

Implemented boundaries:

- Bounded input read; no writes beside the source.
- ZIP declared expansion and entry-count checks, duplicate/path-traversal checks, no filesystem unpacking.
- XML DTD/entity expansion disabled; node count capped, no external resolver.
- Restricted namespace-aware XML selectors; no embedded XPath evaluator or query execution.
- ODS nonempty repetition expansion bounded before allocation; padding budgets checked.
- PDF object/xref and text stream decompression limits passed to lopdf.
- HTML scripts/styles omitted; no resource fetching or scripting engine.
- No macros, formula recalculation, external processes, cloud calls or model downloads.
- Optional CSV formula-prefix escaping for presentation in spreadsheet software.

Limits do not make the process memory-safe from exhaustion. HTML/Calamine/PDF parsing can allocate internally; declared archive sizes are not a verified cumulative decompression meter for every dependency; PDF stream bounds are per stream, not process totals. Output serialization creates additional allocations. There is no deadline, cancellation, global output-byte cap or process memory budget in this release. Configure an isolated worker, OS memory/CPU limits and application-level timeouts for untrusted uploads. Reassess dependency advisories before deployment; no vulnerability audit was performed.

Markdown escaping avoids directly emitting raw HTML from ordinary text, but it is not a universal sanitization guarantee for every consumer. Converted documents can still carry prompt-injection instructions. Treat their content as untrusted data, not application/system instructions. Do not interpret it as executable code.

Feature `office` parses macro-enabled workbooks as data but never executes macros. Rendered CSV can expose formulas as text from malicious input; enable presentation escaping when opening exported data in spreadsheet applications, and retain canonical values separately.
