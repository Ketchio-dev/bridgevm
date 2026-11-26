"""Keep the two development captures tied to their dated captions and reviewed pixels."""
import hashlib
import re


DESKTOP = "images/windows-desktop-20261116.webp"
SETUP = "images/mac-setup-20261126.webp"
ORIGINAL = "docs/media/windows-hvf-desktop-20261116.webp"


def check_image(page, html, site, root):
    assert sorted(image.get("src", "") for image in page.images) == sorted(
        [DESKTOP, SETUP, "icon.png", "icon.png"]
    ), "expected both product captures and both brand marks"

    def capture(src, width, height):
        image = next(image for image in page.images if image["src"] == src)
        assert (image.get("width"), image.get("height")) == (str(width), str(height)), src
        figures = re.findall(r"<figure\b[^>]*>(.*?)</figure>", html, re.S)
        figure, = [figure for figure in figures if f'src="{src}"' in figure]
        caption = re.search(r"<figcaption\b[^>]*>(.*?)</figcaption>", figure, re.S)
        assert caption, f"missing caption for {src}"
        return image.get("alt", ""), caption.group(1)

    alt, caption = capture(DESKTOP, 1440, 900)
    assert all(term in alt for term in ("Windows 11 Arm", "Test Mode"))
    assert all(term in caption for term in (
        "November 16, 2026", "M5 development Mac", "Test Mode", "experimental test-signed display driver",
        f'href="https://github.com/Ketchio-dev/bridgevm/blob/main/{ORIGINAL}"',
    )), "desktop capture provenance is incomplete"
    alt, caption = capture(SETUP, 810, 385)
    assert all(term in alt for term in ("ISO", "disk"))
    assert all(term in caption for term in ("macOS", "development build", "November 26, 2026"))

    # Decode the public 2560×1600 lossless capture to PPM, then use cwebp
    # -resize 1440 900 -q 92 -m 6 -sharp_yuv. The full frame retains the watermark.
    assert hashlib.sha256((root / ORIGINAL).read_bytes()).hexdigest() == "9908656d2d261e4c26486e1d322788d4e0faa26db709b81a78f9679cb9384c6e"
    assert hashlib.sha256((site / DESKTOP).read_bytes()).hexdigest() == "9d254be58516bdf2a7f87e5ef5f8cd6e520fa84890bf075dd71b31434e1c456e"
    # The English Welcome window from 36c07e38 was converted to sRGB, then cropped to its two source cards.
    assert hashlib.sha256((site / SETUP).read_bytes()).hexdigest() == "37714230466f0ba4a6264655132047382d8329c27ae84ccf8013e54b7d57a7de"
