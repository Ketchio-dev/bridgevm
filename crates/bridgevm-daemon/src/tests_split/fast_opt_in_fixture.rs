//! Owner-carrying fast lifecycle test fixture.
use super::helpers::*;
use bridgevm_api::fast_suspend_state_path;
use bridgevm_storage::{VmRuntimeState, VmStore};
use std::{env, fs, path::PathBuf};

pub(super) struct FastOptInFixture {
    pub(super) store: VmStore,
    pub(super) state_path: PathBuf,
    pub(super) lightvm_runner: PathBuf,
    pub(super) apple_vz_runner: PathBuf,
    _root: TestStoreRoot,
}

impl FastOptInFixture {
    pub(super) fn new(state: VmRuntimeState, saved_state: bool) -> Self {
        let (_root, store) = temp_store();
        let manifest = ready_fast_manifest("fast-linux");
        store.create_vm(&manifest).unwrap();
        let bundle = store.bundle_path("fast-linux");
        fs::create_dir_all(bundle.join("boot")).unwrap();
        fs::create_dir_all(bundle.join("disks")).unwrap();
        fs::write(bundle.join("boot").join("vmlinuz"), b"kernel").unwrap();
        fs::write(bundle.join("disks").join("root.raw"), b"disk").unwrap();
        let state_path = fast_suspend_state_path(&bundle, "fast-linux");
        if saved_state {
            fs::create_dir_all(state_path.parent().unwrap()).unwrap();
            fs::write(&state_path, b"saved-state").unwrap();
        }
        store.force_transition_state("fast-linux", state).unwrap();
        let lightvm_runner = store.root().join("fake-lightvm-runner");
        let apple_vz_runner = store.root().join("fake-AppleVzRunner");
        Self {
            store,
            state_path,
            lightvm_runner,
            apple_vz_runner,
            _root,
        }
    }

    pub(super) fn set_runner_env(&self, script: &str) {
        write_executable(&self.lightvm_runner, script);
        write_executable(&self.apple_vz_runner, "#!/bin/sh\nexit 0\n");
        env::set_var("BRIDGEVM_LIGHTVM_RUNNER", &self.lightvm_runner);
        env::set_var("BRIDGEVM_APPLE_VZ_RUNNER", &self.apple_vz_runner);
    }

    pub(super) fn cleanup(&self) {
        fs::remove_dir_all(self.store.root()).unwrap();
    }
}
