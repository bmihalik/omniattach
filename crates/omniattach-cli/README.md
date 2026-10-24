# omniattach-cli

Command-line crate for `crates/omniattach-cli/` in the omniattach workspace. The binary is named **omniattach**. This archive contains only this crate; it does not replace your facade, document/backend/core crates or root workspace manifest.

## Compatibility and verification

The production adapter in `src/facade.rs` targets the public facade 0.1 API delivered in the preceding conversation: `OmniAttach::builder`, `ConversionRequest`, `FacadeContext`, owned bundle `export`, route capabilities and optional external defaults. The actual preceding source archive was not present in this turn's filesystem, and the public repository could not be retrieved. Therefore **the actual full workspace/default facade-enabled build has NOT been compiled here**. No fake facade was created to claim integration success.

The CLI inspection/transport implementation is compiled and tested with an isolated no-facade manifest using the same source files. It has **50 passing integration tests and one passing rustdoc test**, formatting, warning-denying Clippy and rustdoc checks. Injected test engines verify argument forwarding/export response handling, not real conversion correctness. Separate actual-facade integration tests are supplied for your real workspace.

## Install

Extract from your repository root. The archive paths start with `crates/omniattach-cli/`:

```sh
tar -xzf /path/to/downloaded-archive.tar.gz
```

Ensure `crates/omniattach-cli` is a member of the existing root workspace (or covered by `crates/*`). Do not replace the root Cargo.toml. The CLI's dependency uses the real sibling `../omniattach` directly.

```sh
# Real workspace, facade support on by default.
cargo test -p omniattach-cli
cargo run -p omniattach-cli -- capabilities
cargo build -p omniattach-cli --release

# Optional subprocess backend routes, through the real facade.
cargo test -p omniattach-cli --features backends
cargo run -p omniattach-cli --features backends -- --external-backends capabilities

# Optional core/documents namespace features on the real facade.
cargo test -p omniattach-cli --features workspace
```

The workspace root owns Cargo.lock. The supplied independent verification lockfile does not replace it. Cargo resolves path dependency manifests even when optional; use the verification manifest when actual sibling manifests are unavailable:

```sh
cd crates/omniattach-cli
./verification/check.sh
cargo run --manifest-path verification/Cargo.toml --locked -- capabilities
```

Do not enable `facade`, `backends`, `workspace` or `--all-features` on the isolated harness: it deliberately has no stand-in sibling dependency. `--features workspace` is for your actual workspace manifest only.

## Commands

| Command | Behavior | Requires facade? |
|---|---|---|
| `capabilities` | Compiled support and registered route contracts; no tool/model health probing | No; absent facade means no conversion routes |
| `probe INPUT` | Conservative prefix/extension detection and file size | No |
| `xml-probe INPUT` | Namespace-aware paths/counts, repeated children per parent, attribute names/counts | No |
| `plan INPUT` | Resolve requested format/goal/route without conversion | Yes |
| `convert INPUT --output NEW_DIR` | Convert, durably export, then print result | Yes |

`probe` uses a small local signature/extension router aligned to facade 0.1; it is not an Office package validator or an installed-converter check. ZIP document families are explicitly extension-hinted. The facade still redetects staged bytes at conversion time. No input URLs, implicit stdin, directory walking or hidden network fetches are accepted.

## Usage

```sh
omniattach capabilities --pretty
omniattach probe report.pdf
omniattach xml-probe measurements.xml --pretty
omniattach plan notes.txt --goal markdown
omniattach convert notes.txt --goal markdown --output notes-export --progress ndjson

# Existing Markdown is preserved by the facade's portable route.
omniattach convert notes.md --goal markdown --output markdown-export

# Original text on stdout, while still exporting a durable bundle.
omniattach convert notes.txt --output text-export --stdout text

# Explicit external registration AND execution permission.
omniattach --external-backends plan report.pdf --route poppler --allow-external
omniattach --external-backends convert report.pdf --route poppler --allow-external --output pdf-export
omniattach --external-backends convert clip.mp4 --goal video-frames --route ffmpeg-frames --allow-external --output frames-export

# Model registration is explicit; no automatic downloads.
omniattach --external-backends --whisper-model /models/ggml-base.bin --language hu \
  convert recording.wav --goal transcribe --route whisper-cpp --allow-external --output transcript-export
```

