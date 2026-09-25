"""Deterministic monotonic clock for the glyph latency synthetic fixture."""


class FakeTime:
    def __init__(self):
        self.now = 0

    def monotonic_ns(self):
        return self.now

    def sleep(self, seconds):
        self.now += int(seconds * 1_000_000_000)
