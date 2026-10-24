# Security and fidelity boundary

This crate orchestrates trusted converters; it does not sandbox code or verify semantic conversion accuracy.

Implemented: private staged file copies, bounded source reads, source hashing, conservative routing diagnostics, explicit format/goal selection, ambiguity errors, external-execution opt-in, cooperative token/deadline checks, no failure fallback, checked declared output paths, symlink/traversal rejection, output count/total bytes checks and explicit artifact export.

Not implemented: hardlinks/filesystem-race protection from same-user malicious code, cgroups/containers, network denial, syscall/file-access isolation, live scratch-disk quota, in-process memory limits, arbitrary JSON output size caps, forced interruption of in-process parsers, tool/model installation, dependency advisory audit, corpus fidelity benchmarks or prompt-injection filtering.

Source symlinks resolving to regular files are allowed. The source is staged to a neutral basename, so external sidecars/resources are not copied. Staging hashes the copied bytes, but it does not lock the original file against concurrent writers. Original basenames, temporary paths, payloads, warnings and progress error strings may contain sensitive content. Staged output has a lifetime tied to FacadeBundle; temporary directory deletion is best effort.

Converter callbacks, document extractors, progress observers, route declarations and backend options are trusted deployment code. They can bypass cooperative checks. A converter can write undeclared files until scratch cleanup. Output validators run after converters finish and assume no concurrently malicious filesystem modification. A core namespace re-export is not a guarantee of planner/policy interoperability.

Facade manifests are immutable through the public result API, but files can still be modified through exposed paths. Export checks declared sizes; it does not hash artifacts or provide transactional/atomic directory writes. Source byte hashes are not complete cache identities; tool/model/configuration versions must be included by any downstream cache.

Pipeline intermediate identities are recorded explicitly. Rendering or re-extracting a converted Office file changes the source identity, and original regions may not map exactly. Never claim original PDF/Office page or table provenance by reinterpreting rendered Markdown. OCR scores and ASR text remain backend outputs, not medical/hazard-analysis correctness evidence.
