# B6 glyph defect observed through the active-IOSurface channel — 2026-09-07

One `t7-windows-closure` run captured the B6 defect exactly as the registry
words it: body text and menu glyphs render, the window title is blank and no
tab strip is drawn. This is one observation at one resolution and one scale.
It does not run the 3-by-3 matrix, apply a pixel mask or measure frame time,
and it promotes nothing. B6 stays OPEN.

## Sealed inputs

- tested commit: `7f31bfc8d97ebe173823f156ba99b13e6085295b`
- job: `t7-7f31bfc8-keeprunning-b6-observation-r1`, started 20:10:35Z,
  finished 21:07:15Z
- input manifest SHA-256:
  `81c56875325f705fdf3728bb9e8449f3cd4ef129bedf6b3003dfb812e18d6e06`
- probe SHA-256:
  `4385f3817072ff94819a54ee002d18c5025779f67a2d17124789182232bbd974`
- injector SHA-256:
  `10760fb4d81998197d549ec7ce78ff0eb1d4d9457c5d4ed4cdcaee6f38318afe`,
  built from this head with `KEEP_RUNNING=1` and `DISPLAY_ONLY_FIRSTBOOT=1`;
  the corrected `bvinject.cmd` inside its `boot.wim` is byte-identical to the
  tree
- viogpu3d package tree `c46486e5…`, agent `b7820834…`, source image
  `7385d200…`, vars `bec224d2…`, virglrenderer `dc596bf3…`, MoltenVK
  `e1773b59…`
- receipt SHA-256 (private == public):
  `bcb1ae8a89d2d8a0466765b176a38fa435ec5edc5d0f1363b7472ad4e2ec468b`
- retained prepared pair: image `490ae154…`, vars `bec224d2…`
- host: Mac17,9, macOS 26.5

## What the run proved

`outcome=failed`, `pass=false`, `passes=3`, `failures=1`,
`injector_boot_observed=true`, `module_identity_verified=true`.

- **Injection.** The corrected injector planted the firstboot handoff (the
  pending flag, the `BridgeVMGpuDiagnosticsProbe6` service and the Stage1
  RunOnce were all present on the retained disk); the proof boot ran three
  staged reboots, reported `[stage4] done`, and answered
  `BVFIRSTBOOT_READY`.
- **F1 pass.** `BVF1 testsigning=True viogpu_status=OK viogpu_problem=0
  vioserial_status=OK vioserial_problem=0`; `BVF1MODE device=\\.\DISPLAY2
  current=1280x1024 modes=28 has_1600x900=True`.
- **F2 pass.** After `RESIZE 1600x900`, `BVF2 device=\\.\DISPLAY2
  current=1600x900`; 185 `SET_SCANOUT` commands with `rect_w=1600,
  rect_h=900`, all `OK_NODATA`.
- **F3 partial.** `WINLIST` found `131540 … "Untitled - Notepad"`;
  `WINBOUNDS 131540 50 60 700 500 -> OK`, `WINFOCUS -> OK`, guest-side
  `BVWINDOW hwnd=131540 exists=True pid=9088 rect=50,60,700,500
  foreground=131540`. Three `text-hex` key batches were accepted. After
  `WINCLOSE -> OK` the window was still listed, now titled
  `*Untitled - Notepad`: the typed text had modified the document, so
  `WM_CLOSE` raised the save prompt instead of closing.
- **F4 measured-visible-text.** `captures/f4-notepad-focused.ppm`, SHA-256
  `a2f0bb82e2173b71bdc2dfea3d3a60ab83312310f3f0ddbc6d7e4e7887e4ecfc`, from
  `capture.env`: `source=active-cgl-iosurface iosurface_id=505
  initial_seed=30 captured_seed=31 nonblack_pixels=1439262`. This is a newly
  presented 1600x900 frame, not the 2D scanout buffer that produced the
  all-black 2026-08-20 capture.

## The frame

Notepad occupies (50,60)–(700,500). Readable: the menu bar `File Edit Format
View Help`; the banner "A new version of Notepad is available." with its
`Launch` button; the body line `Probe ABCDEFGHIJKLMNOPQRSTUVWXYZ
abcdefghijklmnopqrstuvwxyz 0123456789 !@#$%^&*()`; the status bar `Ln 1,
Col 97 100% Windows (CRLF) UTF-8`. OCR (`captures/f4-ocr.txt`) reads the
menu and body words and no title.

