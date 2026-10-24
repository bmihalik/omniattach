# CLI architecture

```mermaid
flowchart TD
    Args[OS argv] --> Clap[Clap command and option parser]
    Clap --> Select{Command}
    Select --> Cap[capabilities]
    Select --> Probe[probe: bounded prefix reader]
    Select --> XML[xml-probe: bounded UTF-8 XML inspection]
    Select --> Plan[plan]
    Select --> Convert[convert]
    Cap --> Engine[Engine transport trait]
    Plan --> Engine
    Convert --> Engine
    Engine --> Facade[Production FacadeEngine]
    Engine --> Inspect[No-facade InspectionOnlyEngine]
    Facade --> Actual[Real sibling omniattach crate]
    Actual --> Bundle[FacadeBundle owns scratch data]
    Bundle --> Export[Export to new durable directory]
    Export --> JSON[Success envelope on stdout]
    Probe --> JSON
    XML --> JSON
    Cap --> JSON
    Plan --> JSON
    Runtime[Shared cancellation and policy] --> Facade
    Progress[Facade progress] --> Stderr[stderr NDJSON]
    Errors[Typed CLI errors and exit codes] --> Stderr
```

```mermaid
sequenceDiagram
    participant RagDBMan
    participant CLI
    participant Facade
    participant Disk
    RagDBMan->>CLI: argv, no shell, no implicit network
    CLI->>CLI: validate options and new output destination
    CLI->>Facade: configured request and execution context
    Facade-->>CLI: owned conversion bundle
    CLI->>Disk: bundle.export(new_directory)
    alt Export succeeds
        Disk-->>CLI: manifest.json and declared assets durable
        CLI-->>RagDBMan: one JSON success on stdout, exit 0
    else Conversion or export fails
        CLI-->>RagDBMan: JSON error on stderr, nonzero exit
    end
```

## Boundaries

`args.rs` owns argv. `service.rs` owns transport validation/dispatch and the Engine interface. `facade.rs` is the only dependency-specific production bridge. `probe.rs`/`xml.rs` provide inspection without pretending to perform document conversion. `runtime.rs` owns a lightweight cancellation facade; process main, not the library, installs global signal handling.

Only the real sibling facade may convert files. There is no hidden portable conversion copy or substitute core model in the CLI. Inspection-only binaries explicitly advertise `conversion_enabled: false`. The production default feature enables the sibling facade. The isolated verification manifest does not.

The capability output describes registered contracts, never installed tool health. Registering external contracts is separate from conversion permission. XML path statistics have their own versioned CLI schema; they do not claim equivalence to unpublished core XML types.
