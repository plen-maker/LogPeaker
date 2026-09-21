"""Re-aim the "board fade" scene from CN15 (USB OTG) to the lower port of CN19 (dual USB-A host) and render it.

Why: the user plugs the device to be read into a USB *host* port; CN15 (OTG) and CN21
(ST-LINK / power) cannot read a device. The scene's plug is USB-C, so a USB-A plug (shell,
tongue, contacts, overmould) is built here and attached to the existing insertion empty and cable.

The flight (camera + focus + rotation) is taken from Benchpeek_STM32MP257F-DK.blend (CN21 flight),
time-mapped onto this scene's 60 fps timeline and shifted sideways/up by DX/DZ in proportion to the
flight progress, so it ends framing CN19 instead of CN21. The plug keeps its timing. The highlight ring
becomes a rectangle around the USB-A opening, CN19 joins the "stays opaque" collection, and the caption
image is replaced.

  blender-5.2 -b Benchpeek_USB_OTG_board_fade_60fps.blend -P retarget_usba.py -- \
      --paths /tmp/base_paths.json --out frames --res 960x540 --samples 32 [--frames 1,300 | --all]
"""
import json, math, os, sys
import bpy
from mathutils import Vector

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
def arg(name, default=None):
    return argv[argv.index(name) + 1] if name in argv else default

X_CN21 = -0.0264
X_A, Z_A = 0.01655, 0.0041      # centre of the lower USB-A opening (CN19), measured from a straight-on render
DX = X_A - X_CN21               # shift of everything that was authored for CN21
Z_PLUG = 0.0016                 # z of the old plug/cable centre line
DZ = Z_A - Z_PLUG
PULL = float(arg("--pull", "0.75"))   # camera back-off at the close-up (fraction of the original distance)
FONT_DIR = os.environ.get("BP_FONT_DIR", os.path.expanduser("~/LogPeaker/crates/benchpeek-app/assets/fonts"))
LABEL = arg("--label", "USB-A / HOST PORT")
CAP_UP = float(arg("--caption-up", "140"))   # px (1080p) to lift the caption clear of the plug
s = bpy.context.scene
paths = json.load(open(arg("--paths")))
base = {fr["f"]: fr for fr in paths["frames"]}

def base_at(t60):
    """Fade-scene frame (1..480 @ 60 fps) -> interpolated base sample."""
    if t60 <= 60:
        tb = 1.0
    elif t60 >= 300:
        tb = 120.0
    else:
        tb = 1.0 + (t60 - 60) * (119.0 / 240.0)
    lo = int(math.floor(tb)); hi = min(lo + 1, paths["end"]); w = tb - lo
    a, b = base[lo], base[hi]
    lerp = lambda p, q: [x + (y - x) * w for x, y in zip(p, q)]
    return lerp(a["cam"], b["cam"]), lerp(a["focus"], b["focus"]), a["lens"] + (b["lens"] - a["lens"]) * w, lerp(a["rot"], b["rot"])

# ---- 1. sample the plug path before touching its animation
ins = next(o for o in bpy.data.objects if o.name.startswith("USB-C insertion"))
plug_y = {}
for f in range(s.frame_start, s.frame_end + 1):
    s.frame_set(f)
    plug_y[f] = ins.matrix_world.translation.y

SKIP = set(a[5:] for a in argv if a.startswith("--no-"))

# ---- 2. camera + focus: clear their animation, re-key from the CN21 flight
cam = s.camera
focus = next(o for o in bpy.data.objects if "focus" in o.name.lower())
cam_rot = cam.rotation_euler.copy()
for o in (cam, focus):
    if "cam" not in SKIP:
        o.animation_data_clear()
cam.rotation_euler = cam_rot
def flight_w(fo):
    """0 at the overhead start, 1 at the CN21 close-up, from the focus x progress."""
    return max(0.0, min(1.0, fo[0] / X_CN21))
for f in ([] if "cam" in SKIP else range(s.frame_start, s.frame_end + 1)):
    c, fo, lens, rot = base_at(f)
    w = flight_w(fo)
    c = [c[0] + DX * w, c[1], c[2] + DZ * w]
    fo = [fo[0] + DX * w, fo[1], fo[2] + DZ * w]
    # The USB-A plug is bigger than the USB-C one: pull the camera back so port and plug fit.
    k = 1.0 + PULL * w
    c = [fo[i] + (c[i] - fo[i]) * k for i in range(3)]
    cam.location = c
    cam.rotation_euler = rot
    focus.location = fo
    cam.data.lens = lens
    cam.keyframe_insert("location", frame=f)
    cam.keyframe_insert("rotation_euler", frame=f)
    focus.keyframe_insert("location", frame=f)
    cam.data.keyframe_insert("lens", frame=f)

