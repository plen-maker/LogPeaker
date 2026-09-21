"""Render frames of one of the real benchpeek .blend scenes (saved by Blender 5.2).

  blender-5.2 -b scenes/Benchpeek_USB_OTG_board_fade_60fps.blend \
      -P tools/blender/render_scene.py -- --frames 1,300 --out /tmp/f --res 960x540 --samples 32 [--engine cycles|scene]

The scene's own animation, compositor fades and materials are used as saved;
only resolution, sample count and (optionally) the render engine are overridden.
"""
import os, sys
import bpy

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
def arg(name, default=None):
    return argv[argv.index(name) + 1] if name in argv else default

s = bpy.context.scene
out = arg("--out", "/tmp/bp_scene")
res = [int(v) for v in arg("--res", "960x540").split("x")]
samples = int(arg("--samples", "32"))
engine = arg("--engine", "scene")
os.makedirs(out, exist_ok=True)

s.render.resolution_x, s.render.resolution_y = res
s.render.resolution_percentage = 100
if engine == "cycles":
    s.render.engine = "CYCLES"
    s.cycles.device = "CPU"
if s.render.engine == "CYCLES":
    s.cycles.samples = samples
    s.cycles.use_denoising = True
else:
    try:
        s.eevee.taa_render_samples = samples
    except Exception:
        pass
s.render.image_settings.file_format = "JPEG"
s.render.image_settings.quality = 92
s.render.use_compositing = True
print("SCENE engine:", s.render.engine, "res:", res, "samples:", samples, "frames:", s.frame_start, s.frame_end, flush=True)

if "--all" in argv:
    frames = list(range(s.frame_start, s.frame_end + 1))
else:
    frames = [int(v) for v in arg("--frames", "1").split(",")]
for f in frames:
    path = os.path.join(out, f"f{f:04d}.jpg")
    if os.path.exists(path) and "--force" not in argv:
        continue
    s.frame_set(f)
    s.render.filepath = path
    bpy.ops.render.render(write_still=True)
    print("RENDERED", f, flush=True)
