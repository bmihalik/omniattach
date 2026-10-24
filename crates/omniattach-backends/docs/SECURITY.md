# Security boundary

This is an execution library, **not an OS sandbox**.

Implemented boundaries:

- Local regular-file staging with bounded reads, SHA-256 identity, neutral basenames and private temporary job directories.
- No shell evaluation or string command concatenation; literal arguments only.
- Environment allowlist with opt-in overrides; stdin closed.
- Unix child process group; concurrent nonblocking pipe reads; independent stdout/stderr limits; timeout/cancellation; process group cleanup and direct-child reaping.
- Traversal/absolute-path/symlink rejection for declared output files; actual regular-file and length checks; post-process artifact count/total limits.
- A fresh LibreOffice profile per call; expected output existence/signature validation even after a zero exit status.
- FFmpeg/FFprobe input protocol allowlist (`file,pipe`); explicit first-stream selection.
- Explicit backend selection and error propagation, no silent model/service/backend fallback.

Not implemented:

- No seccomp, namespaces, chroot, container, OS job objects, cgroup CPU/RAM/disk quotas, network denial or file-access isolation.
- No protection against trusted programs that deliberately escape their group (setsid/setpgid), spawn unrelated services, alter other user files or access network resources. Process groups are lifecycle control, not hostile-code isolation.
- No live scratch-disk meter; converters may write oversized files before post-process rejection, including undeclared scratch files. Scratch removal failures are best-effort.
- No hard whole-job deadline; input staging, parsing/export, spawn and cleanup are outside the invocation poll deadline. OS spawn/reaping can block in kernel-level edge cases.
- No secure race-free filesystem API against a concurrently malicious same-user tool. Symlink/path checks assume the process has completed and the workspace is not concurrently modified. Hardlinks and platform-specific mount semantics are not neutralized.
- No universal format validator, antivirus, prompt-injection detector, OCR accuracy guarantee or model hallucination detection.
- No dependency advisory audit, fuzzing campaign, production corpus validation or crash-recovery scavenger.

HOME and model locations are accessible. A fresh LibreOffice profile does not prove macro/link/network isolation for all document formats; use a container with an appropriately configured Office security policy. Install the required document filters, not merely the LibreOffice launcher.

Custom commands and Tool environment overrides are trusted deployment configuration, not parameters to accept from untrusted users. Paths/diagnostic stdout/stderr/model paths may expose sensitive content; avoid indiscriminate logging. JSON manifests contain original source names and staged ffprobe metadata may contain temporary paths.

For hazardous/medical material workflows, preserve original bytes and provenance independently and verify conversions before relying on extracted results. Numeric/unit/OCR/transcription correctness is not established by a successful process exit.