# ---- 3. plug: same timing in y, shifted to CN19's lower port
if "plug" not in SKIP:
    ins.animation_data_clear()
for f in ([] if "plug" in SKIP else range(s.frame_start, s.frame_end + 1)):
    ins.location = (DX, plug_y[f], DZ)
    ins.keyframe_insert("location", frame=f)

# hide the USB-C plug parts (kept in the file, just not rendered)
for o in list(bpy.data.objects):
    if o.name.startswith(("USB-C plug", "White USB-C overmould", "Plug contact")):
        o.hide_render = True
        o.hide_viewport = True

def box(name, size, loc, material, bevel=0.0):
    """Cube of `size` at local `loc`, parented to the insertion empty."""
    me = bpy.data.meshes.new(name)
    import bmesh
    bm = bmesh.new(); bmesh.ops.create_cube(bm, size=1.0); bm.to_mesh(me); bm.free()
    o = bpy.data.objects.new(name, me)
    o.scale = size
    o.location = loc
    o.parent = ins
    o.data.materials.append(material)
    if bevel:
        m = o.modifiers.new("Bevel", "BEVEL"); m.width = bevel; m.segments = 4; m.limit_method = "ANGLE"
    for c in ins.users_collection:
        c.objects.link(o)
    return o

def make_usba_plug():
    nickel = bpy.data.materials.get("Satin nickel")
    white = bpy.data.materials.get("White cable polymer")
    gold = bpy.data.materials.get("Gold plated contacts")
    dark = bpy.data.materials.new("USB-A tongue plastic"); dark.diffuse_color = (0.02, 0.02, 0.02, 1)
    dark.use_nodes = True
    dark.node_tree.nodes["Principled BSDF"].inputs["Base Color"].default_value = (0.02, 0.02, 0.02, 1)
    dark.node_tree.nodes["Principled BSDF"].inputs["Roughness"].default_value = 0.5
    cx, cz = X_CN21, Z_PLUG           # local centre line of the old plug
    w, h, t = 0.0120, 0.0045, 0.0003  # USB-A shell: 12.0 x 4.5 mm, 0.3 mm wall
    y0, y1 = -0.0497, -0.0377         # shell back / tip (12 mm)
    ym, yl = (y0 + y1) / 2, (y1 - y0)
    box("USB-A shell top", (w, yl, t), (cx, ym, cz + h / 2 - t / 2), nickel)
    box("USB-A shell bottom", (w, yl, t), (cx, ym, cz - h / 2 + t / 2), nickel)
    box("USB-A shell left", (t, yl, h), (cx - w / 2 + t / 2, ym, cz), nickel)
    box("USB-A shell right", (t, yl, h), (cx + w / 2 - t / 2, ym, cz), nickel)
    box("USB-A tongue", (0.0104, 0.0095, 0.0016), (cx, y1 - 0.0055, cz - 0.0003), dark)
    for i, dx in enumerate((-0.0032, -0.0011, 0.0011, 0.0032)):
        box(f"USB-A contact {i}", (0.0009, 0.0080, 0.00025), (cx + dx, y1 - 0.0062, cz + 0.0006), gold)
    box("USB-A overmould", (0.0146, 0.0106, 0.0080), (cx, y0 - 0.0053, cz), white, bevel=0.0018)
if "plug" not in SKIP:
    make_usba_plug()

# ---- 4. highlight ring: a rounded rectangle around the USB-A opening
old_ring = bpy.data.objects.get("USB-C · highlighted opening")
if old_ring:
    old_ring.hide_render = True
    old_ring.hide_viewport = True
    ring_mat = old_ring.data.materials[0]
    ring_colls = list(old_ring.users_collection)
    W, H, R, Y = 0.0146, 0.0072, 0.0010, -0.0437
    pts = []
    for cx_, cz_, a0 in ((W / 2 - R, H / 2 - R, 0), (-W / 2 + R, H / 2 - R, 90), (-W / 2 + R, -H / 2 + R, 180), (W / 2 - R, -H / 2 + R, 270)):
        for k in range(9):
            a = math.radians(a0 + 90.0 * k / 8)
            pts.append((X_A + cx_ + R * math.cos(a), Y, Z_A + cz_ + R * math.sin(a)))
    cd = bpy.data.curves.new("USB-A highlighted opening", "CURVE")
    cd.dimensions = "3D"
    cd.bevel_depth = 0.00014
    sp = cd.splines.new("POLY"); sp.points.add(len(pts) - 1)
    for p, (x, y, z) in zip(sp.points, pts):
        p.co = (x, y, z, 1.0)
    sp.use_cyclic_u = True
    cd.materials.append(ring_mat)
    ro = bpy.data.objects.new("USB-A · highlighted opening", cd)
    for c in ring_colls:
        c.objects.link(ro)

