#!/usr/bin/env python3
"""Keep the static preview honest about release and capability boundaries."""
import hashlib
import json
import re
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlparse, unquote

from landing_image_contract import check_image

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "site"


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = set()
        self.links = []
        self.images = []
        self.summary = []
        self.in_summary = False

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            assert attrs["id"] not in self.ids, f"duplicate ID {attrs['id']}"
            self.ids.add(attrs["id"])
        if tag == "a":
            self.links.append(attrs.get("href", ""))
        if tag == "link":
            self.links.append(attrs.get("href", ""))
        if tag == "img":
            self.images.append(attrs)
        if attrs.get("id") == "engine-summary":
            self.in_summary = True

    def handle_endtag(self, tag):
        if tag == "p" and self.in_summary:
            self.in_summary = False

    def handle_data(self, data):
        if self.in_summary:
            self.summary.append(data)


def main():
    registry = json.loads((ROOT / "capabilities/windows-hvf.json").read_text())
    html = (SITE / "index.html").read_text()
    css = (SITE / "styles.css").read_text()
    page = Page()
    page.feed(html)
    assert html.startswith("<!doctype html>")
    assert '<html lang="en">' in html
    assert registry["wording"]["product_state_label"] in html
    assert registry["product_state"] == "ENGINEERING_PREVIEW"
    assert "".join(page.summary) == registry["wording"]["engine_summary"]
    for required in ("not notarized", "3D acceleration is excluded", "does not redistribute these drivers", "v0.1.0 download opens in Korean"):
        assert required in html, f"landing boundary absent: {required}"
    check_image(page, html, SITE, ROOT)
    assert "@import" not in css and "url(http" not in css
    # The site mark is rendered from the native BridgeVMMark + app-icon generator.
    assert (SITE / "icon.png").read_bytes().startswith(b"\x89PNG\r\n\x1a\n")
    assert hashlib.sha256((SITE / "icon.png").read_bytes()).hexdigest() == "f6dcbfd1053da08638d17264cb48d4fc933ecdd68cfa2a7a3c3e3e8a2c62de3a"
    assert html.count('class="brand-logo" src="icon.png"') == 2
    assert not (SITE / "icon.svg").exists() and "brand-mark" not in css
    assert "prefers-reduced-motion:reduce" in css and "focus-visible" in css
    for href in page.links:
        assert href and not href.startswith("javascript:"), href
        if href.startswith("#"):
            assert href[1:] in page.ids, href
        elif href.startswith("https://"):
            u = urlparse(href)
            assert u.netloc == "github.com" and u.path.startswith("/Ketchio-dev/bridgevm"), href
            if u.path not in {
                "/Ketchio-dev/bridgevm",
                "/Ketchio-dev/bridgevm/releases/tag/v0.1.0",
                "/Ketchio-dev/bridgevm/releases/download/v0.1.0/BridgeVM-v0.1.0.dmg",
            }:
                prefix = "/Ketchio-dev/bridgevm/blob/main/"
                assert u.path.startswith(prefix), href
                target = ROOT / unquote(u.path[len(prefix):])
                assert target.is_file() and target.resolve().is_relative_to(ROOT), href
                if u.fragment:
                    assert target.suffix == ".md", href
                    headings = re.findall(r"^#{1,6}\s+(.+)$", target.read_text(), re.M)
                    slugs = [re.sub(r"[^\w -]", "", h.lower()).replace(" ", "-") for h in headings]
                    assert unquote(u.fragment) in slugs, href
        else:
            assert (SITE / href).is_file(), href
    print(f"index.html: PASS ({len(page.links)} links, preview claims and captures)")


if __name__ == "__main__":
    main()