Blank: the title bar carries the Notepad icon and the minimize, maximize and
close glyphs, but no "Untitled - Notepad" text; no tab strip (label, close
button or `+`) is drawn at all.

Measured on the retained pixels rather than eyeballed: the dark-pixel ratio
(all channels below 110) in the title text zone, x 90–600 and y 62–90, is
0.0000; the adjacent title icon zone, x 52–90, is 0.0235; the menu text zone,
y 93–108, is 0.0284; the body text zone, y 140–155, is 0.0962. The title area
is not black, not garbled and not boxed; it is painted background where glyphs
should be.

## What it means and what it does not

This is the reproduction the 2026-09-06 record required before any draw-path
cause could be proposed: the exact user-visible failure, on the corrected
capture channel, on a timer-fixed probe, with a properly injected guest. The
draw-path investigation can now start from this frame and this trace.

It is not B6 evidence toward closure. The criterion demands 3/3 at each of
three declared resolutions and three declared scales, a verified pixel mask
and frame time within 10% of baseline; none of that was run, and one
observation at 1600x900 at 100% says nothing about repeatability.

## Two harness findings, recorded rather than fixed here

1. **Stage-4 power-off races the closure tier.** `bvgpu-firstboot.cmd` ends
   stage 4 with `shutdown /s /t 5` unless `C:\BridgeVM\keep-running.txt`
   exists. Job `t7-7f31bfc8-fixed-injector-b6-observation-r1` (receipt
   `add1e95d…`) reached `BVFIRSTBOOT_READY` and passed F1, then every later
   command timed out because the guest powered off seconds later. The sealed
   2026-08-29 injector carried no keep-running marker either. The old
   passing injector's marker state is unknown; its pass does not establish
   the same race. This run used `KEEP_RUNNING=1`.
2. **A modified document survived `WINCLOSE`; shutdown did not finish.**
   The window remained listed as `*Untitled - Notepad` after `WM_CLOSE`.
   A save prompt is a hypothesis, not an observed dialog. The subsequent
   `shutdown /s /t 0` returned zero, but the guest stayed up until the
   3000 s watchdog cancelled the run. The live display showed only wallpaper,
   with no taskbar or dialog. These observations establish incomplete close
   and shutdown, not the precise mechanism that prevented shutdown.

## Interpretation correction

The captured menu (`File/Edit/Format/View/Help`) and upgrade banner identify
the classic Notepad presentation, not the tabbed application. Absence of a tab
strip is therefore **not evidence of missing tab glyphs**. The reproduced
defect is blank caption text despite a nonempty window title reported by
WINLIST. Menu glyphs are readable; tab glyphs remain untested. Earlier wording
that grouped the absent tabs with the reproduced defect was too strong.

## Cleanup confirmed live, 2026-09-08

Job `t7-64a38e83-discard-b6-observation-r1` ran code head
`64a38e83d4c256f9552b1442d43f08f5cbd6f7e2` with the same sealed manifest
`81c56875325f705fdf3728bb9e8449f3cd4ef129bedf6b3003dfb812e18d6e06`.
It started at 01:24:48Z and its receipt finished at 01:32:57Z.
Private and public receipt SHA-256 are both
`8124b25b777e22e87d4e7ed4e7b9fd80aadd92299f450054c7e46e5ad835242a`.

The guest reported `BVDISCARD hwnd=131502 pid=9084 stopped=True`, accepted
`shutdown /s /f /t 0` with exit zero, and terminated with
`stop: PSCI 0x84000008 (system off)` rather than the prior watchdog stop.
F1 and F2 passed, F3 remained partial, and F4 was measured-visible-text.
The receipt correctly remains `pass=false`, three passes and one failure;
cleanup did not turn a failed window-close assertion into a pass.

The second active-CGL capture advanced IOSurface seed 28 to 29 at 1600x900,
with 1,439,262 nonblack pixels. PPM SHA-256:
`d3b417846a82329112a636bf3897a635feff97a95f8b49d6a7bea821c2b03028`.
Visual inspection again shows blank caption text with readable menus and
document text. This supports a second caption-defect observation, not a tab
defect or a completed B6 matrix.

