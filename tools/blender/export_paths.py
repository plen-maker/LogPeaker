"""Sample the camera and focus paths of a benchpeek scene into JSON (world space).

  blender -b Benchpeek_STM32MP257F-DK.blend -P export_paths.py -- /tmp/base_paths.json
"""
import json, sys
import bpy

out = sys.argv[sys.argv.index("--") + 1]
s = bpy.context.scene
cam = s.camera
focus = next(o for o in bpy.data.objects if "focus" in o.name.lower())
frames = []
for f in range(s.frame_start, s.frame_end + 1):
    s.frame_set(f)
    frames.append({
        "f": f,
        "cam": list(cam.matrix_world.translation),
        "rot": list(cam.matrix_world.to_euler()),
        "focus": list(focus.matrix_world.translation),
        "lens": cam.data.lens,
    })
json.dump({"fps": s.render.fps, "start": s.frame_start, "end": s.frame_end, "frames": frames}, open(out, "w"))
print("EXPORTED", len(frames), "frames ->", out)
