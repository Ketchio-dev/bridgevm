const PIN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/firmware/bridgevm-pc-dxe-entry.sha256"
));

pub(super) fn expected() -> Result<&'static str, String> {
    parse(PIN)
}

pub(super) fn parse(pin: &str) -> Result<&str, String> {
    if pin.len() != 65 || !pin.ends_with('\n') {
        return Err("firmware pin must contain one 64-digit digest and newline".into());
    }
    let digest = &pin[..64];
    if !digest
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("firmware pin must contain only lowercase hexadecimal digits".into());
    }
    Ok(digest)
}
