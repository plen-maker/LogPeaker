# Benchpeek connector library

Six illustrative assets for connection guides and hardware animations. These are generic visual models, not manufacturer CAD or fit-checked mechanical parts.

| Asset | Details |
| --- | --- |
| `USB_A_plug` | Hollow nickel shell, blue tongue, gold contacts, black overmould and cable stub |
| `USB_C_plug` | Hollow capsule shell, two rows of contacts, white overmould and cable stub |
| `SD_card` | Clipped corner, write-protect slider, label and nine underside contacts |
| `microSD_card` | Stepped silhouette, label and eight underside contacts |
| `RJ45_plug` | Eight contacts, retaining latch, teal boot and cable stub |
| `SWD_10pin_plug` | Keyed housing, ten socket openings and ribbon cable stub |

## Use

- Open `Benchpeek_accessories.blend` for the staged overview. Each model is a named collection marked as a Blender asset. Append its collection or add this folder as an Asset Library.
- Move or animate the model's `insertion origin` parent empty. In Blender local coordinates, the mating face is at Y=0 and insertion travels along **−Y**. Children retain independently editable materials and geometry. Reset the parent transform after appending if you need the unstaged position.
- Individual `.glb` files are exported at the origin, before staging, with evaluated bevels and mesh lettering. glTF uses its conventional Y-up coordinates; Blender's −Y insertion direction becomes glTF +Z.
- Units are meters; object dimensions are at an approximate physical scale. SD and microSD bodies are approximately 24×32×2.1 mm and 11×15×1 mm. Cable stubs can be replaced with animated curves.
- The overview scene defaults to 60 fps; the assets themselves are static and impose no animation frame rate.

Rebuild all files and the preview with Blender 5.x:

```sh
blender -b --python tools/blender/accessories/build_accessories.py
```

`accessories_preview.png` shows all six assets at the same scale. Materials and labels are self-contained; no external textures are needed.
