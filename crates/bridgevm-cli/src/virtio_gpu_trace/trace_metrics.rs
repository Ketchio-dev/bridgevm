//! Percentiles and bounded renderer-failure summaries for virtio-gpu JSONL reports.

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SubmitFailureKey {
    pub(crate) response: String,
    pub(crate) renderer_status: Option<i64>,
    pub(crate) command_id: Option<u64>,
    pub(crate) resource_id: Option<u64>,
    pub(crate) resource_found: Option<bool>,
    pub(crate) resource_backed: Option<bool>,
}

pub(crate) fn json_i64(value: &serde_json::Value, key: &str) -> Option<i64> {
    match value.get(key)? {
        serde_json::Value::Number(value) => value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|unsigned| unsigned.try_into().ok())),
        serde_json::Value::String(value) => value.trim().parse().ok(),
        _ => None,
    }
}

pub(crate) fn percentile_ns(values: &[u64], numerator: usize, denominator: usize) -> u64 {
    if values.is_empty() || denominator == 0 {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = sorted
        .len()
        .saturating_mul(numerator)
        .div_ceil(denominator)
        .max(1);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

/// Command ids are protocol-specific: naming a Venus id from the VirGL table
/// invents a wrong opcode, so Venus traces report the raw id instead.
pub(crate) fn submit_command_name(command_id: Option<u64>, venus: bool) -> &'static str {
    match command_id {
        Some(id) if !venus => virgl_command_name(id),
        _ => "UNKNOWN",
    }
}

fn virgl_command_name(command_id: u64) -> &'static str {
    // Space-separated to stay within this file's structural budget.
    const NAMES: &str = concat!(
        "NOP CREATE_OBJECT BIND_OBJECT DESTROY_OBJECT SET_VIEWPORT_STATE SET_FRAMEBUFFER_STATE ",
        "SET_VERTEX_BUFFERS CLEAR DRAW_VBO RESOURCE_INLINE_WRITE SET_SAMPLER_VIEWS SET_INDEX_BUFFER ",
        "SET_CONSTANT_BUFFER SET_STENCIL_REF SET_BLEND_COLOR SET_SCISSOR_STATE BLIT ",
        "RESOURCE_COPY_REGION BIND_SAMPLER_STATES BEGIN_QUERY END_QUERY GET_QUERY_RESULT ",
        "SET_POLYGON_STIPPLE SET_CLIP_STATE SET_SAMPLE_MASK SET_STREAMOUT_TARGETS ",
        "SET_RENDER_CONDITION SET_UNIFORM_BUFFER SET_SUB_CTX CREATE_SUB_CTX DESTROY_SUB_CTX ",
        "BIND_SHADER SET_TESS_STATE SET_MIN_SAMPLES SET_SHADER_BUFFERS SET_SHADER_IMAGES ",
        "MEMORY_BARRIER LAUNCH_GRID SET_FRAMEBUFFER_STATE_NO_ATTACH TEXTURE_BARRIER ",
        "SET_ATOMIC_BUFFERS SET_DEBUG_FLAGS GET_QUERY_RESULT_QBO TRANSFER3D END_TRANSFERS ",
        "COPY_TRANSFER3D SET_TWEAKS CLEAR_TEXTURE PIPE_RESOURCE_CREATE PIPE_RESOURCE_SET_TYPE ",
        "GET_MEMORY_INFO SEND_STRING_MARKER LINK_SHADER CREATE_VIDEO_CODEC DESTROY_VIDEO_CODEC ",
        "CREATE_VIDEO_BUFFER DESTROY_VIDEO_BUFFER BEGIN_FRAME DECODE_MACROBLOCK DECODE_BITSTREAM ",
        "ENCODE_BITSTREAM END_FRAME CLEAR_SURFACE GET_PIPE_RESOURCE_LAYOUT",
    );
    usize::try_from(command_id)
        .ok()
        .and_then(|index| NAMES.split_whitespace().nth(index))
        .unwrap_or("UNKNOWN")
}

pub(crate) fn option_label<T: std::fmt::Display>(value: Option<T>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub(crate) fn option_bool_label(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}
