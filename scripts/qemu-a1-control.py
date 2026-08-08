#!/usr/bin/env python3
"""A1 control experiment: boot the canonical Windows image under QEMU's HVF
backend (userspace GICv3, hv_vcpu_set_pending_interrupt before every run; zero
hv_gic_* imports) on the SAME host kernel. Success = the image's own bvagent
service reaches READY over virtio-serial. Clean poweroff between boots."""
import os, socket, subprocess, sys, time
from pathlib import Path

W = Path.home() / "BridgeVM/work"
OUT = Path.home() / f"BridgeVM/runs/qemu-a1-control-{time.strftime('%H%M%S')}"
OUT.mkdir(parents=True, exist_ok=True)
DISK = W / "qemu-a1-control.raw"
VARS = W / "qemu-a1-control-vars.fd"
BOOTS = int(os.environ.get("BOOTS", "10"))
TIMEOUT = int(os.environ.get("BOOT_TIMEOUT", "600"))

if not DISK.exists():
    subprocess.run(["cp", "-c", str(W / "canonical-attach-resident-20260731.raw"), str(DISK)], check=True)
    subprocess.run(["cp", str(W / "canonical-attach-resident-20260731-vars.fd"), str(VARS)], check=True)

def one_boot(i: int) -> tuple[str, float]:
    sock_path = f"/tmp/qemu-a1-agent-{os.getpid()}-{i}.sock"
    serial = OUT / f"serial-{i}.log"
    for p in (sock_path,):
        try: os.unlink(p)
        except FileNotFoundError: pass
    cmd = [
        "qemu-system-aarch64", "-M", "virt,accel=hvf,gic-version=3",
        "-cpu", "host", "-smp", "4", "-m", "6144",
        "-drive", "if=pflash,format=raw,readonly=on,file=/opt/homebrew/share/qemu/edk2-aarch64-code.fd",
        "-drive", f"if=pflash,format=raw,file={VARS}",
        "-drive", f"file={DISK},if=none,id=d0,format=raw",
        "-device", "nvme,drive=d0,serial=bvnvme",
        "-device", "ramfb",
        "-device", "qemu-xhci", "-device", "usb-kbd", "-device", "usb-tablet",
        "-device", "virtio-serial-pci",
        "-chardev", f"socket,id=agent,path={sock_path},server=on,wait=off",
        "-device", "virtserialport,chardev=agent,name=org.bridgevm.agent",
        "-display", "none", "-serial", f"file:{serial}",
        "-monitor", "none",
    ]
    t0 = time.time()
    qemu = subprocess.Popen(cmd, stdout=open(OUT / f"qemu-{i}.log", "w"), stderr=subprocess.STDOUT)
    agent_log = open(OUT / f"agent-{i}.log", "wb")
    verdict = "no-ready"
    sock = None
    buf = b""
    sent_off = False
    try:
        deadline = t0 + TIMEOUT
        while time.time() < deadline:
            if qemu.poll() is not None:
                verdict = f"qemu-exit-{qemu.returncode}" if not sent_off else verdict
                break
            if sock is None:
                try:
                    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
                    sock.settimeout(1.0)
                    sock.connect(sock_path)
                except OSError:
                    sock = None
                    time.sleep(1)
                    continue
            try:
                data = sock.recv(4096)
                if data:
                    buf += data
                    agent_log.write(data); agent_log.flush()
            except socket.timeout:
                pass
            except OSError:
                sock = None
                continue
            if not sent_off and b"READY " in buf:
                verdict = f"READY@{time.time()-t0:.0f}s"
                import base64
                cmd = base64.b64encode("shutdown /p /f".encode()).decode()
                sock.sendall(f"RUN {cmd}\n".encode())
                sent_off = True
                deadline = time.time() + 240
        if sent_off:
            # wait for guest poweroff -> qemu exit
            end = time.time() + 240
            while time.time() < end and qemu.poll() is None:
                time.sleep(2)
    finally:
        if qemu.poll() is None:
            qemu.kill()
            verdict += "+forced-kill"
        qemu.wait()
        agent_log.close()
        if sock:
            sock.close()
        try: os.unlink(sock_path)
        except FileNotFoundError: pass
    return verdict, time.time() - t0

results = []
for i in range(1, BOOTS + 1):
    verdict, dt = one_boot(i)
    line = f"boot {i}: {verdict} total={dt:.0f}s"
    print(line, flush=True)
    results.append(verdict)
    (OUT / "summary.log").write_text("\n".join(
        f"boot {n+1}: {v}" for n, v in enumerate(results)) + "\n")
    time.sleep(3)

ready = sum(1 for v in results if v.startswith("READY"))
print(f"RESULT: {ready}/{len(results)} READY under QEMU-hvf userspace-GIC out={OUT}")
