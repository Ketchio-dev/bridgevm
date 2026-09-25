#[cfg(all(bridgevm_non_debug_profile, debug_assertions))]
compile_error!("hvf-runner non-debug Cargo profile cannot enable debug assertions");
