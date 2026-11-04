//! Clipboard images through the resident service loop (filled in step 1b).

use super::*;

#[test]
fn clipx_parses_image_and_text_fields() {
    let digest = "ab".repeat(32);
    let path = base64_encode(br"C:\T\BridgeVM-clip\guest.png");
    let rest = format!("3 2 33 {digest} {path} {}", base64_encode(b"hi"));
    let (image, text) = parse_clipx(&rest).expect("well formed");
    let image = image.expect("image");
    assert_eq!((image.width, image.height, image.bytes), (3, 2, 33));
    assert_eq!(image.path.as_deref(), Some(r"C:\T\BridgeVM-clip\guest.png"));
    assert_eq!(text, base64_encode(b"hi"));
    assert_eq!(parse_clipx("0 0 0 - - "), Ok((None, "")));
    assert!(parse_clipx("0 0 0 -").is_err());
    assert!(parse_clipx("x 0 0 - - ").is_err());
}
