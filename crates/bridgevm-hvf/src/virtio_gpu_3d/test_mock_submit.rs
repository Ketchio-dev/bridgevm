// Configurable mock SUBMIT_3D response for renderer-diagnostic tests.

pub(crate) fn mock_submit_3d(
    backend: &mut std::sync::Arc<std::sync::Mutex<MockBackend>>,
    ctx_id: u32,
    cmdbuf: &[u8],
) -> Submit3dResult {
    let mut inner = backend.lock().unwrap();
    inner.submits.push((ctx_id, cmdbuf.to_vec()));
    inner.submit_result.unwrap_or_else(Submit3dResult::accepted)
}
