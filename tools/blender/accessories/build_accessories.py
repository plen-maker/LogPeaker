"""Create reusable, illustrative connector assets. Blender 5.x, no external dependencies."""
from pathlib import Path
import math
import bpy
from mathutils import Vector

OUT = Path(__file__).resolve().parent
bpy.ops.object.select_all(action='SELECT')
bpy.ops.object.delete(use_global=False)
for collection in list(bpy.data.collections):
    if not collection.objects:
        bpy.data.collections.remove(collection)
scene = bpy.context.scene
scene.unit_settings.system = 'METRIC'
scene.unit_settings.length_unit = 'MILLIMETERS'


def material(name, color, metallic=0, roughness=.35):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    mat.diffuse_color = (*color, 1)
    shader = mat.node_tree.nodes.get('Principled BSDF')
    shader.inputs['Base Color'].default_value = (*color, 1)
    shader.inputs['Metallic'].default_value = metallic
    shader.inputs['Roughness'].default_value = roughness
    return mat


silver = material('Brushed nickel', (.55, .61, .67), .85, .27)
gold = material('Gold contacts', (.83, .52, .12), .8, .24)
black = material('Graphite polymer', (.025, .033, .042), 0, .36)
rubber = material('Cable rubber', (.012, .019, .025), 0, .5)
white = material('Warm white polymer', (.8, .84, .86), 0, .3)
blue = material('USB blue insert', (.02, .16, .48), 0, .35)
cyan = material('Benchpeek teal', (.015, .65, .66), .15, .3)
ink = material('Printed white', (.86, .91, .96), 0, .5)
smoke = material('RJ45 translucent-look polymer', (.30, .40, .45), .1, .25)
assets = []
current = None
parent = None


def start(name, description):
    global current, parent
    current = bpy.data.collections.new(name)
    scene.collection.children.link(current)
    parent = bpy.data.objects.new(name + ' · insertion origin', None)
    current.objects.link(parent)
    parent.empty_display_size = .003
    parent['insertion_axis'] = '-Y; mating face at local Y=0'
    parent['units'] = 'meters'
    parent['purpose'] = 'Illustrative animation asset; not manufacturing CAD'
    current.asset_mark()
    current.asset_data.description = description
    for tag in ['Benchpeek', 'connector', 'animation']:
        current.asset_data.tags.new(tag)
    assets.append((current, parent))


def adopt(obj, name, mat=None):
    obj.name = name
    for col in list(obj.users_collection):
        col.objects.unlink(obj)
    current.objects.link(obj)
    obj.parent = parent
    if mat:
        obj.data.materials.append(mat)
    return obj


def box(name, xyz, size, mat, bevel=.2):
    bpy.ops.mesh.primitive_cube_add(size=1, location=tuple(v / 1000 for v in xyz))
    obj = adopt(bpy.context.object, name, mat)
    obj.dimensions = tuple(v / 1000 for v in size)
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    if bevel:
        modifier = obj.modifiers.new('Moulded edges', 'BEVEL')
        modifier.width = bevel / 1000
        modifier.segments = 3
        obj.modifiers.new('Weighted normals', 'WEIGHTED_NORMAL')
    return obj


def prism(name, outline, z0, z1, mat, bevel=.15):
    n = len(outline)
    verts = [(x / 1000, y / 1000, z / 1000) for z in [z0, z1] for x, y in outline]
    faces = [tuple(reversed(range(n))), tuple(range(n, 2*n))]
    faces += [(i, (i+1) % n, (i+1) % n+n, i+n) for i in range(n)]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    current.objects.link(obj)
    obj.parent = parent
    mesh.materials.append(mat)
    if bevel:
        mod = obj.modifiers.new('Edge radius', 'BEVEL')
        mod.width = bevel / 1000
        mod.segments = 3
        obj.modifiers.new('Weighted normals', 'WEIGHTED_NORMAL')
    return obj


def label(text, xyz, size=2, mat=ink):
    data = bpy.data.curves.new(text, 'FONT')
    data.body = text
    data.size = size / 1000
    data.align_x = 'CENTER'
    data.extrude = .005 / 1000
    obj = bpy.data.objects.new(text, data)
    current.objects.link(obj)
    obj.parent = parent
    obj.location = tuple(v / 1000 for v in xyz)
    data.materials.append(mat)
    return obj


def cable(y, radius=1.7, length=22):
    bpy.ops.mesh.primitive_cylinder_add(vertices=32, radius=radius/1000,
        depth=length/1000, location=(0, (y+length/2)/1000, 0),
        rotation=(math.pi/2, 0, 0))
    obj = adopt(bpy.context.object, 'Flexible cable stub', rubber)
    for face in obj.data.polygons:
        face.use_smooth = True
    for step in range(4):
        box('Strain relief rib', (0, y+step*1.1, 0),
            (radius*2+1, .65, radius*2+1), black, .45)


