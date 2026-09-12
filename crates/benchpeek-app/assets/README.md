# USB OTG intro

`board_intro.bpk` embeds an 8-second, 60 fps animation of the STM32MP257F-DK (MB1605C). It begins with the full board and the original overhead-to-port camera move. After reaching the CN15 close-up at 4.17 seconds, the board fades into transparent context while the port and cable remain opaque. A cyan contour highlights the port, with “Please plug in your device.” beside it inside the animation.

All board geometry is retained in the Blender master. Separate render layers implement the board fade. The port geometry comes from the ST Altium data; the cable and highlight are custom Blender geometry.

The BPK2 binary layout is `BPK2`, little-endian u32 frame count, then one pair of little-endian u32 values per frame (absolute byte offset, byte length), followed by JPEG payloads. Each frame is 960 × 540. Still frames share payloads; moving frames are sampled directly from the 60 fps Blender timeline.

The onboarding module decodes frames on demand. Selected image/egui dependencies use optimized dev builds to keep decoding within the 60 fps frame budget when running `cargo run`.