External commands require the `backends` Cargo feature and installed tools/models supported by the real sibling backend crate. Registering routes (`--external-backends`) and permitting execution (`--allow-external`) are separate actions. This CLI does not independently invoke a shell or bypass the facade's converter registry.

`OmniAttach` facade 0.1 defaults contain only its portable text route. Enabling `workspace` exposes the real sibling namespaces, but does **not** automatically register unknown document extraction APIs. The CLI deliberately does not promise DOCX/XML-to-CSV/PDF layout conversion unless those routes are actually registered. Embed `FacadeEngine::from_engine` with your configured real facade to add document adapters; no function signatures are guessed from module filenames.

## Request options

| Option | Meaning |
|---|---|
| `--goal extract` | extract, markdown, tables, pdf-layout, pdf-scientific, ocr, transcribe, video-frames, media-probe, normalize, custom:NAME |
| `--format FORMAT` | Explicit format override; never inferred from the requested goal |
| `--route ID` | Explicit route; no fallback after failure |
| `--options-json '{}'` | Converter-defined JSON object, forwarded unchanged |
| `--options-file FILE` | Same JSON object from a file; maximum 1 MiB; mutually exclusive with inline JSON |
| `--allow-external` | Explicit facade policy permission; does not register routes |
| `--max-input-bytes` | Facade source byte budget, default 512 MiB |
| `--max-export-bytes` | Facade declared artifact byte budget, default 512 MiB |
| `--max-export-files` | Declared artifact count budget, default 10,000 |
| `--timeout-ms N` | Cooperative facade deadline; omitted means no facade deadline, zero means immediate deadline |
| `--scratch-parent DIR` | Existing parent for facade scratch directories |
| `--output DIR` | Required NEW destination for convert; parent must exist |
| `--stdout manifest` | JSON result envelope, exact text, or no stdout (`manifest`, `text`, `none`) |
| `--progress ndjson` | Facade progress on stderr, never mixed into stdout |
| `--pretty` | Pretty JSON; default is compact one-record JSON |

Only options accepted by the selected real converter are supported. The earlier facade's portable route rejects nonempty options; its external adapter requires per-backend configuration rather than silently accepting arbitrary request options. The CLI does not silently drop options to make a conversion succeed.

Output paths can contain spaces and Unicode when passed as normal argv values. Use `--` before positional values that begin with a dash. No path expansion, environment interpolation, command templates, executable-path selection, overwrite or implicit output-parent creation is implemented.

## JSON subprocess contract

Default successful stdout is exactly one compact JSON value followed by a newline. `--pretty` intentionally changes it to multi-line JSON. `--stdout text` is raw exact facade text, not JSON; `--stdout none` emits no success output. Human help/version output is the normal exception.

A conversion success has this envelope (field names are contractual; this is not an observed conversion fixture):

```text
schema:        "omniattach-cli/result/0.1"
output_dir:    absolute durable export directory
manifest_file: absolute path to output_dir/manifest.json
manifest:      the actual unchanged facade manifest object
```

Artifacts listed by the facade manifest are relative to that export directory. The facade's private temporary bundle can drop only after export. Source identity fields may contain source/staging paths as metadata; they are not promised durable asset paths. Nested backend payload paths keep the facade's original remapping warning—consume top-level artifacts for files.

The CLI exports to a new directory before printing success. It does not add another manifest into that directory or rewrite original provenance. The manifest file remains the actual facade manifest; the CLI response envelope is only on stdout.

Errors have one compact stderr JSON record:

```text
schema:    "omniattach-cli/error/0.1"
error:     {code: stable symbolic code, message: diagnostic string}
exit_code: process exit status
```