start('USB_A_plug', 'USB-A male plug with hollow metal shell, blue tongue, contacts and cable stub.')
# Hollow shell; mouth at Y=0, cable extending along positive Y.
box('Shell top', (0, 6, 2.15), (12, 12, .3), silver, .12)
box('Shell bottom', (0, 6, -2.15), (12, 12, .3), silver, .12)
for x in [-5.85, 5.85]:
    box('Shell side', (x, 6, 0), (.3, 12, 4.3), silver, .12)
box('Blue contact tongue', (0, 6.5, -.8), (10.6, 11, 1.5), blue, .15)
for x in [-3.4, -1.15, 1.15, 3.4]:
    box('Gold contact', (x, 5, .01), (1.05, 7, .08), gold, .04)
for x in [-3, 3]:
    box('Retention recess', (x, 5.5, 2.31), (2.1, 2.6, .025), black, .2)
box('Overmould', (0, 20, 0), (15, 16, 7), black, 1.2)
label('USB', (0, 19, 3.53), 2.5)
cable(28, 2)

start('USB_C_plug', 'USB-C male plug, capsule-shaped hollow shell, internal tongue and white overmould.')
# Capsule ring extruded along Y, genuinely open at the mating end.
ring = []
for sign, angles in [(1, range(-90, 91, 10)), (-1, range(90, 271, 10))]:
    for a in angles:
        r = math.radians(a)
        ring.append((sign*2.8+1.25*math.cos(r), 1.25*math.sin(r)))
n = len(ring)
verts = []
for y, inset in [(0, 0), (7, 0), (0, .22), (7, .22)]:
    for x, z in ring:
        verts.append((x*(1-inset/4.05)/1000, y/1000, z*(1-inset/1.25)/1000))
faces = []
for i in range(n):
    j = (i+1) % n
    faces += [(i,j,n+j,n+i), (2*n+j,2*n+i,3*n+i,3*n+j),
              (i,2*n+i,2*n+j,j), (n+j,3*n+j,3*n+i,n+i)]
mesh = bpy.data.meshes.new('USB-C hollow shell')
mesh.from_pydata(verts, [], faces)
mesh.update()
obj = bpy.data.objects.new('USB-C hollow shell', mesh)
current.objects.link(obj)
obj.parent = parent
mesh.materials.append(silver)
box('Inner tongue', (0, 4, 0), (6, 5.5, .55), black, .15)
for x in range(12):
    for z in [-.31, .31]:
        box('USB-C contact', ((x-5.5)*.45, 3.5, z), (.2, 3.7, .06), gold, .025)
box('White overmould', (0, 13, 0), (11, 12, 5.5), white, 1.2)
label('C', (0, 12, 2.8), 2.5, black)
cable(19, 1.5)

start('SD_card', 'Full-size SD memory card with clipped corner, write-protect slider and underside contacts.')
prism('SD body', [(-12,0),(8,0),(12,4),(12,32),(-12,32)], -1.05, 1.05, black)
box('Label panel', (0, 18, 1.09), (20, 22, .09), cyan, .65)
label('benchpeek', (0, 24, 1.15), 2.3)
label('SD', (0, 16, 1.15), 5)
label('STORAGE', (0, 11, 1.15), 1.6)
box('Write-protect slider', (-12, 12, 0), (1.1, 4, 1.1), white, .2)
for i in range(9):
    box('SD underside contact', (-9.4+i*2.3, 3.5, -1.09), (1.5, 6, .08), gold, .12)

start('microSD_card', 'microSD memory card with stepped silhouette and eight underside contacts.')
prism('microSD body', [(-5.5,0),(5.5,0),(5.5,10),(4.5,10),(4.5,15),(-5.5,15),
    (-5.5,9),(-4.8,9),(-4.8,6),(-5.5,6)], -.5, .5, black, .08)
box('Micro label', (0, 9, .54), (8, 9, .06), cyan, .3)
label('micro', (0, 10.5, .59), 1.5)
label('SD', (0, 7, .59), 2.8)
for i in range(8):
    box('microSD underside contact', (-4.2+i*1.2, 2.5, -.54), (.8, 4, .06), gold, .06)

start('RJ45_plug', 'Illustrative Ethernet plug with eight contacts, raised retention latch and cable boot.')
box('RJ45 clear-look housing', (0, 10, 0), (11.6, 20, 7.8), smoke, .5)
for i in range(8):
    box('Ethernet gold contact', ((i-3.5)*1.05, 3, 3.94), (.55, 5.5, .12), gold, .05)
