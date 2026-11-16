# Hub provisioning progress

See [MODEL_PROVISIONING.md](MODEL_PROVISIONING.md) for the Hub-only API and migration.

The generic fetcher/polling wrapper is removed. ModelStore::provision_with_progress
now accepts only a HubModelSpec, ProvisionOptions and observer. It uses the official
Hub SDK Progress callbacks, not staging-file lengths (the SDK may preallocate files).
Progress schema is omniattach/model-progress/0.2; per-file total_bytes is available
when the SDK reports it. Do not use a byte safety cap as a percentage denominator.

Existing explicit local inference and its separate execution progress are unchanged.
CLI example modes are text, ndjson and none. stderr contains progress/structured
errors; stdout contains final model/status JSON. No model download is implicit.
Observer callbacks run on the provisioning worker thread; use a UI event channel.
