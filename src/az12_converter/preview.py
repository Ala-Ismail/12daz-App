"""Bounded, shape-aware display geometry for the plan preview."""

from collections import deque
import heapq
import math

from .errors import checkpoint


def segment_distance(point, start, end):
    dx, dy = end[0] - start[0], end[1] - start[1]
    length = math.hypot(dx, dy)
    px, py = point[0] - start[0], point[1] - start[1]
    if not length:
        return math.hypot(px, py)
    ux, uy = dx / length, dy / length
    along = max(0.0, min(length, px * ux + py * uy))
    return math.hypot(px - along * ux, py - along * uy)


def anchors(record):
    points = record["points"]
    last = len(points) - 1
    if not record["closed"]:
        return {0, last}
    second = max(range(1, last), key=lambda i: math.dist(points[0], points[i]))
    third = max((i for i in range(1, last) if points[i] not in (points[0], points[second])),
                key=lambda i: segment_distance(points[i], points[0], points[second]), default=None)
    return {0, second, third, last} if third is not None else {0, last}


def spatial_records(records, minimums, allowance):
    """Offer each occupied tile a feature before adding detail to dense tiles."""
    locations = [record["points"][0] for record in records]
    xs, ys = zip(*locations)
    min_x, min_y = min(xs), min(ys)
    width, height = max(xs) - min_x, max(ys) - min_y
    side = max(2, math.isqrt(allowance))
    buckets = {}
    for index, (x, y) in enumerate(locations):
        tile = (int((x - min_x) / width * (side - 1)) if width else 0,
                int((y - min_y) / height * (side - 1)) if height else 0)
        buckets.setdefault(tile, deque()).append(index)
    tiles = deque(sorted(buckets))
    # Visit the spatial extremes first when the budget can represent only a few tiles.
    order = deque()
    while tiles:
        order.append(buckets[tiles.popleft()])
        if tiles:
            order.append(buckets[tiles.pop()])
    chosen = []
    while order and allowance:
        bucket = order.popleft()
        index = bucket.popleft()
        cost = len(minimums[index])
        if cost <= allowance:
            chosen.append(index)
            allowance -= cost
        if bucket:
            order.append(bucket)
    return sorted(chosen)


def simplify(records, allowance, cancel):
    minimums = [anchors(record) for record in records]
    if sum(map(len, minimums)) > allowance:
        chosen = spatial_records(records, minimums, allowance)
        if not chosen and allowance:
            # A point can indicate a model's position when no complete path fits.
            return [{**records[0], "kind": "point", "closed": False, "points": records[0]["points"][:1]}]
        records = [records[i] for i in chosen]
        minimums = [minimums[i] for i in chosen]

    excess = sum(len(record["points"]) for record in records) - allowance
    if excess <= 0:
        return records
    previous, following, queue = [], [], []

    def deviation(path, index):
        points = records[path]["points"]
        return segment_distance(points[index], points[previous[path][index]], points[following[path][index]])

    for path, (record, protected) in enumerate(zip(records, minimums)):
        checkpoint(cancel)
        size = len(record["points"])
        previous.append(list(range(-1, size - 1)))
        following.append(list(range(1, size + 1)))
        for i in range(1, size - 1):
            if i % 4096 == 0:
                checkpoint(cancel)
            if i not in protected:
                queue.append((deviation(path, i), path, i, i - 1, i + 1))
    heapq.heapify(queue)
    while excess:
        checkpoint(cancel)
        error, path, index, left, right = heapq.heappop(queue)
        if (left, right) != (previous[path][index], following[path][index]):
            continue
        following[path][left], previous[path][right] = right, left
        previous[path][index] = following[path][index] = -1
        excess -= 1
        for neighbor in (left, right):
            if neighbor not in minimums[path]:
                heapq.heappush(queue, (max(error, deviation(path, neighbor)), path, neighbor,
                                      previous[path][neighbor], following[path][neighbor]))
    return [{**record, "points": [point for i, point in enumerate(record["points"]) if following[path][i] != -1]}
            for path, record in enumerate(records)]


class PreviewCollector:
    """Batch compaction keeps display memory bounded as source objects stream in."""

    def __init__(self, point_limit, cancel=None):
        self.limit = max(0, point_limit)
        self.cancel = cancel
        self.models = {}
        self.sizes = {}
        self.total = 0
        self.limited = False

    def add(self, model, primitive, points):
        if not points or not self.limit:
            return
        closed = len(points) >= 4 and (primitive.closed or primitive.kind == "circle")
        if closed and points[-1] != points[0]:
            points = [*points[:-1], points[0]]
        self.models.setdefault(model, []).append({"model": model, "kind": primitive.kind, "points": points,
                                                 "colour": primitive.colour, "text": primitive.text, "closed": closed})
        self.sizes[model] = self.sizes.get(model, 0) + len(points)
        self.total += len(points)
        if self.total > self.limit * 2:
            self._compact()

    def _compact(self):
        if self.total <= self.limit:
            return
        self.limited = True
        remaining = self.limit
        # Satisfy small models first so the others can use their unused share.
        for index, model in enumerate(sorted(self.models, key=self.sizes.__getitem__)):
            checkpoint(self.cancel)
            count = len(self.models) - index
            share = (remaining + count - 1) // count
            if self.sizes[model] > share:
                self.models[model] = simplify(self.models[model], share, self.cancel) if share else []
                self.sizes[model] = sum(len(record["points"]) for record in self.models[model])
            remaining -= self.sizes[model]
        self.total = self.limit - remaining

    def result(self):
        self._compact()
        return [record for records in self.models.values() for record in records]

    def omitted_models(self):
        return [model for model, records in self.models.items() if not records]
