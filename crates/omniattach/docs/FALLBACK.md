# Opt-in facade fallback

Ordinary plan/convert remain single-route and reject ambiguity. The explicit convert_with_fallback method tries registered route IDs in policy order; it does not install tools or infer an order from registration. This is facade-library support, not new stock CLI flags. The branch includes the layout integrations but does not merge either feature into main.

```rust,ignore
let mut policy = FallbackPolicy::new(["documents", "marker", "mineru"])
    .with_acceptance(FallbackAcceptance::NonEmptyText);
policy.candidates[0] = FallbackCandidate::new("documents")
    .with_options(serde_json::json!({"dehyphenate_max_tokens":6}));
policy.candidates[1] = FallbackCandidate::new("marker").with_options(serde_json::json!({}));
policy.candidates[2] = FallbackCandidate::new("mineru").with_options(serde_json::json!({}));
let bundle = engine.convert_with_fallback(&request, &context, &policy)?;
bundle.export("new-output")?;
```

Register all candidates first and enable external execution when the chain contains external routes. The full chain is preflighted before any converter: route existence, input format, goal and permission must match. One denied external candidate invalidates the entire policy, even if an earlier native candidate might succeed. request.route must be absent. IDs are unique/nonempty and the list has 1..=32 entries. A Poppler Extract-only route cannot be placed in a Markdown-goal chain without an explicit compatible adapter.

Candidate options replace, not merge, request options; omission inherits them. Use empty options for model adapters rather than accidentally forwarding native OCR/dehyphenation settings. Their tool/model configuration remains on the backend instance.

The wrapper hashes a bounded input snapshot, then uses independent convert workspaces/input copies per attempt. It explicitly closes rejected bundles; failed converter workspaces retain best-effort drop cleanup. Accepted artifact paths and bytes, including layout image relationships, are unchanged. Converters are trusted in-process code: this is not isolation against a converter deliberately modifying request.path, arbitrary files or other process memory. Use StagedInput.path for converter I/O.

Default retries cover backend Spawn with NotFound and ProcessFailed with an explicit exit code. Per-attempt/underlying backend timeouts retry only if AttemptTimeout is added to retry_on. Signal-only process failure, denied execution, invalid/unsafe output, cancellation, typed input/resource/configuration errors, nested fallback and opaque converter errors are terminal. No stderr/message guessing is used; untyped upstream exits/warnings can conceal causes that cannot be recovered here. Native document errors are preserved as Error::Document, including Parse; they are conservatively terminal. A successful but weak native result can instead trigger the configured acceptance rule.

AnySuccess (default) permits artifact-only output. NonEmptyText and MinTextChars count non-whitespace Unicode scalar characters, not tokens or proof of table/figure/equation completeness. Normal artifact validation runs before acceptance, so unsafe paths are never treated as benign quality rejection.

The context timeout covers snapshot, preflight and all attempts. Optional positive per_attempt_timeout_ms can shorten each attempt but never extends the overall budget. Overall deadline/cancellation take precedence. OCR receives the job cancellation token and remaining timeout; process limits remain per invocation and in-process work/observers remain cooperative, not forcibly interrupted. Resource limits bound each attempt and the selected export, not total live CPU/RAM/disk; aggregate work can reach the sum of attempt budgets.

Success includes omniattach/fallback/0.1 with attempts, outcomes, retry classes, selected route and elapsed time; the manifest route remains the actual winner. Top-level source identifies the original input, while nested payload source names may refer to snapshot copies, explicitly warned. Failure is Error::Fallback { report, source }, with a typed terminal source or FallbackExhausted. Finalization can still fail after a candidate passed acceptance; the report retains that candidate's accepted/selected record. Details are bounded to 1024 characters but may contain sensitive paths/stderr; options are not logged by the fallback wrapper.

Intermediate converter Completed/Failed progress events are suppressed; staging/converting events use chain-relative elapsed time and one final event is emitted. There is no separate fallback planner or per-attempt progress-stage enum.

Public API additions include FallbackPolicy/Candidate/Acceptance/Retry/Report, convert_with_fallback and new Error variants. Downstream exhaustive error matches may require updates. No package/schema version bump, merge or publication is included. Validation is not claimed: Rust/Cargo were unavailable in the preparation environment. Run cargo fmt --all; cargo test -p omniattach --no-default-features --test fallback; cargo test -p omniattach --all-features --test fallback; cargo test --workspace --all-features; cargo check --workspace --all-features --all-targets; cargo clippy --workspace --all-features --all-targets -- -D warnings. Tests use scripted converters/injected runners, not actual model inference.
