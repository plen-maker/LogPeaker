"""Package rendered ST-Link frames; requires Pillow and ffmpeg on PATH."""
from pathlib import Path
import io
import struct
import subprocess

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
FRAMES = ROOT / 'target/stlink-intro'
COUNT = 480


def main():
    payloads = []
    index = []
    seen = {}
    offset = 8 + COUNT * 8
    for frame in range(1, COUNT + 1):
        with Image.open(FRAMES / f'{frame:04d}.png') as image:
            assert image.size == (960, 540), (frame, image.size)
            output = io.BytesIO()
            image.convert('RGB').save(output, format='JPEG', quality=91, subsampling=0)
        data = output.getvalue()
        if data not in seen:
            seen[data] = (offset, len(data))
            payloads.append(data)
            offset += len(data)
        index.extend(seen[data])
    pack = b'BPK2' + struct.pack('<I', COUNT)
    pack += struct.pack(f'<{COUNT * 2}I', *index) + b''.join(payloads)
    destination = ROOT / 'crates/benchpeek-app/assets/board_intro.bpk'
    destination.write_bytes(pack)
    subprocess.run([
        'ffmpeg', '-y', '-loglevel', 'error', '-framerate', '60',
        '-start_number', '1', '-i', str(FRAMES / '%04d.png'),
        '-frames:v', str(COUNT), '-c:v', 'libx264', '-crf', '17',
        '-preset', 'medium', '-pix_fmt', 'yuv420p', '-movflags', '+faststart',
        str(FRAMES / 'Benchpeek_STLink_75.mp4'),
    ], check=True)
    print(f'Packed {COUNT} frames, {len(seen)} unique; {len(pack):,} bytes')


if __name__ == '__main__':
    main()
