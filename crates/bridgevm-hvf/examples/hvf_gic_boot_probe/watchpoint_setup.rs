use super::*;

pub(crate) fn watchpoint_config() -> (Option<u64>, u64) {
    // Hardware watchpoint on a firmware address (default the poll target
    // 0x5ffdf798). Opt-in via BRIDGEVM_WATCH=1, or provide another address.
    let watch_addr = std::env::var("BRIDGEVM_WATCH").ok().map(|value| {
        let value = value.trim();
        if value == "1" {
            WATCH_TARGET
        } else {
            parse_u64(value).unwrap_or(WATCH_TARGET)
        }
    });
    (watch_addr, watch_addr.unwrap_or(WATCH_TARGET))
}