## Retained trace analysis: caption draw remains unidentified

The second observation's `proof/virtio-gpu.jsonl` has SHA-256
`f2bba3fab7ff95a299864ff80bb3adc66bda21f7971be7390aaa9915e85dfe87`.
An offline analysis counted 10,306 records and 1,171 SUBMIT_3D commands.
Every submit carries a truncated `submit_prefix_hex`, not its full payload.
Consequently this trace cannot reconstruct shader creation, draw-time sampler
bindings or the complete sequence of commands inside a submit.

Tracking RESOURCE_CREATE_3D through RESOURCE_UNREF rather than treating
reused resource IDs as permanent identities found one exact 700x500 candidate:
resource 14 created at sequence 8760 and attached to context 7 at 8761.
This matches the requested window dimensions but does not identify a caption
render target. Context 7 also attaches many other extents, including the
1600x900 scanout and intermediate surfaces. Later 256x80 texture candidates
belong to contexts 12 and 25; context 25 also attaches a 704x480 surface.
Dimensions alone do not establish an atlas's role, the window owner, or the
draw that produced the missing caption.

The retained renderer source's existing `dump_cmd_streams` facility is not
a complete replacement: it drops the first 4096 bytes of each submit, skips
submits of 4096 bytes or less, and names files by content hash rather than
context and execution order. Six of this trace's submits fall below that
threshold. Using these dumps as a complete chronological replay would lose
commands and attribution.

Next required measurement is a bounded, diagnostic-only capture around the
already-reproduced Notepad scene: ordered full submit payloads with context
identity, resource lifetimes, and draw-time target/shader/sampler identities.
It must correlate to an active-IOSurface frame and cannot change blending,
buffer synchronization or atlas contents. The older login-surface evidence
does not justify a renderer fix for the current caption failure.

## Diagnostic-only ordered capture, 2026-09-08

Job `t7-caption-diagnostic-20260908-r1` used an isolated renderer, not the
installed renderer. Its source was upstream 2a173eef plus the shipped patch
and private bounded capture hooks; probe linkage was changed only on a copy.
This diagnostic is not shipping or frame-time evidence. Receipt SHA-256:
`0c5e0498cc6eba2a526da83b113e26294dcb9b76c99c09a4ce20744833e182fa`.
The caption remained blank in the active-IOSurface image
`58a8c0093d5b3ea38472b03bb06cd0c9d8d7adde1efd91c99aec4438281c8fd9`.

Capture validation recovered 1,161 complete renderer submits, 101,837
commands and 6,967 draw records without truncation. Draw context order
matched decoded DRAW_VBO order exactly. Context, payload size and prefix
matched every inner submit to the outer trace, with six outer submits not
reaching this renderer. Resource IDs were resolved by lifetime at the matched
submit, not reused across resets.

The actual 700x500 resource in this run is ID 251, created at outer sequence
8857. Its pixels arrive through COPY_TRANSFER3D from guest staging buffers;
no host draw targets that resource during this lifetime. Context 7 samples
it with fragment shader 196 and composites it to the scanout. The first copy
covers the whole 700x500 surface; later copies cover client rectangles starting
at y=31 or y=78. This is not the historical five-element glyph shader writing
directly into a window-sized target.

The next diagnostic, `t7-caption-diagnostic-20260908-r2`, captures bounded
guest iovec source bytes immediately before those uploads. It separates a
blank incoming caption from loss during upload/composition. No source pixels,
blend state, synchronization calls or atlas contents are modified.

R2 completed with receipt SHA-256
`8032985b61d285f24821345076064bc04035aa8f3db21d1135d344f4cc153f8d`.
All 18 captured upload records returned the exact requested byte count.
The initial 700x500 upload's title rectangle is uniformly BGRA
`(255,255,255,0)`: no title text, icon or window control buttons. The client
menus are present. This is an app surface with a transparent non-client area,
not proof that the guest omitted its caption rendering. The compositor must
draw that chrome separately.

The ordered R1 records also connect a separate 960x768 target from context 5
to context 7's compositor samples: fragment shader 557 produces it, and
shader 243 samples resource 167 adjacent to the 700x500 client draw. This
supplies the missing producer/consumer relationship, but does not yet locate
the caption pixels inside that target.

