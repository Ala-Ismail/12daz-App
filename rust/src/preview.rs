//! Bounded, shape-aware preview geometry.

use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap, HashMap, VecDeque};
use std::sync::atomic::AtomicBool;

use crate::error::{Result, checkpoint};
use crate::geometry::{Kind, Point, Primitive};

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct PreviewRecord {
    pub model: String,
    pub kind: Kind,
    pub points: Vec<Point>,
    pub colour: String,
    pub text: String,
    pub closed: bool,
}

fn segment_distance(point: Point, start: Point, end: Point) -> f64 {
    let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
    let length = dx.hypot(dy);
    let (px, py) = (point[0] - start[0], point[1] - start[1]);
    if length == 0.0 {
        return px.hypot(py);
    }
    let (ux, uy) = (dx / length, dy / length);
    let along = (px * ux + py * uy).max(0.0).min(length);
    (px - along * ux).hypot(py - along * uy)
}

fn anchors(record: &PreviewRecord) -> Vec<usize> {
    let points = &record.points;
    let last = points.len() - 1;
    if last == 0 {
        return vec![0];
    }
    if !record.closed {
        return vec![0, last];
    }
    let best = |candidates: Vec<usize>, measure: &dyn Fn(usize) -> f64| {
        candidates
            .into_iter()
            .rev()
            .max_by(|&a, &b| measure(a).total_cmp(&measure(b)))
    };
    let second = best((1..last).collect(), &|i| {
        segment_distance(points[i], points[0], points[0])
    });
    if let Some(second) = second {
        let candidates = (1..last)
            .filter(|&i| points[i] != points[0] && points[i] != points[second])
            .collect();
        if let Some(third) = best(candidates, &|i| {
            segment_distance(points[i], points[0], points[second])
        }) {
            return vec![0, second, third, last];
        }
    }
    vec![0, last]
}

fn spatial_records(
    records: &[PreviewRecord],
    minimums: &[Vec<usize>],
    mut allowance: usize,
) -> Vec<usize> {
    let (mut lower, mut upper) = (records[0].points[0], records[0].points[0]);
    for record in records {
        for axis in 0..2 {
            lower[axis] = lower[axis].min(record.points[0][axis]);
            upper[axis] = upper[axis].max(record.points[0][axis]);
        }
    }
    let side = allowance.isqrt().max(2);
    let mut tiles: BTreeMap<[usize; 2], VecDeque<usize>> = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let tile = std::array::from_fn(|axis| {
            let width = upper[axis] - lower[axis];
            if width == 0.0 {
                0
            } else {
                ((record.points[0][axis] - lower[axis]) / width * (side - 1) as f64) as usize
            }
        });
        tiles.entry(tile).or_default().push_back(index);
    }
    let mut order = VecDeque::new();
    while let Some((_, first)) = tiles.pop_first() {
        order.push_back(first);
        if let Some((_, last)) = tiles.pop_last() {
            order.push_back(last);
        }
    }
    let mut chosen = Vec::new();
    while allowance > 0
        && let Some(mut bucket) = order.pop_front()
    {
        let index = bucket
            .pop_front()
            .expect("only occupied tiles enter the queue");
        if minimums[index].len() <= allowance {
            chosen.push(index);
            allowance -= minimums[index].len();
        }
        if !bucket.is_empty() {
            order.push_back(bucket);
        }
    }
    chosen.sort_unstable();
    chosen
}

#[derive(Clone, Copy)]
struct Candidate {
    error: f64,
    path: usize,
    index: usize,
    left: usize,
    right: usize,
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.error.total_cmp(&other.error).then_with(|| {
            (self.path, self.index, self.left, self.right).cmp(&(
                other.path,
                other.index,
                other.left,
                other.right,
            ))
        })
    }
}
impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Candidate {}

// Endpoints keep their original array positions. Removing a vertex only changes its neighbours.
struct Links {
    previous: Vec<usize>,
    next: Vec<usize>,
}
const REMOVED: usize = usize::MAX;