With `--progress ndjson`, stderr is a sequence of facade progress JSON events and, on failure, a final CLI error record. Consumers must parse each record; the final error has the error schema. No result is printed for conversion failure. A failing stdout pipe or export can occur after side effects; a nonzero exit is not proof that no output directory exists.

## Exit codes

| Status | Meaning |
|---|---|
| 0 | Success, help or version |
| 1 | Conversion/output failure or signal-handler setup failure |
| 2 | Invalid arguments/configuration/options |
| 3 | File I/O or invalid inspection input |
| 4 | Unsupported build, no route, route mismatch, ambiguous route or unknown format |
| 5 | Facade external execution permission denied |
| 6 | Enforced CLI/facade limit exceeded |
| 124 | Facade deadline or XML probe timeout |
| 130 | Cooperative cancellation requested via SIGINT/SIGTERM/SIGHUP handler |

Optional nested backend error variants are mapped conservatively to CONVERSION_FAILED/1, not classified by text matching or guessed variant names. Thus backend-internal timeout/cancellation errors may report 1 rather than the direct facade's 124/130. See the actual sibling backend API before refining that mapping.

## XML probe details

`xml-probe` is a CLI-local structural inspection implementation, not an alias for an uninspected core XML schema probe. It reports namespace URI + local-name arrays, avoiding prefix-dependent paths and ambiguous slash-delimited strings. Default element namespaces do not apply to unprefixed attributes. Prefix aliases resolve to the same expanded-name path; rebinding remains distinct. Attribute/text values are intentionally not returned.

The entire input is inspected; limits fail rather than silently return partial statistics. Defaults: 16 MiB input, depth 128, 10,000 unique element paths, 100,000 attributes and 30,000 ms cooperative deadline. Input is UTF-8 XML 1.0 only. DTD/DOCTYPE and custom/external entities are rejected; predefined/numeric references are checked. Structural validity checks are not XSD/Relax NG/full conformance validation. CSV column selection, Cartesian expansion policy and schema inference are not implemented here.

The implementation reads the bounded whole input into memory and builds path statistics. The byte/depth/path budgets are not a hard RSS bound or parser-event allocation quota. In particular, namespace declarations may allocate inside the parser before the callback can enforce an attribute limit; use OS memory/process limits with hostile XML.

## Cancellation and security

The process installs one ctrlc handler that requests cooperative cancellation and shares the facade cancellation token. Its termination feature also handles SIGTERM/SIGHUP on Unix. No forced exit, process-group manipulation or signal handler is installed by the library entry point. An embedding program owns global signal registration.

Conversion/deadline/export guarantees remain those of the actual facade/backend. Arbitrary in-process converter work and blocking filesystem operations cannot be forcibly interrupted. Export is synchronous/non-atomic; the CLI does not claim a hard job deadline includes it. A late cancellation after successful durable export does not convert that completed success into an error.

Sources, diagnostics, namespace URIs, manifests and exported text can contain sensitive data. No sandbox, prompt-injection filtering, antivirus, codec validation, cloud backend or automatic model downloading is added. Only trusted configured converter code is invoked. See `docs/SECURITY.md`.

## Development

```sh
# Actual workspace compatibility checks:
cargo test -p omniattach-cli
cargo clippy -p omniattach-cli --all-targets -- -D warnings
cargo test -p omniattach-cli --features workspace

# Standalone CLI/inspection checks (no fake sibling crates):
cd crates/omniattach-cli
./verification/check.sh
```

`tests/cli.rs` tests parser/transport/probe behavior; its RecordingEngine is an explicit test double for the CLI transport boundary, not evidence of real document/media conversion. `tests/facade.rs` exercises actual facade conversion/export/planning when your production workspace has the real sibling source; these tests were supplied but **not run** in this delivery.

The independent Cargo.lock is under `verification/`. Apache-2.0 license. No sibling source, tool/model binaries, targets, vendored dependencies or root workspace replacements are included.