Diagnostic R3 (`t7-caption-diagnostic-20260908-r3`) captures at most eight
complete 960x768 targets after five-element draws, armed only after the first
700x500 guest upload. The full-target GL readback preserves pixel-pack and
read-framebuffer bindings but necessarily perturbs synchronization; it is
diagnostic-only and cannot establish a performance result. Its separately
sealed renderer is `4215708393aad68b5091d85fcfeb668bbace7e1caef1c3a6953782eb85194208`.

R3 finished with receipt SHA-256
`3158f633015d92bfb30313962d7fce37013f2ee09d8b87271e6f3b91a97270a8`.
The caption remained absent. One armed 960x768 readback was captured after a
five-element draw and contained only RGBA zeros. Timestamp/context matching
identifies context 5, submit 1012, fragment shader 441, sampler resource IDs
171 and 174. Its draw has six indices and sixteen instances. These identities
are local to this captured run, not fixed keys for a renderer workaround.
Two decoded draws did not reach the metadata hook, so global ordinal zipping
is invalid for R3; association used the enclosing submit timestamp and context.
The readback payload length is exact, but this first hook did not capture GL
error status or framebuffer completeness. An all-zero payload alone therefore
does not prove successful framebuffer readback; this validity check is required
before treating the target as proven transparent.

R4 (`t7-caption-diagnostic-20260908-r4`) adds bounded guest-iovec reads for
the two samplers at that candidate draw. It does not call glGetBufferSubData;
missing backing and short reads are recorded separately from valid zero data.

R4 finished with receipt
`f6d9fe7c269d18a73738b8eb35176ba3434b6681fceaf4bddfbab2edcbde2c20`.
Sampler 0's guest backing read exactly 524288 bytes, all zero. Sampler 1 had
only 4096 readable bytes for an 81920-byte request, so that sample is explicitly
unavailable, not a zero atlas. The ordered commands show why the TBO guest
backing is insufficient: RESOURCE_COPY_REGION populates its host buffer from
another resource; those writes do not update this guest backing.

The current Apple buffer-copy fallback already reads source GL bytes into CPU
staging, then writes the destination with glBufferSubData. R5 records those
existing staging bytes without adding another GL buffer read. It also records
framebuffer completeness and GL error status around target readback. This
distinguishes actual copied zeros from stale guest backing and invalid zero
readback; neither R4 observation alone justifies a renderer change.

R5 validity measurements retract the apparent zero-target result: each armed
readback found a complete framebuffer (`0x8cd5`) and no prior GL error, but
glReadPixels returned `GL_INVALID_OPERATION` (`1282`). The zero bytes were
the initialized diagnostic buffer, not proven framebuffer content. R3/R4 zero
readbacks must not be used as evidence of shader output.

The existing buffer-copy staging records contain nonzero TBO data. At one
candidate draw, five observed copies populated 2589 bytes with 1502 nonzero
bytes; the region at offset1680 is not all zero. This contradicts interpreting
the zero guest backing as the GL TBO's contents. A valid texture/FBO readback
is still required before attributing the caption failure to that producer.

R5 receipt SHA-256 is
`71aa8df764cae90052761d87b6198dc4fd10f982a895a5d43f0034668ea32185`.
The caption still reproduced. R6 replaces only the invalid framebuffer read
with glGetTexImage of the tracked target texture, preserving the prior texture
binding and capturing GL error status. This is the texture readback method
already used elsewhere in the renderer; its result must still be validated
before interpreting pixel values.

R6 did not reach the guest graphics path: renderer initialization failed before
any capture hook fired. Receipt
`8ea4919df6bfc87094feffcbdf59b95db9fcd80578994607e1b339eb62ed9783`
is retained as failed, with no target readback conclusion. Inspection found the
isolated build's default render-server path was the nonexistent
`/opt/homebrew/libexec/virgl_render_server`, unlike the installed build's
existing helper under the BridgeVM prefix. This is a diagnostic-build
prerequisite error; how earlier diagnostic processes obtained their server
remains unestablished and is not explained away.

