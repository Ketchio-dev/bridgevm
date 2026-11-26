"""Keep the two development captures tied to their dated captions and reviewed pixels."""
import hashlib
import re


DESKTOP = "images/windows-desktop-20261126.webp"
SETUP = "images/mac-setup-20261126.webp"
ORIGINAL = "docs/media/windows-hvf-import-20261126.webp"


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

    alt, caption = capture(DESKTOP, 1280, 800)
    assert all(term in alt for term in ("Windows 11 Arm", "desktop", "taskbar"))
    assert all(term in caption for term in (
        "November 26, 2026", "installed-disk import", "M5 development Mac", "3D off",
        f'href="https://github.com/Ketchio-dev/bridgevm/blob/main/{ORIGINAL}"',
    )), "desktop capture provenance is incomplete"
    alt, caption = capture(SETUP, 810, 385)
    assert all(term in alt for term in ("ISO", "disk"))
    assert all(term in caption for term in ("macOS", "development build", "November 26, 2026"))

    # Decode ORIGINAL to PNG, then encode the full 1280×800 frame with cwebp -q 86 -m 6 -sharp_yuv.
    assert hashlib.sha256((root / ORIGINAL).read_bytes()).hexdigest() == "fb10906d31714b08152aedb90af59a69f76766df6e36c9afdf91e6a69bc4d907"
    assert hashlib.sha256((site / DESKTOP).read_bytes()).hexdigest() == "823a5f7dbb9aa41a8feed9bd096177a60254917a430d2a9d5079e34e107af24e"
    # The English Welcome window from 36c07e38 was converted to sRGB, then cropped to its two source cards.
    assert hashlib.sha256((site / SETUP).read_bytes()).hexdigest() == "37714230466f0ba4a6264655132047382d8329c27ae84ccf8013e54b7d57a7de"
