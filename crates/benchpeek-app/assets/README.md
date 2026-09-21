# USB-A host intro

`board_intro.bpk` embeds an 8-second, 60 fps animation of the STM32MP257F-DK (MB1605C).
It begins with the full board overhead, flies to the CN19 close-up (the dual USB-A host
port - where the device to inspect is plugged in), fades the board to transparent context
while a cyan contour highlights the lower port, and a USB-A cable plugs in. Caption:
"USB-A / HOST PORT - Please plug in your device." (CN15 is the USB OTG port and CN21 the
ST-LINK / power port; neither can read a device.)

It is rendered from the real board model (ST STEP geometry, kept in `tools/blender/*.blend`)
by `tools/blender/retarget_usba.py`, which re-aims the "board fade" scene and builds the USB-A
plug; see the README's "Connection-guide animation" for the commands. Frames are 960 x 540
JPEG, 480 frames, the first 60 and last 65 share one payload each.

The BPK2 binary layout is `BPK2`, little-endian u32 frame count, then one pair of
little-endian u32 values per frame (absolute byte offset, byte length), followed by
JPEG payloads.

The onboarding module decodes frames on demand. Selected image/egui dependencies use
optimized dev builds to keep decoding within the 60 fps frame budget when running
`cargo run`.