R7 restores the baseline prefix in the isolated build configuration and
records the existing server SHA-256
`60492dbf2e1080d21bc82eb264adc64c626c400fd27dd467ce2806e7209c475b`.
Its renderer SHA-256 is
`8f08ceae0d3b55eed624c282ab9d085125787fd50ac1c60264c8dd8b6a009bf2`.
No installed renderer or helper was replaced.

R7 finished with receipt
`740e045aa5bba378eaad40c79a6c0bdc50cc53eabc5e52d2602e23f1d9a84e4b`.
Renderer initialization succeeded. Eight complete 960x768 texture reads had
zero prior and post-read GL errors and complete framebuffer status. They
contained nonzero pixels (58,754 bytes in the first capture), not zero targets.
RGB and alpha visualizations show box/shadow shapes, not readable caption
glyphs; the visible Notepad caption remains blank. This corrects the invalid
zero-target narrative without yet proving the precise caption texture region.

R8 captures bounded per-element guest vertex backing and asks GL for its
attached compiled shader source via glGetShaderSource rather than dereferencing
the renderer's shader-string objects. Exact vertex rectangles, offsets and
linked GLSL are needed to evaluate the candidate's texture fetch chain.

R8 finished with receipt
`5c4781dbd26062b0e5f8da61cd9bdeeb37843702cbc0556e5bd9ca32fd7ed6c4`.
The exact sixteen instance rectangles and observed copied TBO bytes reconstruct
the text **Untitled - Notepad**, identifying this draw as the caption rather
than merely a similarly-sized login surface. All sixteen source index ranges
were observed valid and contained nonzero coverage indices.

The linked fragment GLSL declares `samplerBuffer fssamp0` while its bound view
format is R8_UINT. It samples the buffer and then applies floatBitsToUint to
form the atlas coordinate. A standalone host CGL reproduction with byte181
shows that float sampler returning approximately0.709804; reinterpreting those
float bits produces an out-of-range atlas index. A usamplerBuffer returns the
integer181 and preserves the intended address. Both host draws report no GL
error, so absence of GL errors did not validate this type mismatch.

R9 tests a generic candidate: shader keys record signed/unsigned integer buffer
view types, and buffer-sampler translation selects the matching integer sampler
and bit-preserving result conversion. It does not match Notepad, context IDs,
texture dimensions or a five-element layout. This remains a candidate until
the caption renders live and regressions are checked.

R9 restored the visible title **\*Untitled - Notepad** in the active-IOSurface
capture `e7afdf4485a5e8132372f62cff932c03063e357d4afb607d31c92fbbff2a8363`.
Menus, document text and window controls also remained visible. Receipt
`02b3f4ebdb3424db1d53827b5e2f32010a6ba7f6bec25c9db4268e0047d60171`
still reports F3 partial, not a full criterion pass. Attached GLSL confirms
usamplerBuffer was selected for the integer buffer view.

A fresh non-instrumented renderer is now built separately with only the generic
sampler correction. It additionally preserves explicit TGSI SVIEW return types,
using format inference only for legacy shaders without such declarations.
Clean renderer SHA-256:
`568e1544e05df78709e8ba88fca6d8d63c5e421a317f05cf2c23af61ca381db8`.
Job `t7-caption-clean-20260908-r1` verifies this clean candidate with the
unchanged scene. B6 matrix and frame-time acceptance remain open.

The clean candidate's active-IOSurface frame also visibly renders
**\*Untitled - Notepad**, with menus, uppercase/lowercase document text,
digits, punctuation and status text intact. The existing upgrade banner is
unchanged. This removes diagnostic hooks as a prerequisite for the observed
caption restoration. Real vrend_convert_shader execution passed unsigned,
signed and float buffer fallback cases plus explicit FLOAT/UINT/SINT SVIEW
precedence against conflicting inferred keys. Broader matrix and workload
validation still remain separate obligations.

Clean candidate receipt SHA-256:
`3f161aa0617144ed9d560ca2208202e617faa210811e0880c580e222f4145eb2`.
Its active-IOSurface capture is
`01f4783d7a14df4ab39f6ea3c91a5f76516b35902f68bfa684378c8eb8cab3e1`,
seed27 to28, title visible. F1/F2 pass, F3 partial and overall pass=false
are preserved. The product patch contains only the generic sampler correction;
private instrumentation was removed from active diagnostic source trees and
the installed renderer remains unchanged until a separately sealed cutover.