fn candidate(
    path: usize,
    index: usize,
    points: &[Point],
    links: &Links,
    minimum: f64,
) -> Reverse<Candidate> {
    let (left, right) = (links.previous[index], links.next[index]);
    let error = segment_distance(points[index], points[left], points[right]).max(minimum);
    Reverse(Candidate {
        error,
        path,
        index,
        left,
        right,
    })
}

fn simplify(
    mut records: Vec<PreviewRecord>,
    allowance: usize,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<PreviewRecord>> {
    let mut minimums: Vec<_> = records.iter().map(anchors).collect();
    if minimums.iter().map(Vec::len).sum::<usize>() > allowance {
        let chosen = spatial_records(&records, &minimums, allowance);
        if chosen.is_empty() && allowance > 0 {
            let mut record = records.remove(0);
            record.kind = Kind::Point;
            record.closed = false;
            record.points.truncate(1);
            return Ok(vec![record]);
        }
        let retained = |index: usize| chosen.binary_search(&index).is_ok();
        records = records
            .into_iter()
            .enumerate()
            .filter_map(|(i, record)| retained(i).then_some(record))
            .collect();
        minimums = minimums
            .into_iter()
            .enumerate()
            .filter_map(|(i, anchors)| retained(i).then_some(anchors))
            .collect();
    }
    let mut excess = records
        .iter()
        .map(|record| record.points.len())
        .sum::<usize>()
        .saturating_sub(allowance);
    if excess == 0 {
        return Ok(records);
    }
    let mut links = Vec::new();
    let mut candidates = Vec::new();
    for (path, record) in records.iter().enumerate() {
        checkpoint(cancel)?;
        let size = record.points.len();
        let path_links = Links {
            previous: std::iter::once(REMOVED).chain(0..size - 1).collect(),
            next: (1..=size).collect(),
        };
        for index in 1..size - 1 {
            if index % 4096 == 0 {
                checkpoint(cancel)?;
            }
            if !minimums[path].contains(&index) {
                candidates.push(candidate(path, index, &record.points, &path_links, 0.0));
            }
        }
        links.push(path_links);
    }
    let mut queue = BinaryHeap::from(candidates);
    while excess > 0 {
        checkpoint(cancel)?;
        let Reverse(Candidate {
            error,
            path,
            index,
            left,
            right,
        }) = queue.pop().expect("minimum anchors fit the budget");
        let path_links = &mut links[path];
        if (left, right) != (path_links.previous[index], path_links.next[index]) {
            continue;
        }
        path_links.next[left] = right;
        path_links.previous[right] = left;
        path_links.next[index] = REMOVED;
        path_links.previous[index] = REMOVED;
        excess -= 1;
        for neighbour in [left, right] {
            if !minimums[path].contains(&neighbour) {
                queue.push(candidate(
                    path,
                    neighbour,
                    &records[path].points,
                    path_links,
                    error,
                ));
            }
        }
    }
    for (record, path_links) in records.iter_mut().zip(links) {
        let mut next = path_links.next.into_iter();
        record.points.retain(|_| next.next() != Some(REMOVED));
    }
    Ok(records)
}

struct ModelPreview {
    name: String,
    records: Vec<PreviewRecord>,
    size: usize,
}

pub struct PreviewCollector<'a> {
    limit: usize,
    cancel: Option<&'a AtomicBool>,
    model_indices: HashMap<String, usize>,
    models: Vec<ModelPreview>,
    total: usize,
    limited: bool,
}

pub struct Preview {
    pub records: Vec<PreviewRecord>,
    pub omitted_models: Vec<String>,
    pub limited: bool,
}

impl<'a> PreviewCollector<'a> {
    pub fn new(limit: usize, cancel: Option<&'a AtomicBool>) -> Self {
        Self {
            limit,
            cancel,
            model_indices: HashMap::new(),
            models: Vec::new(),
            total: 0,
            limited: false,
        }
    }

