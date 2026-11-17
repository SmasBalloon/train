#!/usr/bin/env python3
"""Fabrique data/world.txt, le fond de carte du jeu, à partir de Natural Earth.

Natural Earth (https://www.naturalearthdata.com) est dans le domaine public.
Couches attendues, au 1:50 000 000, en GeoJSON (dépôt nvkelso/natural-earth-vector,
dossier geojson/) :

    ne_50m_admin_0_countries.geojson
    ne_50m_admin_0_boundary_lines_land.geojson
    ne_50m_rivers_lake_centerlines.geojson

Usage : python3 -I tools/world.py DOSSIER_DES_GEOJSON > data/world.txt

Seule l'Europe est gardée (voir BBOX), simplifiée à quelques kilomètres près.
"""

import json
import math
import os
import sys

# Longitude ouest, latitude sud, longitude est, latitude nord.
BBOX = (-25.0, 33.0, 62.0, 72.0)
# Tolérance de simplification (degrés) : de l'ordre de 2 km.
TOLERANCE = 0.02
# Îles et lacs plus petits que cette aire (degrés carrés) : ignorés.
MIN_AREA = 0.004
# Rivières : les plus importantes seulement (rang Natural Earth).
MAX_RIVER_RANK = 6

# Noms français des fleuves (Natural Earth donne le nom local ou anglais).
RIVER_NAMES = {
    "Thames": "Tamise", "Rhine": "Rhin", "Rhein": "Rhin", "Nederrijn": "Rhin",
    "Donau": "Danube", "Soroksari Duna": "Danube", "Tajo": "Tage", "Tejo": "Tage",
    "Duero": "Douro", "Vistula": "Vistule", "Dnipro": "Dniepr", "Dnepre": "Dniepr",
    "Dniester": "Dniestr", "Oder": "Oder", "Tisza": "Tisza", "Tisa": "Tisza",
    "Severnaya Dvina": "Dvina du Nord", "Daugava": "Daugava", "Svir’": "Svir",
    "Euphrates": "Euphrate", "Al Furat": "Euphrate", "Firat": "Euphrate",
    "Tigris": "Tigre", "Dicle": "Tigre", "Jordan": "Jourdain", "Po": "Pô",
}

# Petites rivières absentes de Natural Earth, mais chères au jeu.
EXTRA_RIVERS = {
    # La Tees, entre Darlington et Stockton : la toute première ligne la longe.
    "Tees": [(-2.08, 54.63), (-1.92, 54.54), (-1.70, 54.52), (-1.55, 54.50),
             (-1.42, 54.49), (-1.35, 54.51), (-1.31, 54.57), (-1.22, 54.58),
             (-1.13, 54.62)],
}


def load(folder, name):
    with open(os.path.join(folder, name), encoding="utf-8") as f:
        return json.load(f)["features"]


def rings_of(geometry):
    """Anneaux (extérieurs et trous) d'un polygone ou multipolygone."""
    kind, coords = geometry["type"], geometry["coordinates"]
    if kind == "Polygon":
        return list(coords)
    if kind == "MultiPolygon":
        return [ring for polygon in coords for ring in polygon]
    return []


def lines_of(geometry):
    kind, coords = geometry["type"], geometry["coordinates"]
    if kind == "LineString":
        return [coords]
    if kind == "MultiLineString":
        return list(coords)
    return []


def area(ring):
    return abs(sum(a[0] * b[1] - b[0] * a[1] for a, b in zip(ring, ring[1:] + ring[:1]))) / 2


def clip_ring(ring):
    """Découpe un anneau au rectangle BBOX (Sutherland-Hodgman)."""
    west, south, east, north = BBOX
    edges = [
        (lambda p: p[0] >= west, lambda a, b: cut_x(a, b, west)),
        (lambda p: p[0] <= east, lambda a, b: cut_x(a, b, east)),
        (lambda p: p[1] >= south, lambda a, b: cut_y(a, b, south)),
        (lambda p: p[1] <= north, lambda a, b: cut_y(a, b, north)),
    ]
    points = ring[:-1] if ring and ring[0] == ring[-1] else ring
    for inside, cut in edges:
        out = []
        for i, cur in enumerate(points):
            prev = points[i - 1]
            if inside(cur):
                if not inside(prev):
                    out.append(cut(prev, cur))
                out.append(cur)
            elif inside(prev):
                out.append(cut(prev, cur))
        points = out
        if not points:
            break
    return points


