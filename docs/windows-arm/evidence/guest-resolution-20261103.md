# Windows desktop resolution, 2026-11-03

Live single runs (evidence rank 2) on one Mac (Apple M4 Max, macOS 27.0.1),
each a cold boot of an APFS clone of the staged Windows 11 Arm test image and
its UEFI variables, through `scripts/perf/vm_bench.py` on the internal SSD.
The image is the installed test image every performance run uses; it had only
ever run at 800x600.

## Firmware

The product firmware is rebuilt from the same pinned EDK2 source with patch
`0002-ramfb-desktop-resolutions.patch`: the ramfb driver offers 15 modes up to
2560x1600 instead of 640x480, 800x600 and 1024x768. Two builds of the patched
source produced the same digest
(`241ce0117723bf30008e83ce1329528c3486398ce6256d943cd1300defffd8ff`). The
previous firmware (`b1dc201b…`) rebuilt byte for byte once its original build
path and stack cookie, read back from the binary, were used.

## What Windows does with a new firmware mode

| run | firmware mode | Windows desktop | what the frame showed |
|---|---|---|---|
| `rm-1920` | 1920x1080 | 800x600 (saved) | the 800x600 desktop scaled to 1440x1080, black bars left and right |
| #461 runs (`r1024`) | 1024x768 | 800x600 (saved) | the 800x600 desktop scaled to fill 1024x768 (taskbar 60 px tall, 48 px at 100 %) |

An installed Windows keeps the desktop mode it saved for the display and
scales it into whatever mode the firmware set. Windows listed exactly one mode,
the firmware's (`desktop-modes` 1). The 2026-11-03 "1024x768 desktop" reading
of PR #461 described the framebuffer only; that Windows desktop stayed at
800x600.

## Switching the desktop

`vm_bench --desktop-mode native` switches the desktop to the display's largest
mode with `ChangeDisplaySettingsEx` (persisted), as choosing the recommended
resolution in Settings does:

| run | before | after | display fps | frame p95 | in-guest animation |
|---|---|---|---:|---:|---:|
| `dn2-1920` | 800x600 | 1920x1080 | 58.0 | 27.7 ms | 186 fps |
| `dn2-2560` | 800x600 | 2560x1600 | 61.8 | 25.7 ms | 86 fps |

The frame after the switch shows a native 1920x1080 desktop (taskbar with a
search box). Scaled instead of native, 2560x1600 had reached only 28.7 fps
(`rm-2560`).

## Automatic switch (this change)

`ra-default`, two boots of one clone, no user choice:

- boot 1: the VMM added the default 1280x800 preference, the agent service
  started and the probe's switch command logged
  `BVDESKTOP before=800x600 target=1280x800 result=0`; the harness then read a
  1280x800 desktop.
- boot 2: no variable change and no switch command; the desktop was still
  1280x800.

## Cost

Idle VMM CPU rises with the framebuffer, because the exporter compares the
whole frame on every tick: 5.4 % at 1024x768, 5.8 % at 1280x800, 7.4–7.7 % at
1920x1080 and 8.1–10.0 % at 2560x1600 (single runs, load 4–7). This change
compares unchanged frames in place instead of copying them first. Same
binary with an environment toggle, 2560x1600, export every 16 ms, two
interleaved pairs of two boots: in place 5.8/6.0 and 7.7/8.3 %, copy first
9.2/9.6 and 8.8/8.0 % (medians 6.9 vs 9.0 %; the second pair overlaps). On
the host, one unchanged tick costs 0.64 instead of 1.18 ms at 2560x1600 and
0.09 instead of 0.20 ms at 1280x800.
