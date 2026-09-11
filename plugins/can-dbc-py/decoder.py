# `can-dbc-py` decoder: a real (if partial) DBC parser and CAN signal
# extractor, written in Python and shipped as a `benchpeek:decoder` WASM
# component via `componentize-py` - proof that the plugin boundary isn't
# Rust-only, and a step up from `can-raw`'s one-undecoded-signal-per-ID: a
# DBC gives per-signal names, physical units, and healthy ranges for free.
#
# Reads the same `candump`-style `<id>#<hex data>` lines
# `benchpeek-app`'s CAN source emits (see `source.rs::can_frame_line`), the
# same as every other decoder.
#
# The `decoder` resource's constructor takes an optional `config` string
# (see wit/decoder.wit) - the host reads a .dbc file itself (this component
# has no filesystem access; built with `--stub-wasi` to match the host's
# no-WASI linker, see scripts/build-can-dbc-py.sh) and passes its contents
# through as `config`, so any DBC file can be loaded at runtime without a
# rebuild. DBC_TEXT below is only the fallback when no config is given -
# the worked example against `sequences/example.toml`/`kicad/example.net`.
#
# Supports the two DBC byte orders (`@1` Intel/little-endian, `@0`
# Motorola/big-endian) and signed/unsigned signals; does not support
# multiplexed signals or messages that need more than one CAN frame.

from wit_world.imports import types

DBC_TEXT = """
BO_ 769 VEHICLE_STATUS: 8 ECU
 SG_ VBAT : 0|16@1+ (0.01,0) [0|500] "V" ECU
 SG_ RPM : 16|16@1+ (1,0) [0|10000] "rpm" ECU
 SG_ TEMP : 39|16@0- (0.1,-40) [-40|215] "C" ECU
"""


class DbcSignal:
    def __init__(
        self, name, start_bit, length, byte_order, signed, factor, offset, minimum, maximum, unit
    ):
        self.name = name
        self.start_bit = start_bit
        self.length = length
        self.byte_order = byte_order  # 1 = Intel (little-endian), 0 = Motorola (big-endian)
        self.signed = signed
        self.factor = factor
        self.offset = offset
        self.minimum = minimum
        self.maximum = maximum
        self.unit = unit


def parse_dbc(text):
    """Parses the `BO_`/`SG_` subset of DBC syntax into {can_id: [DbcSignal]}."""
    messages = {}
    current_id = None
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if line.startswith("BO_ "):
            # BO_ <id> <name>: <dlc> <sender>
            # DBC convention sets bit 31 on an extended (29-bit) CAN ID to
            # distinguish it from a standard ID with the same numeric
            # value; mask it off to get the raw ID, matching SocketCAN's
            # raw_id() (which already strips the EFF/RTR/ERR flag bits
            # before this decoder ever sees the candump line's <id>#...).
            # A no-op for standard IDs, which are always below 0x800.
            current_id = int(line[len("BO_ ") :].split()[0]) & 0x1FFFFFFF
            messages[current_id] = []
        elif line.startswith("SG_ ") and current_id is not None:
            # SG_ <name> : <start>|<length>@<order><sign> (<factor>,<offset>) [<min>|<max>] "<unit>" <receiver>
            name_part, rest = line[len("SG_ ") :].split(":", 1)
            layout, rest = rest.strip().split(" ", 1)
            start_str, length_order = layout.split("|", 1)
            length_str, order_sign = length_order.split("@", 1)
            factor_offset, rest = rest.strip().split(")", 1)
            factor_str, offset_str = factor_offset.lstrip("(").split(",")
            range_part, rest = rest.strip().split("]", 1)
            min_str, max_str = range_part.lstrip("[").split("|")
            unit = rest.split('"')[1] if '"' in rest else ""
            messages[current_id].append(
                DbcSignal(
                    name=name_part.strip(),
                    start_bit=int(start_str),
                    length=int(length_str),
                    byte_order=int(order_sign[0]),
                    signed=order_sign[1] == "-",
                    factor=float(factor_str),
                    offset=float(offset_str),
                    minimum=float(min_str),
                    maximum=float(max_str),
                    unit=unit,
                )
            )
    return messages


def motorola_bit_positions(start_bit, length):
    """DBC's Motorola (big-endian) bit numbering: `start_bit` is the
    signal's MSB, addressed as byte*8 + bit-within-byte (0=LSB..7=MSB).
    Walking to the next-less-significant bit decrements within a byte, but
    jumps +15 when crossing into the next byte (its MSB) - the standard
    DBC big-endian bit-walk. Returns physical bit indices, MSB first."""
    positions = []
    bit = start_bit
    for _ in range(length):
        positions.append(bit)
        bit = bit + 15 if bit % 8 == 0 else bit - 1
    return positions


def extract_raw(data, sig):
    if sig.byte_order == 1:
        as_int = int.from_bytes(bytes(data), "little")
        raw = (as_int >> sig.start_bit) & ((1 << sig.length) - 1)
    else:
        raw = 0
        for pos in motorola_bit_positions(sig.start_bit, sig.length):
            byte_index, bit_index = pos // 8, pos % 8
            bit = (data[byte_index] >> bit_index) & 1 if byte_index < len(data) else 0
            raw = (raw << 1) | bit
    if sig.signed and raw & (1 << (sig.length - 1)):
        raw -= 1 << sig.length
    return raw


class Decoder:
    def __init__(self, config):
        self.buf = bytearray()
        # `is not None`, not truthiness: an empty-but-present config (e.g.
        # an empty .dbc file) should parse to zero signals, not silently
        # fall back to the worked example as if no config were given at
        # all - only a real `None` means "no config".
        self.messages = parse_dbc(config if config is not None else DBC_TEXT)

    def name(self):
        return "can-dbc-py"

    def signals(self):
        return [
            types.SignalMeta(name=sig.name, unit=sig.unit or None, min=sig.minimum, max=sig.maximum)
            for sigs in self.messages.values()
            for sig in sigs
        ]

    def decode(self, data, t):
        self.buf.extend(data)
        out = []
        while True:
            nl = self.buf.find(b"\n")
            if nl < 0:
                break
            line = bytes(self.buf[:nl])
            del self.buf[: nl + 1]
            out.extend(self._decode_frame(line, t))
        if len(self.buf) > 64 * 1024:
            self.buf.clear()
        return out

    def _decode_frame(self, line, t):
        try:
            text = line.decode("ascii")
        except ValueError:
            return []
        if "#" not in text:
            return []
        id_hex, data_hex = text.split("#", 1)
        try:
            can_id = int(id_hex, 16)
            payload = bytes.fromhex(data_hex)
        except ValueError:
            return []
        sigs = self.messages.get(can_id)
        if not sigs:
            return []
        return [
            types.Sample(signal=sig.name, value=extract_raw(payload, sig) * sig.factor + sig.offset, t=t)
            for sig in sigs
        ]
