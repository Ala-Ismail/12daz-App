"""Bounded display geometry with a fair allowance for each source model."""


def spaced_indices(length, count):
    if count >= length:
        return list(range(length))
    if count == 1:
        return [0]
    return [i * (length - 1) // (count - 1) for i in range(count)]


class PreviewCollector:
    """Keep models represented even when an earlier model exhausts the display cap.

    Simplification is for display only. Exact output primitives are never mutated.
    Both paths and points are sampled if necessary; the UI labels this explicitly.
    """

    def __init__(self, point_limit):
        self.limit = max(0, point_limit)
        self.models = {}
        self.limited = False

    def add(self, model, primitive, points):
        if not points or not self.limit:
            return
        is_new = model not in self.models
        records = self.models.setdefault(model, [])
        records.append({"model": model, "kind": primitive.kind, "points": points,
                        "colour": primitive.colour, "text": primitive.text})
        if is_new:
            # Late models get the same allowance as models encountered first.
            self._rebalance()
        else:
            self._compact(model, self._allowances()[model])

    def _allowances(self):
        count = len(self.models)
        if not count:
            return {}
        per_model, remainder = divmod(self.limit, count)
        return {model: per_model + (index < remainder) for index, model in enumerate(self.models)}

    def _rebalance(self):
        for model, allowance in self._allowances().items():
            self._compact(model, allowance)

    def _compact(self, model, allowance):
        records = self.models[model]
        total = sum(len(record["points"]) for record in records)
        if total <= allowance:
            return
        self.limited = True
        if not allowance:
            self.models[model] = []
            return
        # Retain both endpoints of each displayed path where the allowance permits.
        minimum = lambda record: min(len(record["points"]), 2)
        if sum(minimum(record) for record in records) > allowance:
            count = max(1, allowance // 2)
            records = [records[i] for i in spaced_indices(len(records), count)]
        if allowance == 1:
            record = {**records[0], "kind": "point", "points": records[0]["points"][:1]}
            self.models[model] = [record]
            return
        base = sum(minimum(record) for record in records)
        spare = allowance - base
        weights = [len(record["points"]) - minimum(record) for record in records]
        total_weight = sum(weights)
        allocated, cumulative = 0, 0
        compacted = []
        for record, weight in zip(records, weights):
            cumulative += weight
            target = spare * cumulative // total_weight if total_weight else 0
            count = minimum(record) + target - allocated
            allocated = target
            points = record["points"]
            compacted.append({**record, "points": [points[i] for i in spaced_indices(len(points), count)]})
        self.models[model] = compacted

    def result(self):
        return [record for records in self.models.values() for record in records]

    def omitted_models(self):
        return [model for model, records in self.models.items() if not records]
