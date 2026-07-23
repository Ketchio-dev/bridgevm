//! Shared fixtures for the crate's unit tests.

pub(crate) use crate::*;

pub(crate) fn unique_store(prefix: &str) -> VmStore {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    VmStore::new(root)
}

pub(crate) fn unique_trace_path(prefix: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "{prefix}-{}-{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path
}

pub(crate) fn test_manifest(name: &str) -> VmManifest {
    VmManifest::new(
        name,
        VmMode::Fast,
        Guest {
            os: "ubuntu".to_string(),
            version: None,
            arch: "arm64".to_string(),
        },
        "80GiB",
    )
}

pub(crate) fn complete_virtio_gpu_trace_sample() -> &'static str {
    r#"{"seq":1,"event":"device_init","width":1280,"height":720,"backend_3d":true}
{"seq":2,"event":"common_read","field":"device_features","device_features_sel":0,"value":27}
{"seq":3,"event":"common_read","field":"device_features","device_features_sel":1,"value":1}
{"seq":4,"event":"driver_features","select":0,"accepted":25}
{"seq":5,"event":"driver_features","select":1,"accepted":1}
{"seq":6,"event":"queue_notify","queue":0,"valid":true}
{"seq":7,"event":"command","name":"GET_CAPSET_INFO","response_name":"OK_CAPSET_INFO","response_capset_id":4,"response_capset_max_version":1,"response_capset_max_size":64}
{"seq":8,"event":"command","name":"GET_CAPSET","response_name":"OK_CAPSET","capset_id":4,"capset_version":1}
{"seq":9,"event":"command","name":"RESOURCE_CREATE_BLOB","response_name":"OK_NODATA"}
{"seq":10,"event":"command","name":"CTX_CREATE","response_name":"OK_NODATA","context_init":4}
{"seq":11,"event":"command","name":"SUBMIT_3D","response_name":"OK_NODATA","fenced":true,"submit_size":16}
{"seq":12,"event":"fence_create","ctx_id":1,"ring_idx":0,"fence_id":9,"backend_accepted":true,"outcome":"parked"}
{"seq":13,"event":"fence_deliver","ctx_id":1,"ring_idx":0,"fence_id":9,"used_len":24}
"#
}
