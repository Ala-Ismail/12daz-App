"""Small code-drawn, DPI-scaled interface icons; no external image dependency."""
import base64
from functools import lru_cache
import math
import struct
import zlib
import tkinter as tk


def _inside(x, y, points):
    inside = False
    previous = points[-1]
    for point in points:
        ax, ay = previous
        bx, by = point
        if (ay > y) != (by > y) and x < (bx - ax) * (y - ay) / (by - ay) + ax:
            inside = not inside
        previous = point
    return inside


def _distance(x, y, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    length = dx * dx + dy * dy
    t = max(0, min(1, ((x - a[0]) * dx + (y - a[1]) * dy) / length)) if length else 0
    return math.hypot(x - a[0] - t * dx, y - a[1] - t * dy)


@lru_cache(maxsize=80)
def icon_data(name, colour, size):
    colour = tuple(bytes.fromhex(colour.lstrip('#')))
    shapes = []
    def line(points, width=1.7, ink=colour):
        shapes.append(('line', points, width, ink))
    def polygon(points, ink=colour):
        shapes.append(('polygon', points, 0, ink))
    if name == 'logo':
        for y in (15, 10, 5):
            polygon([(2, y + 3), (12, y - 2), (22, y + 3), (12, y + 8)])
            polygon([(6, y + 3), (12, y), (18, y + 3), (12, y + 6)], (242, 246, 252))
    elif name == 'folder':
        line([(3, 19), (3, 5), (10, 5), (12, 8), (21, 8), (21, 11)])
        line([(3, 19), (7, 11), (23, 11), (19, 19), (3, 19)])
    elif name == 'file':
        line([(5, 3), (14, 3), (19, 8), (19, 21), (5, 21), (5, 3)])
        line([(14, 3), (14, 8), (19, 8)])
    elif name == 'search':
        shapes.append(('ring', (10, 10, 6), 1.7, colour))
        line([(15, 15), (21, 21)])
    elif name == 'fit':
        for points in [[(3, 9), (3, 3), (9, 3)], [(15, 3), (21, 3), (21, 9)], [(21, 15), (21, 21), (15, 21)], [(9, 21), (3, 21), (3, 15)]]:
            line(points)
    elif name in {'zoom_in', 'zoom_out'}:
        shapes.append(('ring', (10, 10, 6), 1.7, colour))
        line([(15, 15), (21, 21)])
        line([(7, 10), (13, 10)], 1.5)
        if name == 'zoom_in':
            line([(10, 7), (10, 13)], 1.5)
    elif name in {'checked', 'empty'}:
        if name == 'checked':
            polygon([(3, 2), (21, 2), (22, 3), (22, 21), (21, 22), (3, 22), (2, 21), (2, 3)])
            line([(6, 12), (10, 16), (18, 8)], 2, (255, 255, 255))
        else:
            line([(4, 3), (20, 3), (21, 4), (21, 20), (20, 21), (4, 21), (3, 20), (3, 4), (4, 3)], 1.3)
    elif name == 'export':
        line([(10, 3), (4, 3), (4, 21), (10, 21)])
        line([(10, 12), (22, 12), (17, 7)])
        line([(22, 12), (17, 17)])
    elif name == 'warning':
        polygon([(12, 2), (23, 21), (1, 21)])
        line([(12, 8), (12, 13)], 2, (255, 255, 255))
        shapes.append(('circle', (12, 17, 1), 0, (255, 255, 255)))
    elif name == 'success':
        shapes.append(('circle', (12, 12, 10), 0, colour))
        line([(6, 12), (10, 16), (18, 8)], 2, (255, 255, 255))
    else:
        shapes.append(('ring', (12, 12, 9), 1.7, colour))
        line([(12, 10), (12, 17)], 1.8)
        shapes.append(('circle', (12, 6, 1), 0, colour))

    data = bytearray()
    # Four coverage samples per pixel give clean edges without an imaging library.
    for row in range(size):
        data.append(0)
        for col in range(size):
            samples = []
            for ox, oy in ((.25, .25), (.75, .25), (.25, .75), (.75, .75)):
                x, y = (col + ox) * 24 / size, (row + oy) * 24 / size
                pixel = None
                for kind, coordinates, width, ink in shapes:
                    if kind == 'polygon':
                        hit = _inside(x, y, coordinates)
                    elif kind == 'line':
                        hit = any(_distance(x, y, a, b) <= width / 2 for a, b in zip(coordinates, coordinates[1:]))
                    else:
                        cx, cy, radius = coordinates
                        distance = math.hypot(x - cx, y - cy)
                        hit = distance <= radius if kind == 'circle' else abs(distance - radius) <= width / 2
                    if hit:
                        pixel = ink
                if pixel:
                    samples.append(pixel)
            if samples:
                data.extend([*(round(sum(p[i] for p in samples) / len(samples)) for i in range(3)), round(len(samples) / 4 * 255)])
            else:
                data.extend((0, 0, 0, 0))
    def chunk(kind, payload):
        return struct.pack('!I', len(payload)) + kind + payload + struct.pack('!I', zlib.crc32(kind + payload))
    png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('!2I5B', size, size, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(data)) + chunk(b'IEND', b'')
    return base64.b64encode(png).decode('ascii')


def make_icon(master, name, colour='#0969da', size=24):
    return tk.PhotoImage(master=master, data=icon_data(name, colour, size), format='png')
