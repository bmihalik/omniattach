# Security boundary

The CLI runs trusted configured converters through the real sibling facade. It is not a sandbox. Registering a route or passing an execution permission is not a restriction on arbitrary Rust code.

No shell commands, executable templates, URL downloads, automatic model installs, recursive directory traversal or implicit file overwrites are added. Conversion output requires a new directory with an existing parent. Export correctness, artifact containment and temporary-file ownership are delegated to the actual facade.

Inspection uses regular-file metadata checks and byte budgets. Filesystem races are not prevented by those checks; untrusted same-user writers can alter paths between validation and open. Source symlinks resolving to regular files are accepted. Named pipes and device inputs are not intentionally supported. No assumption of atomic source snapshots or output export is made.

XML inspection reads a bounded whole document into memory and maintains namespace-aware path counts. It rejects DTD/custom entities, but parser internals may allocate on namespace/start-tag processing before per-event checks execute. Byte/path/depth/attribute limits and cooperative deadlines are not hard memory/CPU quotas. Run hostile inputs in an OS-limited worker process.

Inspection does not return XML text/attribute values, but source paths, namespace URIs, attribute names, manifests and error diagnostics may still be sensitive. No prompt-injection, semantic fidelity, PHI redaction or hazardous-material/medical correctness guarantees are made.

Signal handling requests cancellation only. Arbitrary parser code, callbacks, blocking I/O and export may not stop promptly. The CLI does not kill arbitrary process groups itself. Use the real backend runner and an external supervisor where hard deadlines/termination are needed.

Default stdout is machine JSON and progress/errors use stderr. Human --help/--version and explicitly chosen --stdout text are exceptions. Error strings may expose paths/tool details. A stdout write failure or nonzero exit does not imply no exported files exist. Inspect the requested destination and error record.

The dependency set was compiled/tested, not formally audited for vulnerabilities or license compatibility of external tools/models. Their binaries and weights are not included.
