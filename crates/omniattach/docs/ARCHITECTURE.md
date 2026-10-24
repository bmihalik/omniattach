# Facade architecture

```mermaid
flowchart TD
    Request[ConversionRequest: file + goal + options] --> Stage[Private staged source + SHA-256]
    Stage --> Detect[Conservative prefix/extension detection]
    Detect --> Plan[Explicit format/goal route resolution]
    Registry[Immutable registered contracts] --> Plan
    Policy[External permission + byte budgets + cancellation] --> Plan
    Plan --> Decision{Exactly one permitted route?}
    Decision -->|No| Error[Typed error; no fallback]
    Decision -->|Yes| Converter[Converter trait]
    Converter --> Text[Portable text]
    Converter --> Docs[DocumentAdapter: caller-supplied actual API]
    Converter --> External[Optional ExternalAdapter: backend 0.1 contract]
    Converter --> Pipeline[ArtifactPipeline: explicit intermediate]
    Docs --> Payload[Namespaced original schema payload]
    External --> Payload
    Converter --> Assets[Declared output files]
    Assets --> Check[Containment/count/byte validation]
    Payload --> Bundle[FacadeBundle owns scratch workspace]
    Check --> Bundle
    Bundle --> Read[Borrow payloads / access files]
    Bundle --> Export[Explicit export to new directory]
```

```mermaid
sequenceDiagram
    participant Caller
    participant Facade
    participant Registry
    participant Converter
    participant Scratch
    Caller->>Facade: convert(request, context)
    Facade->>Scratch: stage source and hash copied bytes
    Facade->>Registry: format + goal + explicit optional route
    alt Zero or ambiguous eligible routes
        Registry-->>Facade: typed selection error
        Facade->>Scratch: drop temporary workspace
        Facade-->>Caller: error, no fallback
    else Unique allowed route
        Registry-->>Facade: configured converter
        Facade->>Converter: staged input + options + job context
        Converter-->>Facade: original payloads + text + declared artifacts
        Facade->>Scratch: validate output paths/count/bytes
        Facade-->>Caller: owned FacadeBundle
        Caller->>Facade: export(optional new directory)
    end
```

```mermaid
flowchart LR
    Original[Original source identity] --> First[Explicit first converter]
    First --> Intermediate[Selected artifact + actual intermediate hash]
    Intermediate --> Second[Explicit second converter]
    Second --> Final[Final text and payload]
    First --> Assets1[stage-1 declared artifacts]
    Second --> Assets2[stage-2 declared artifacts]
    Original --> Edge[Pipeline relationship payload]
    Intermediate --> Edge
    Assets1 --> Result[FacadeBundle]
    Assets2 --> Result
    Final --> Result
    Edge --> Result
```

## Schema and compatibility

The facade envelope version is `omniattach/facade/0.1`. It holds source identity, route/goal/format, elapsed time, warnings, text fragments, payloads and owned artifact descriptors. Payloads are namespaced by the actual producer schema, not translated through guessed JSON fields. This avoids presenting an unpublished core/document schema as if it had been verified.

`omniattach::core`, `::documents` and `::backends` are optional sibling namespaces. The portable facade has its own route and progress contracts; these are not aliases for an uninspected core planner. The actual workspace should eventually consolidate planner/progress/types rather than maintain two canonical systems. No serialization equivalence is promised today.

A converter is configured once and must be Send + Sync. The facade engine is immutable after build, so multiple caller-controlled threads can share an engine. Every conversion has its own temporary directory. Concurrency scheduling, model loading, GPU access, backpressure, retry and task persistence remain caller/backend responsibilities.

## Portability

The portable facade uses filesystem/encoding operations without POSIX process APIs. Only Linux was tested. Optional subprocess platform guarantees come from the actual backend runner. A registered `ExecutionKind::InProcess` is a declaration by trusted application code, not runtime proof that a closure never starts a process.
