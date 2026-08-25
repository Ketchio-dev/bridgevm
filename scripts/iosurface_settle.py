"""Wait for IOSurface activity followed by stable full-frame content."""
import time

def wait(sample,initial_seed,timeout_ms,settle_ms,clock=time.monotonic_ns,pause=time.sleep):
    deadline=clock()+timeout_ms*1_000_000; digest=None; settled_at=None; latest=None
    while clock()<=deadline:
        current=sample(); observed,current_digest,_=current
        if current_digest!=digest: digest=current_digest; settled_at=clock()+settle_ms*1_000_000
        latest=current
        if observed!=initial_seed and settled_at is not None and clock()>=settled_at: return latest
        pause(0.025)
    raise RuntimeError("active IOSurface content did not settle after seed advance")
