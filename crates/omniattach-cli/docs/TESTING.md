# Verification

Environment: Linux x86_64; Rust/Cargo 1.99.1.

| Check | Executed result |
|---|---|
| Independent CLI/inspection harness integration tests | 50 passed |
| Documentation example | One passed |
| Formatting | Passed |
| Warning-denying Clippy, all isolated targets | Passed |
| Warning-denying rustdoc | Passed |
| Extracted archive isolated tests | Passed |
| Production facade-enabled main manifest | Not run; actual sibling source unavailable |
| Actual facade integration tests | Four supplied, not run |
| Backend-permission integration test | Supplied, not run |
| External tools, ASR, OCR, media conversion | Not run through this CLI |

The isolated manifest compiles the same src files with the facade feature disabled. It has real crates.io dependencies and a lockfile; it does not provide a fake omniattach crate. Engine test doubles exercise CLI forwarding/export-response behavior only.

Test coverage includes commands/help/version, compact/pretty JSON, stderr errors, output modes, options limits/forwarding, destination refusal, path characters, route configuration validation, inspection size caps, XML namespaces/aliases/rebinding, repeated-per-parent counts, entity/DTD rejection, malformed input, illegal characters, budgets, deadline, cancellation and error-only output.

Run the actual workspace build and tests before relying on integrated conversion. The production adapter references the exact facade API shown in the earlier source delivery, but that source archive was not available for integration compilation during this turn.