def cut_x(a, b, x):
    t = (x - a[0]) / (b[0] - a[0])
    return [x, a[1] + t * (b[1] - a[1])]


def cut_y(a, b, y):
    t = (y - a[1]) / (b[1] - a[1])
    return [a[0] + t * (b[0] - a[0]), y]


def inside_bbox(p, margin=0.0):
    west, south, east, north = BBOX
    return west - margin <= p[0] <= east + margin and south - margin <= p[1] <= north + margin


def runs_in_bbox(line):
    """Morceaux d'une ligne qui passent dans BBOX (un point de marge de chaque côté)."""
    runs, current = [], []
    for i, p in enumerate(line):
        near = inside_bbox(p) or (i > 0 and inside_bbox(line[i - 1])) or (
            i + 1 < len(line) and inside_bbox(line[i + 1])
        )
        if near:
            current.append(p)
        elif current:
            runs.append(current)
            current = []
    if current:
        runs.append(current)
    return [r for r in runs if len(r) >= 2]


def simplify(points, tolerance=TOLERANCE):
    """Douglas-Peucker."""
    if len(points) < 3:
        return points
    keep = [False] * len(points)
    keep[0] = keep[-1] = True
    stack = [(0, len(points) - 1)]
    while stack:
        first, last = stack.pop()
        ax, ay = points[first]
        bx, by = points[last]
        dx, dy = bx - ax, by - ay
        norm = math.hypot(dx, dy)
        worst, index = 0.0, None
        for i in range(first + 1, last):
            px, py = points[i]
            if norm == 0:
                d = math.hypot(px - ax, py - ay)
            else:
                d = abs(dy * px - dx * py + bx * ay - by * ax) / norm
            if d > worst:
                worst, index = d, i
        if index is not None and worst > tolerance:
            keep[index] = True
            stack += [(first, index), (index, last)]
    return [p for p, k in zip(points, keep) if k]


def fmt(points):
    out, last = [], None
    for lon, lat in points:
        cell = f"{lon:.2f},{lat:.2f}"
        if cell != last:
            out.append(cell)
            last = cell
    return " ".join(out)


def emit(kind, tag, points, closed=False):
    if closed:
        if len(points) < 3 or area(points) < MIN_AREA:
            return
    elif len(points) < 2:
        return
    text = fmt(simplify(points + points[:1]) if closed else simplify(points))
    if len(text.split()) >= (3 if closed else 2):
        print(f"{kind}\t{tag}\t{text}")


def main():
    folder = sys.argv[1]
    print("# Fond de carte de l'Europe, généré par tools/world.py à partir de Natural Earth")
    print("# (domaine public, https://www.naturalearthdata.com), échelle 1:50 000 000.")
    print("# Une ligne par tracé : type, étiquette, puis les points « longitude,latitude ».")
    print("#   C  contour d'un pays (étiquette : code ADM0_A3), pour le remplissage")
    print("#   B  frontière terrestre          R  fleuve (nom)")
    print("# Les côtes se déduisent des contours des pays.")

    for feature in load(folder, "ne_50m_admin_0_countries.geojson"):
        code = feature["properties"]["ADM0_A3"]
        for ring in rings_of(feature["geometry"]):
            emit("C", code, clip_ring(ring), closed=True)

    for feature in load(folder, "ne_50m_admin_0_boundary_lines_land.geojson"):
        for line in lines_of(feature["geometry"]):
            for run in runs_in_bbox(line):
                emit("B", "-", run)

    for feature in load(folder, "ne_50m_rivers_lake_centerlines.geojson"):
        props = feature["properties"]
        if props.get("featurecla") != "River" or (props.get("scalerank") or 99) > MAX_RIVER_RANK:
            continue
        name = (props.get("name_fr") or props.get("name") or "").strip()
        if not name:
            continue
        name = RIVER_NAMES.get(name, name)
        for line in lines_of(feature["geometry"]):
            for run in runs_in_bbox(line):
                emit("R", name.replace("\t", " "), run)

    for name, line in EXTRA_RIVERS.items():
        print(f"R\t{name}\t{fmt(line)}")


if __name__ == "__main__":
    main()
