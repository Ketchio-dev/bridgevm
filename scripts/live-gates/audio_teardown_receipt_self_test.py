"""Mutation fixture for the ten-run audio receipt."""
import tempfile
from pathlib import Path

def run(assess):
    with tempfile.TemporaryDirectory() as tmp:
        out=Path(tmp); (out/"runs").mkdir()
        valid="out_dir=x\na5_audio=pass\nframes_rendered=48000\ndrops=0\ncallback_errors=0\nunexpected_callback_errors=0\nteardown_reenqueue_refusals=3\nstop_errors=0\ndispose_errors=0\nguest_sound_device_status=OK\n"
        for n in range(1,11):
            path=out/f"runs/run{n}"; path.mkdir(); (path/"summary.txt").write_text(valid)
        assert len(assess(out)[1])==10; target=out/"runs/run7/summary.txt"
        mutations=[("callback_errors=0","callback_errors=1"),("drops=0","drops=1"),("frames_rendered=48000","frames_rendered=0"),("callback_errors=0","callback_errors=0\ncallback_errors=0"),("guest_sound_device_status=OK","unknown=1")]
        for old,new in mutations:
            saved=target.read_text(); target.write_text(saved.replace(old,new))
            try: good=len(assess(out)[1])
            except ValueError: good=9
            assert good==9; target.write_text(saved)
    print("PASS: audio teardown receipt verifier and mutations")
