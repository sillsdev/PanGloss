"""Generate the WordMaker category table from Unicode 14.0.0 UnicodeData.txt."""
import hashlib
import pathlib
import struct
import sys

source = pathlib.Path(sys.argv[1]).read_bytes()
if hashlib.sha256(source).hexdigest() != "36018e68657fdcb3485f636630ffe8c8532e01c977703d2803f5b89d6c5feafb":
    raise SystemExit("expected official Unicode 14.0.0 UnicodeData.txt")
categories = {"Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Sk", "Nd", "Nl", "No"}
points = set()
first = None
for line in source.decode("utf-8").splitlines():
    fields = line.split(";")
    value, name, category = int(fields[0], 16), fields[1], fields[2]
    if name.endswith(", First>"):
        first = value
    elif name.endswith(", Last>"):
        if first is None:
            raise SystemExit("unpaired range")
        if category in categories:
            points.update(range(first, value + 1))
        first = None
    elif category in categories:
        points.add(value)
if first is not None:
    raise SystemExit("unpaired range")
ranges = []
for value in sorted(points):
    if ranges and ranges[-1][1] + 1 == value:
        ranges[-1] = (ranges[-1][0], value)
    else:
        ranges.append((value, value))
path = pathlib.Path(__file__).parents[1] / "src" / "unicode-14.bin"
path.write_bytes(b"".join(struct.pack("<II", *pair) for pair in ranges))