    pub fn add(
        &mut self,
        model: &str,
        primitive: &Primitive,
        mut points: Vec<Point>,
    ) -> Result<()> {
        if self.limit == 0 || points.is_empty() {
            return Ok(());
        }
        let closed = points.len() >= 4 && (primitive.closed || primitive.kind == Kind::Circle);
        if closed {
            let last = points.len() - 1;
            points[last] = points[0];
        }
        let index = *self.model_indices.entry(model.into()).or_insert_with(|| {
            let index = self.models.len();
            self.models.push(ModelPreview {
                name: model.into(),
                records: vec![],
                size: 0,
            });
            index
        });
        self.total += points.len();
        self.models[index].size += points.len();
        self.models[index].records.push(PreviewRecord {
            model: model.into(),
            kind: primitive.kind,
            points,
            colour: primitive.colour.clone(),
            text: primitive.text.clone(),
            closed,
        });
        if self.total > self.limit.saturating_mul(2) {
            self.compact()?;
        }
        Ok(())
    }

    fn compact(&mut self) -> Result<()> {
        if self.total <= self.limit {
            return Ok(());
        }
        self.limited = true;
        let mut indices: Vec<_> = (0..self.models.len()).collect();
        indices.sort_by_key(|&index| self.models[index].size);
        let mut remaining = self.limit;
        for (index, model_index) in indices.into_iter().enumerate() {
            checkpoint(self.cancel)?;
            let share = remaining.div_ceil(self.models.len() - index);
            let model = &mut self.models[model_index];
            if model.size > share {
                let previous = std::mem::take(&mut model.records);
                model.records = if share == 0 {
                    Vec::new()
                } else {
                    simplify(previous, share, self.cancel)?
                };
                model.size = model.records.iter().map(|record| record.points.len()).sum();
            }
            remaining -= model.size;
        }
        self.total = self.limit - remaining;
        Ok(())
    }

    pub fn finish(mut self) -> Result<Preview> {
        self.compact()?;
        let mut records = Vec::new();
        let mut omitted_models = Vec::new();
        for model in self.models {
            if model.records.is_empty() {
                omitted_models.push(model.name);
            } else {
                records.extend(model.records);
            }
        }
        Ok(Preview {
            records,
            omitted_models,
            limited: self.limited,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_07_preview_preserves_models_and_shape_within_limit() {
        let original: Vec<_> = (0..50)
            .map(|x| [x as f64, if x == 25 { 100.0 } else { 0.0 }])
            .collect();
        let dense = Primitive::new(Kind::Polyline, original.clone(), "red", "1");
        let isolated = Primitive::new(Kind::Point, vec![[1000.0, 0.0]], "green", "1");
        let mut collector = PreviewCollector::new(5, None);
        collector
            .add("First", &dense, dense.points.clone())
            .unwrap();
        collector
            .add("Later", &isolated, isolated.points.clone())
            .unwrap();
        let preview = collector.finish().unwrap();
        assert!(preview.limited && preview.omitted_models.is_empty());
        assert_eq!(
            preview
                .records
                .iter()
                .map(|record| record.points.len())
                .sum::<usize>(),
            5
        );
        assert!(preview.records[0].points.contains(&[25.0, 100.0]));
        assert_eq!(preview.records[1].points, isolated.points);
        assert_eq!(dense.points, original);
    }

    #[test]
    fn rust_07_closed_outlines_retain_three_vertices_and_closure() {
        let mut primitive = Primitive::new(Kind::Polyline, vec![], "red", "1");
        primitive.closed = true;
        let outline = vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [2.0, 0.0],
            [2.0, 2.0],
            [0.0, 2.0],
            [0.0, 0.0],
        ];
        let mut collector = PreviewCollector::new(4, None);
        collector.add("Outline", &primitive, outline).unwrap();
        let preview = collector.finish().unwrap();
        let points = &preview.records[0].points;
        assert!(preview.records[0].closed);
        assert_eq!(points.len(), 4);
        assert_eq!(points.first(), points.last());
        assert!(points[0] != points[1] && points[0] != points[2] && points[1] != points[2]);
    }
}
