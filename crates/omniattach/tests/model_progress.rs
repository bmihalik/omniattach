#![cfg(feature = "backends")]
use omniattach::{model_progress::*, model_store::*};
use std::sync::Mutex;
#[test]
fn offline_failure_delivers_structured_terminal_event() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ModelStore::new(tmp.path()).unwrap();
    let spec = texteller3_onnx_spec().unwrap();
    let events = Mutex::new(Vec::new());
    let observer = |e: &ModelProgressEvent| events.lock().unwrap().push(e.clone());
    assert!(
        store
            .provision_with_progress(&spec, &ProvisionOptions::default(), &observer)
            .is_err()
    );
    let events = events.lock().unwrap();
    assert_eq!(events.first().unwrap().stage, ModelProvisionStage::Started);
    assert_eq!(events.last().unwrap().stage, ModelProvisionStage::Failed);
    assert!(
        events
            .iter()
            .all(|e| e.schema_version == "omniattach/model-progress/0.2")
    );
}
#[test]
fn cancelled_operation_does_not_report_completed() {
    let tmp = tempfile::tempdir().unwrap();
    let store = ModelStore::new(tmp.path()).unwrap();
    let options = ProvisionOptions::default();
    options.cancellation.cancel();
    let events = Mutex::new(Vec::new());
    let observer = |e: &ModelProgressEvent| events.lock().unwrap().push(e.stage);
    assert!(
        store
            .provision_with_progress(&texteller3_onnx_spec().unwrap(), &options, &observer)
            .is_err()
    );
    assert_eq!(
        events.lock().unwrap().last(),
        Some(&ModelProvisionStage::Cancelled)
    );
}
