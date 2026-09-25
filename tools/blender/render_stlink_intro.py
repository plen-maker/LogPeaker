"""Rebuild the CN21 intro: blender -b --python tools/blender/render_stlink_intro.py -- --all."""
import math
from pathlib import Path
import sys

import bpy

ROOT = Path(__file__).resolve().parents[2]
bpy.ops.wm.open_mainfile(
    filepath=str(ROOT / 'tools/blender/Benchpeek_STM32MP257F-DK.blend')
)
scene = bpy.context.scene

# Preserve the original motion while retiming the 24 fps master to 60 fps.
for action in bpy.data.actions:
    for layer in action.layers:
        for strip in layer.strips:
            for bag in strip.channelbags:
                for curve in bag.fcurves:
                    for key in curve.keyframe_points:
                        key.co.x = 1 + (key.co.x - 1) * 2.5
                        key.handle_left.x = 1 + (key.handle_left.x - 1) * 2.5
                        key.handle_right.x = 1 + (key.handle_right.x - 1) * 2.5
for marker in scene.timeline_markers:
    marker.frame = round(1 + (marker.frame - 1) * 2.5)
scene.frame_end = 480
scene.render.fps = 60

cable = bpy.data.objects['USB-C insertion animation']
focus = {cable, *cable.children_recursive}
focus.update(obj for obj in scene.objects if obj.name.startswith('CN21 /'))
board = bpy.data.collections.new('Board context')
scene.collection.children.link(board)
port = bpy.data.collections.new('ST-Link port and cable')
scene.collection.children.link(port)
for obj in list(scene.objects):
    if obj.type in {'LIGHT', 'CAMERA', 'EMPTY'} and obj not in focus:
        continue
    for collection in list(obj.users_collection):
        collection.objects.unlink(obj)
    (port if obj in focus else board).objects.link(obj)

# Trace a thin cyan capsule around the real connector mouth, in CAD coordinates.
curve = bpy.data.curves.new('CN21 outline', 'CURVE')
curve.dimensions = '3D'
curve.bevel_depth = .00008
curve.bevel_resolution = 3
spline = curve.splines.new('POLY')
points = []
for sign, angles in [(1, range(-90, 91, 5)), (-1, range(90, 271, 5))]:
    for angle in angles:
        radians = math.radians(angle)
        points.append((
            -.026399935 + (sign * 3.1 + 1.55 * math.cos(radians)) * .001,
            -.04205,
            .00155 + 1.55 * math.sin(radians) * .001,
            1,
        ))
spline.points.add(len(points) - 1)
for point, coordinates in zip(spline.points, points):
    point.co = coordinates
spline.use_cyclic_u = True
outline = bpy.data.objects.new('CN21 cyan emphasis', curve)
port.objects.link(outline)
material = bpy.data.materials.new('ST-Link cyan')
material.use_nodes = True
shader = material.node_tree.nodes.get('Principled BSDF')
shader.inputs['Base Color'].default_value = (.01, .65, .8, 1)
shader.inputs['Emission Color'].default_value = (.01, .65, .8, 1)
shader.inputs['Emission Strength'].default_value = 2
curve.materials.append(material)

# Blend 25% toward the isolated focus pass, retaining 75% of the board pass.
full_layer = scene.view_layers[0]
full_layer.name = 'Full board'
focus_layer = scene.view_layers.new('Focus only')
focus_layer.layer_collection.children[board.name].exclude = True
compositor = bpy.data.node_groups.new('Board opacity 75 percent', 'CompositorNodeTree')
compositor.interface.new_socket(name='Image', in_out='OUTPUT', socket_type='NodeSocketColor')


def over_black(layer_name):
    render = compositor.nodes.new('CompositorNodeRLayers')
    render.layer = layer_name
    over = compositor.nodes.new('CompositorNodeAlphaOver')
    over.inputs['Background'].default_value = (0, 0, 0, 1)
    compositor.links.new(render.outputs['Image'], over.inputs['Foreground'])
    return over.outputs[0]


mix = compositor.nodes.new('CompositorNodeAlphaOver')
mix.name = 'Board fade: 100 to 75 percent'
compositor.links.new(over_black(full_layer.name), mix.inputs['Background'])
compositor.links.new(over_black(focus_layer.name), mix.inputs['Foreground'])
for frame, factor in [(1, 0), (251, 0), (295, .25), (480, .25)]:
    mix.inputs['Factor'].default_value = factor
    mix.inputs['Factor'].keyframe_insert('default_value', frame=frame)
output = compositor.nodes.new('NodeGroupOutput')
compositor.links.new(mix.outputs[0], output.inputs['Image'])
scene.compositing_node_group = compositor
scene.render.engine = 'BLENDER_EEVEE'
scene.eevee.taa_render_samples = 24
scene.render.film_transparent = True
scene.render.resolution_x = 960
scene.render.resolution_y = 540
scene.render.resolution_percentage = 100
scene['connection_target'] = 'CN21 ST-Link'
scene['final_board_opacity'] = .75
scene['focus_opacity'] = 1.0
scene.frame_set(480)
bpy.ops.wm.save_as_mainfile(
    filepath=str(ROOT / 'tools/blender/Benchpeek_STLINK_board75_60fps.blend')
)

# Batch static pads only after saving, keeping the editable master intact.
bpy.ops.object.select_all(action='DESELECT')
pads = [
    obj for obj in board.objects
    if obj.type == 'MESH'
    and (obj.name.startswith('Pad') or ' / solder contact' in obj.name)
]
for obj in pads:
    obj.select_set(True)
if pads:
    bpy.context.view_layer.objects.active = pads[0]
    bpy.ops.object.join()

destination = ROOT / 'target/stlink-intro'
destination.mkdir(parents=True, exist_ok=True)
frames = range(1, 481) if '--all' in sys.argv else [1, 251, 480]
for frame in frames:
    scene.frame_set(frame)
    scene.render.filepath = str(destination / f'{frame:04d}.png')
    bpy.ops.render.render(write_still=True)
    print('FRAME_DONE', frame, flush=True)