box('Retention latch', (0, 12, -5), (5, 11, 1), white, .25)
box('Latch bridge', (0, 6.5, -4.4), (5, 1.4, 2), white, .2)
box('Protective boot', (0, 25, 0), (13, 10, 9), cyan, 1.5)
cable(30, 2.8)

start('SWD_10pin_plug', 'Illustrative keyed 2x5 SWD cable connector with ten socket openings and ribbon stub.')
box('Keyed SWD housing', (0, 5, 0), (9, 10, 5), black, .35)
box('Polarizing key', (4.8, 5, 0), (1.1, 3, 3), black, .15)
for x in range(5):
    for z in [-.635, .635]:
        box('Socket contact rim', ((x-2)*1.27, -.04, z), (.9, .12, .9), gold, .08)
        box('Socket opening', ((x-2)*1.27, -.12, z), (.65, .08, .65), rubber, .08)
for i in range(10):
    box('Ribbon conductor', ((i-4.5)*.75, 20, 0), (.7, 20, 1), cyan if i == 0 else white, .3)
label('SWD', (0, 5, 2.55), 1.8)

# Convert typography to geometry so exports remain self-contained.
for collection, root in assets:
    for obj in list(collection.objects):
        if obj.type == 'FONT':
            bpy.ops.object.select_all(action='DESELECT')
            obj.select_set(True)
            bpy.context.view_layer.objects.active = obj
            bpy.ops.object.convert(target='MESH')

# Export each asset in its local insertion coordinate system before staging.
for collection, root in assets:
    bpy.ops.object.select_all(action='DESELECT')
    for obj in collection.objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = root
    bpy.ops.export_scene.gltf(filepath=str(OUT / (collection.name + '.glb')),
        use_selection=True, export_format='GLB', export_extras=True, export_apply=True)

# Render a clean asset-library overview; all models use the same physical scale.
studio = bpy.data.collections.new('Studio · preview only')
scene.collection.children.link(studio)
current = studio
parent = None
positions = [(-.06,.024), (0,.024), (.06,.024), (-.06,-.046), (0,-.046), (.06,-.046)]
for (collection, root), (x,y) in zip(assets, positions):
    root.location = (x, y, .009)
    root.rotation_euler = (math.radians(12), 0, math.radians(-18))
    label(collection.name.replace('_', ' ').upper(), (x*1000, y*1000-12, 1), 2.8)
    label('ANIMATION ASSET', (x*1000, y*1000-17, 1), 1.4, cyan)
box('Studio floor', (0, 0, -2), (250, 235, 3), material('Studio navy', (.013,.024,.036), 0, .7), 2)
label('BENCHPEEK  /  CONNECTOR LIBRARY', (0, 100, 1), 4.3)
label('USB   /   STORAGE   /   NETWORK   /   DEBUG', (0, 92, 1), 2, cyan)


def aim(obj, point):
    obj.rotation_euler = (Vector(point)-obj.location).to_track_quat('-Z','Y').to_euler()


bpy.ops.object.camera_add(location=(.04, -.18, .29))
camera = bpy.context.object
camera.name = 'Library overview camera'
aim(camera, (0,.021,0))
camera.data.type = 'ORTHO'
camera.data.ortho_scale = .235
scene.camera = camera
for name, xyz, energy, size in [
    ('Large softbox', (-.08,-.04,.22), .8, .18),
    ('Cool rim', (.13,.09,.16), .5, .12),
    ('Fill', (-.1,.12,.09), .25, .1),
]:
    bpy.ops.object.light_add(type='AREA', location=xyz)
    lamp = bpy.context.object
    lamp.name = name
    lamp.data.energy = energy
    lamp.data.shape = 'DISK'
    lamp.data.size = size
    aim(lamp, (0,0,0))
scene.world.color = (.12,.12,.12)
scene.render.engine = 'CYCLES'
scene.cycles.samples = 48
scene.cycles.use_denoising = True
scene.render.resolution_x = 1600
scene.render.resolution_y = 1400
scene.render.resolution_percentage = 100
scene.render.image_settings.file_format = 'PNG'
scene.render.filepath = str(OUT / 'accessories_preview.png')
scene.render.fps = 60
bpy.ops.object.select_all(action='DESELECT')
bpy.ops.wm.save_as_mainfile(filepath=str(OUT / 'Benchpeek_accessories.blend'))
bpy.ops.render.render(write_still=True)
print('DONE: six reusable assets, six GLBs, Blender library and preview')
