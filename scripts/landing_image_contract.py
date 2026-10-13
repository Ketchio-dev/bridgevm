"""Admit only the dated Windows Test Mode image, never an unqualified hero."""
import hashlib


def check_image(page, html, site, root):
    assert len(page.images) == 1
    image = page.images[0]
    assert image.get("src") == "images/windows-guest-20260813.webp"
    assert image.get("width") == "1200" and image.get("height") == "900"
    assert "Test Mode" in image.get("alt", "") and "not a current release" in image["alt"]
    caption = html.split("<figcaption>", 1)[1].split("</figcaption>", 1)[0]
    assert "August 2026 archive" in caption and "Windows Test Mode" in caption
    assert "not a current release or app UI screenshot" in caption
    assert "docs/media/windows-hvf-desktop.png" in caption
    assert hashlib.sha256((site / image["src"]).read_bytes()).hexdigest() == "a584829b844c87d7ffe6571de35a3e6b3643d2ee3a9a5e935e414ade4a521b03"
    assert hashlib.sha256((root / "docs/media/windows-hvf-desktop.png").read_bytes()).hexdigest() == "727cac7134cfd13538da4453b47b15f9f26d40d9c51610a6d2e58361d4587db8"
