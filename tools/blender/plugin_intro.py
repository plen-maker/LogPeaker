"""Blender scene for the benchpeek connection-guide animation.

Monochrome, 8 s @ 60 fps (480 frames), 960x540: a stylised STM32MP257F-DK-like
board, camera flies from the full board to the CN15 USB-C port, the board fades
to context, and a USB-C cable plugs into CN15 (the data port - the other
connectors are inputs/power).  The cable's far end leaves the frame toward the
host computer.

Run headless:
  Blender -b -P plugin_intro.py -- --frames 0,100,216 --out /tmp/preview --res 480x270 --samples 6
  Blender -b -P plugin_intro.py -- --all --out /tmp/final

To use the real board model instead of the stand-in, import it in
`build_board()` (keep the names `CN15_PORT`, and the light/camera rig works
unchanged) - the CN15 receptacle position is PORT_POS below.
"""
import math, os, sys
import bpy
from mathutils import Vector

# ------------------------------------------------------------------ args ----
argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
def arg(name, default=None):
    return argv[argv.index(name) + 1] if name in argv else default
OUT = arg("--out", "/tmp/bp_frames")
RES = [int(v) for v in arg("--res", "960x540").split("x")]
SAMPLES = int(arg("--samples", "32"))
FONT_DIR = os.environ.get("BP_FONT_DIR", os.path.expanduser("~/LogPeaker/crates/benchpeek-app/assets/fonts"))

FPS = 60
HOLD_START_END = 59      # frames 0..59 are identical (wide hold)
HOLD_END_START = 405     # frames 405..479 are identical (connected hold)
PORT_POS = Vector((-5.75, -2.2, 0.40))   # centre of the CN15 receptacle (1 unit = 1 cm)

# --------------------------------------------------------------- helpers ----
bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.frame_start, scene.frame_end = 0, 479
scene.render.fps = FPS
coll = scene.collection