# ---- 5. collections: CN19 (dual USB-A) stays opaque, CN15 goes back to the fading board
opaque = bpy.data.collections["CN15 and cable · remain opaque"]
board = bpy.data.collections["Board · fades at OTG close-up"]
def move(obj, src, dst):
    if obj.name in src.objects:
        src.objects.unlink(obj)
    if obj.name not in dst.objects:
        dst.objects.link(obj)
for o in ([] if "coll" in SKIP else list(opaque.objects)):
    if o.name.startswith("CN15"):
        move(o, opaque, board)
for o in ([] if "coll" in SKIP else list(board.objects)):
    if o.name.startswith("CN19"):
        move(o, board, opaque)

# ---- 6. caption: render a new transparent 1920x1080 PNG with a temporary scene
def make_caption(path):
    cap = bpy.data.scenes.new("caption")
    cap.render.engine = "BLENDER_EEVEE"
    cap.render.film_transparent = True
    cap.render.resolution_x, cap.render.resolution_y = 1920, 1080
    cap.render.image_settings.file_format = "PNG"
    cap.render.image_settings.color_mode = "RGBA"
    cap.view_settings.view_transform = "Standard"
    cd = bpy.data.cameras.new("capcam"); cd.type = "ORTHO"; cd.ortho_scale = 1920
    co = bpy.data.objects.new("capcam", cd); co.location = (960, 540, 10)
    cap.collection.objects.link(co); cap.camera = co
    def mat(rgb, strength=1.0):
        m = bpy.data.materials.new("capmat"); m.use_nodes = True
        nt = m.node_tree; nt.nodes.clear()
        em = nt.nodes.new("ShaderNodeEmission"); em.inputs["Color"].default_value = (*rgb, 1); em.inputs["Strength"].default_value = strength
        out = nt.nodes.new("ShaderNodeOutputMaterial"); nt.links.new(em.outputs[0], out.inputs[0])
        return m
    def text(body, size, x, y_top, font_file, rgb):
        cu = bpy.data.curves.new("t", "FONT"); cu.body = body; cu.size = size
        cu.font = bpy.data.fonts.load(os.path.join(FONT_DIR, font_file))
        cu.align_x, cu.align_y = "LEFT", "TOP"; cu.space_line = 1.05
        o = bpy.data.objects.new("t", cu); o.location = (x, 1080 - y_top, 0)
        o.data.materials.append(mat(rgb)); cap.collection.objects.link(o)
    cyan = (0.05, 0.64, 0.92)   # linear values close to the original #5ad2f5-ish cyan
    # short rule above the label
    bpy.ops.mesh.primitive_plane_add(size=1, location=(1120 + 31, 1080 - (398 - CAP_UP), 0))
    rule = bpy.context.active_object; rule.scale = (62, 4, 1); rule.data.materials.append(mat(cyan))
    for c in list(rule.users_collection): c.objects.unlink(rule)
    cap.collection.objects.link(rule)
    text(LABEL, 27, 1120, 424 - CAP_UP, "Inter-Medium.ttf", cyan)
    text("Please plug in\nyour device.", 80, 1120, 476 - CAP_UP, "Inter-Light.ttf", (0.86, 0.87, 0.92))
    cap.render.filepath = path
    bpy.ops.render.render(write_still=True, scene="caption")
    return path

if "caption" not in SKIP:
    cap_path = make_caption("/tmp/stlink_caption.png")
    img = bpy.data.images.load(cap_path)
    img.alpha_mode = "STRAIGHT"
    node = next(n for n in s.compositing_node_group.nodes if n.bl_idname.endswith("Image"))
    node.image = img

# ---- 7. render (or save)
res = [int(v) for v in arg("--res", "960x540").split("x")]
s.render.resolution_x, s.render.resolution_y = res
s.render.resolution_percentage = 100
try:
    s.eevee.taa_render_samples = int(arg("--samples", "32"))
except Exception:
    pass
s.render.image_settings.file_format = "JPEG"
s.render.image_settings.quality = int(arg("--quality", "85"))
s.render.use_compositing = True
if arg("--save"):
    bpy.ops.wm.save_as_mainfile(filepath=arg("--save"), copy=True)
    print("SAVED", arg("--save"))
out = arg("--out", "/tmp/stlink_frames")
os.makedirs(out, exist_ok=True)
if "--all" in argv:
    frames = list(range(s.frame_start, s.frame_end + 1))
elif arg("--frames"):
    frames = [int(v) for v in arg("--frames").split(",")]
else:
    frames = []
for f in frames:
    s.frame_set(f)
    s.render.filepath = os.path.join(out, f"f{f:04d}.jpg")
    bpy.ops.render.render(write_still=True, scene=s.name)
    print("RENDERED", f, flush=True)