def mat(name, base=0.1, metal=0.0, rough=0.5, alpha=1.0, emit=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (base, base, base, 1)
    b.inputs["Metallic"].default_value = metal
    b.inputs["Roughness"].default_value = rough
    b.inputs["Alpha"].default_value = alpha
    if emit:
        b.inputs["Emission Color"].default_value = (1, 1, 1, 1)
        b.inputs["Emission Strength"].default_value = emit
    return m

def key_alpha(m, pairs):
    b = m.node_tree.nodes["Principled BSDF"]
    for f, v in pairs:
        b.inputs["Alpha"].default_value = v
        b.inputs["Alpha"].keyframe_insert("default_value", frame=f)

def box(name, size, loc, material, radius=0.0, parent=None):
    """Cube of `size`; `loc` is world space, or local to `parent` when given."""
    bpy.ops.mesh.primitive_cube_add(size=1, location=(0, 0, 0) if parent else loc)
    o = bpy.context.active_object
    o.name = name
    o.scale = size
    bpy.ops.object.transform_apply(scale=True)
    if radius > 0:
        m = o.modifiers.new("Bevel", "BEVEL")
        m.width = radius
        m.segments = 6
        m.limit_method = "NONE"
    o.data.materials.append(material)
    if parent:
        o.parent = parent          # identity parent-inverse: loc is local
        o.location = loc
    return o

def empty(name, loc=(0, 0, 0)):
    o = bpy.data.objects.new(name, None)
    o.location = loc
    coll.objects.link(o)
    return o

def ease_keys(obj, path="location"):
    if obj.animation_data and obj.animation_data.action:
        for fc in obj.animation_data.action.fcurves:
            for kp in fc.keyframe_points:
                kp.interpolation = "BEZIER"
                kp.handle_left_type = kp.handle_right_type = "AUTO_CLAMPED"

# ------------------------------------------------------------ materials ----
M_PCB = mat("pcb", 0.045, 0.0, 0.55)
M_CHIP = mat("chip", 0.32, 0.6, 0.35)
M_CHIP2 = mat("chip2", 0.10, 0.0, 0.4)
M_METAL = mat("metal", 0.65, 1.0, 0.28)
M_PORT_METAL = mat("port_metal", 0.75, 1.0, 0.22)   # CN15 stays fully opaque
M_BLACK = mat("black", 0.01, 0.0, 0.8)
M_MOLD = mat("mold", 0.03, 0.0, 0.45)
M_SILK = mat("silk", 0.7, 0.0, 0.7)
FADE_MATS = [M_PCB, M_CHIP, M_CHIP2, M_METAL, M_BLACK, M_SILK]   # board "context"

# ---------------------------------------------------------------- board ----
def build_board():
    box("PCB", (12, 9, 0.16), (0, 0, 0), M_PCB, radius=0.12)
    top = 0.08
    box("SoC", (2.6, 2.6, 0.28), (0.4, 0.3, top + 0.14), M_CHIP, radius=0.06)
    box("SoC_die", (1.5, 1.5, 0.02), (0.4, 0.3, top + 0.29), M_CHIP2)
    box("DDR_a", (1.3, 1.0, 0.16), (-2.8, 1.7, top + 0.08), M_CHIP2, radius=0.03)
    box("DDR_b", (1.3, 1.0, 0.16), (-2.8, 0.2, top + 0.08), M_CHIP2, radius=0.03)
    box("PMIC", (0.9, 0.9, 0.12), (2.7, 2.4, top + 0.06), M_CHIP2, radius=0.03)
    for i in range(9):   # small passives
        box(f"R{i}", (0.24, 0.12, 0.08), (2.3 + 0.36 * (i % 3), -0.9 - 0.3 * (i // 3), top + 0.04), M_BLACK)
    # Other connectors (inputs / power): dimmed together with the board.
    box("RJ45", (1.6, 1.6, 1.3), (5.0, 1.8, top + 0.65), M_METAL, radius=0.05)
    box("USB_A_1", (1.5, 1.3, 0.75), (5.1, -0.9, top + 0.38), M_METAL, radius=0.04)
    box("USB_A_2", (1.5, 1.3, 0.75), (5.1, -2.6, top + 0.38), M_METAL, radius=0.04)
    box("CN21_stlink", (0.9, 0.95, 0.40), (-5.75, 2.4, top + 0.24), M_METAL, radius=0.18)
    box("PWR_in", (0.9, 0.95, 0.40), (-5.75, 0.3, top + 0.24), M_METAL, radius=0.18)
    box("header", (4.2, 0.5, 0.9), (1.0, -3.6, top + 0.45), M_BLACK)
    # Silkscreen labels.
    font = bpy.data.fonts.load(os.path.join(FONT_DIR, "Inter-Medium.ttf"))
    for txt, loc in (("CN15", (-4.6, -3.55, top + 0.005)), ("CN21", (-4.6, 3.75, top + 0.005))):
        cu = bpy.data.curves.new("silk_" + txt, "FONT")
        cu.body, cu.size, cu.font = txt, 0.42, font
        o = bpy.data.objects.new("silk_" + txt, cu)
        o.location = loc
        o.data.materials.append(M_SILK)
        coll.objects.link(o)

def build_port():
    """CN15: USB-C receptacle with a real cavity + tongue; never fades."""
    cx, cy, cz = PORT_POS
    shell = box("CN15_PORT", (0.9, 0.95, 0.40), (cx, cy, cz), M_PORT_METAL, radius=0.18)
    cav = box("CN15_cavity", (1.0, 0.82, 0.26), (cx - 0.12, cy, cz), M_BLACK, radius=0.12)
    bm = shell.modifiers.new("Cavity", "BOOLEAN")
    bm.operation, bm.object, bm.solver = "DIFFERENCE", cav, "EXACT"
    cav.hide_render = True
    cav.hide_viewport = True
    box("CN15_tongue", (0.55, 0.62, 0.05), (cx + 0.05, cy, cz), M_MOLD)
    return shell

# --------------------------------------------------------------- plug -------
def build_plug():
    """USB-C plug, origin at the tip, pointing +X; cable curve trails to -X."""
    g = empty("PLUG", (-10.6, PORT_POS.y, PORT_POS.z))
    box("plug_shell", (0.9, 0.82, 0.26), (-0.45, 0, 0), M_PORT_METAL, radius=0.11, parent=g)
    box("plug_slot", (0.05, 0.5, 0.09), (0.0, 0, 0), M_BLACK, radius=0.03, parent=g)
    box("plug_mold", (1.5, 1.25, 0.56), (-1.65, 0, 0), M_MOLD, radius=0.22, parent=g)
    box("plug_relief", (0.7, 0.34, 0.34), (-2.75, 0, 0), M_MOLD, radius=0.15, parent=g)
    return g

def build_cable(plug):
    cd = bpy.data.curves.new("cable", "CURVE")
    cd.dimensions = "3D"
    cd.bevel_depth = 0.12
    cd.bevel_resolution = 6
    sp = cd.splines.new("BEZIER")
    sp.bezier_points.add(3)
    x0 = plug.location.x - 3.05
    pts = [(x0, PORT_POS.y, PORT_POS.z), (x0 - 3.0, PORT_POS.y + 0.4, PORT_POS.z + 0.15),
           (x0 - 6.0, PORT_POS.y + 2.6, 0.9), (-26.0, 6.5, 2.4)]
    for bp, p in zip(sp.bezier_points, pts):
        bp.co = p
        bp.handle_left_type = bp.handle_right_type = "AUTO"
    o = bpy.data.objects.new("cable", cd)
    o.data.materials.append(M_MOLD)
    coll.objects.link(o)
    return o, sp

# -------------------------------------------------------------- ring/text ---
def build_ring():
    cx, cy, cz = PORT_POS
    pts = []
    w, h, r = 0.62, 0.36, 0.30
    n = 20
    for corner, (sx, sy) in enumerate(((1, 1), (-1, 1), (-1, -1), (1, -1))):
        a0 = corner * math.pi / 2
        for i in range(n + 1):
            a = a0 + (math.pi / 2) * i / n
            pts.append((sx * (w - r) + r * math.cos(a) if False else 0, 0, 0))
    # stadium outline in the YZ plane (facing -X)
    pts = []
    for i in range(96):
        t = 2 * math.pi * i / 96
        # superellipse -> rounded rectangle look
        c, s = math.cos(t), math.sin(t)
        e = 0.45
        y = w * math.copysign(abs(c) ** e, c)
        z = h * math.copysign(abs(s) ** e, s)
        pts.append((y, z))
    cd = bpy.data.curves.new("ring", "CURVE")
    cd.dimensions = "3D"
    cd.bevel_depth = 0.028
    sp = cd.splines.new("POLY")
    sp.points.add(len(pts) - 1)
    for p, (y, z) in zip(sp.points, pts):
        p.co = (0.0, y, z, 1.0)
    sp.use_cyclic_u = True
    o = bpy.data.objects.new("ring", cd)
    o.location = (cx - 0.62, cy, cz)
    m = bpy.data.materials.new("ring_mat")
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Strength"].default_value = 4.0
    tr = nt.nodes.new("ShaderNodeBsdfTransparent")
    mix = nt.nodes.new("ShaderNodeMixShader")
    outn = nt.nodes.new("ShaderNodeOutputMaterial")
    nt.links.new(tr.outputs[0], mix.inputs[1])
    nt.links.new(em.outputs[0], mix.inputs[2])
    nt.links.new(mix.outputs[0], outn.inputs[0])
    mix.inputs[0].default_value = 0.0
    o.data.materials.append(m)
    coll.objects.link(o)
    return o, mix

def text_obj(name, body, size, loc, cam, weight="Inter-Medium.ttf"):
    font = bpy.data.fonts.load(os.path.join(FONT_DIR, weight))
    cu = bpy.data.curves.new(name, "FONT")
    cu.body, cu.size, cu.font = body, size, font
    cu.align_x, cu.align_y = "LEFT", "CENTER"
    o = bpy.data.objects.new(name, cu)
    o.parent = cam
    o.location = loc
    m = bpy.data.materials.new(name + "_m")
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Strength"].default_value = 3.0
    tr = nt.nodes.new("ShaderNodeBsdfTransparent")
    mix = nt.nodes.new("ShaderNodeMixShader")
    outn = nt.nodes.new("ShaderNodeOutputMaterial")
    nt.links.new(tr.outputs[0], mix.inputs[1])
    nt.links.new(em.outputs[0], mix.inputs[2])
    nt.links.new(mix.outputs[0], outn.inputs[0])
    mix.inputs[0].default_value = 0.0
    o.data.materials.append(m)
    coll.objects.link(o)
    return o, mix

def key_fac(mix, pairs):
    for f, v in pairs:
        mix.inputs[0].default_value = v
        mix.inputs[0].keyframe_insert("default_value", frame=f)

# ---------------------------------------------------------------- build -----
build_board()
port = build_port()
plug = build_plug()
cable, spline = build_cable(plug)
ring, ring_mix = build_ring()

# Ground + world + lights (neutral, monochrome).
box("floor", (200, 200, 0.1), (0, 0, -0.15), mat("floor", 0.003, 0.0, 0.9))
world = bpy.data.worlds.new("W")
world.use_nodes = True
world.node_tree.nodes["Background"].inputs[0].default_value = (0.003, 0.003, 0.004, 1)
world.node_tree.nodes["Background"].inputs[1].default_value = 1.0
scene.world = world
def area(name, loc, target, energy, size):
    l = bpy.data.lights.new(name, "AREA")
    l.energy, l.size = energy, size
    o = bpy.data.objects.new(name, l)
    o.location = loc
    coll.objects.link(o)
    c = o.constraints.new("TRACK_TO")
    c.target, c.track_axis, c.up_axis = target, "TRACK_NEGATIVE_Z", "UP_Y"
    return o
focus = empty("focus", (0, 0, 0))
area("key", (-9, -9, 14), focus, 2600, 5)
area("rim", (10, 8, 8), focus, 3200, 4)
area("fill", (-14, 6, 5), focus, 180, 8)

# Camera rig.
cd = bpy.data.cameras.new("cam")
cam = bpy.data.objects.new("cam", cd)
coll.objects.link(cam)
scene.camera = cam
tt = cam.constraints.new("TRACK_TO")
tt.target, tt.track_axis, tt.up_axis = focus, "TRACK_NEGATIVE_Z", "UP_Y"
cd.lens = 40
cam_keys = [
    (0,   (2.0, -15.5, 12.5), (0.0, 0.0, 0.0), 40),
    (60,  (2.0, -15.5, 12.5), (0.0, 0.0, 0.0), 40),
    (216, (-10.4, -9.2, 4.4), (-6.6, -2.3, 0.35), 55),
    (479, (-10.4, -9.2, 4.4), (-6.6, -2.3, 0.35), 55),
]
for f, cl, fl, lens in cam_keys:
    cam.location, focus.location, cd.lens = cl, fl, lens
    cam.keyframe_insert("location", frame=f)
    focus.keyframe_insert("location", frame=f)
    cd.keyframe_insert("lens", frame=f)
for o in (cam, focus):
    ease_keys(o)
ease_keys_lens = [fc for fc in cd.animation_data.action.fcurves]
for fc in ease_keys_lens:
    for kp in fc.keyframe_points:
        kp.interpolation = "BEZIER"

# Board context fades to ~16% while the camera arrives.
for m in FADE_MATS:
    key_alpha(m, [(150, 1.0), (216, 0.16)])

# Highlight ring: fades in, then one pulse when the plug seats.
key_fac(ring_mix, [(214, 0.0), (238, 1.0), (372, 1.0), (392, 0.35), (404, 0.0)])
ring.scale = (1, 1, 1)
for f, s in ((372, 1.0), (392, 1.12), (404, 1.0)):
    ring.scale = (1, s, s)
    ring.keyframe_insert("scale", frame=f)

# Plug: approaches along +X, seats fully (tip inside the receptacle).
plug.location.x = -10.6
plug.keyframe_insert("location", frame=270)
plug.location.x = -5.35
plug.keyframe_insert("location", frame=384)
ease_keys(plug)

# Cable follows the plug: point0 with the plug, point1 partially, rest fixed.
pts = spline.bezier_points
base = [p.co.copy() for p in pts]
def set_cable(frame, dx):
    pts[0].co = base[0] + Vector((dx, 0, 0))
    pts[1].co = base[1] + Vector((dx * 0.7, 0, 0))
    pts[2].co = base[2] + Vector((dx * 0.25, 0, 0))
    for i in (0, 1, 2):
        pts[i].keyframe_insert("co", frame=frame)
set_cable(270, 0.0)
set_cable(384, 5.25)

# Text overlay (camera-parented, bottom-right region of the frame).
lbl, lbl_mix = text_obj("label", "USB-C / OTG", 0.11, (0.62, 0.30, -6.0), cam, "Inter-SemiBold.ttf")
t1, t1_mix = text_obj("t1", "Please plug in\nyour device.", 0.24, (0.62, -0.08, -6.0), cam, "Inter-Medium.ttf")
t2, t2_mix = text_obj("t2", "Connected.", 0.24, (0.62, 0.06, -6.0), cam, "Inter-Medium.ttf")
key_fac(lbl_mix, [(236, 0.0), (256, 1.0)])
key_fac(t1_mix, [(240, 0.0), (262, 1.0), (372, 1.0), (384, 0.0)])
key_fac(t2_mix, [(386, 0.0), (402, 1.0)])

# --------------------------------------------------------------- render -----
scene.render.engine = "CYCLES"
scene.cycles.device = "CPU"
if "--gpu" in argv:
    prefs = bpy.context.preferences.addons["cycles"].preferences
    prefs.compute_device_type = "METAL"
    prefs.get_devices()
    for d in prefs.devices:
        d.use = True
        print("DEVICE", d.name, d.type)
    scene.cycles.device = "GPU"
scene.cycles.samples = SAMPLES
scene.cycles.use_denoising = True
scene.cycles.denoiser = "OPENIMAGEDENOISE"
scene.cycles.max_bounces = 6
scene.render.resolution_x, scene.render.resolution_y = RES
scene.render.resolution_percentage = 100
scene.render.image_settings.file_format = "JPEG"
scene.render.image_settings.quality = 92
scene.view_settings.view_transform = "Standard"
scene.render.film_transparent = False

if arg("--cam"):   # debug: static camera "x,y,z,fx,fy,fz,lens"
    v = [float(x) for x in arg("--cam").split(",")]
    for o in (cam, focus):
        o.animation_data_clear()
    cd.animation_data_clear()
    cam.location, focus.location, cd.lens = v[0:3], v[3:6], v[6]

os.makedirs(OUT, exist_ok=True)
if "--all" in argv:
    frames = list(range(0, 1)) + list(range(HOLD_START_END + 1, HOLD_END_START + 1))
else:
    frames = [int(v) for v in arg("--frames", "0").split(",")]
for f in frames:
    path = os.path.join(OUT, f"f{f:04d}.jpg")
    if os.path.exists(path) and "--force" not in argv:
        continue
    scene.frame_set(f)
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)
    print("RENDERED", f, flush=True)
