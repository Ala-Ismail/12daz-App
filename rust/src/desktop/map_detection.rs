use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Arc, mpsc},
    time::Duration,
};

use az12_converter::map::{
    LonLat, SourceUnit,
    corridor_match::{self, Assessment, Status, Sweep, SweepStep},
    detection::{ArchiveEvidence, Candidate, Declaration, Detection, evaluate},
    road_match::{self, Grid, Recommendation, Segment},
};
use az12_converter::{geometry::Kind, preview::PreviewRecord};
use eframe::egui;

use super::backdrop::crs_label;

type MatchResult = Result<(Recommendation, String), String>;
struct SweepResult {
    assessments: Vec<Assessment>,
    reference_warning: Option<String>,
}

enum SweepEvent {
    LoadingRoads,
    Checking(SweepStep),
    Finished(Result<SweepResult, String>),
}

#[derive(PartialEq)]
struct RankingInputs {
    revision: u64,
    unit: SourceUnit,
    zone: Option<u8>,
    road_model: String,
    boundary_model: String,
}

#[derive(Default)]
pub(super) struct ProjectionCheck {
    evidence: Option<Arc<ArchiveEvidence>>,
    worker: Option<mpsc::Receiver<Detection>>,
    result: Option<Detection>,
    selected: Option<Candidate>,
    known: [String; 2],
    zone: Option<u8>,
    message: String,
    road_model: String,
    boundary_model: String,
    ranking: Option<mpsc::Receiver<MatchResult>>,
    rank_result: Option<MatchResult>,
    rank_inputs: Option<RankingInputs>,
    corridor_worker: Option<mpsc::Receiver<SweepEvent>>,
    corridor_result: Option<Result<SweepResult, String>>,
    corridor_progress: Option<SweepStep>,
    downloading_roads: bool,
    online_requested: bool,
    reviewed: Option<Candidate>,
    corridor_units: Option<SourceUnit>,
    corridor_references: Vec<PathBuf>,
}

impl ProjectionCheck {
    pub(super) fn allows_candidate(&self, candidate: Candidate, unit: Option<SourceUnit>) -> bool {
        matches!(&self.result, Some(Detection::Candidates(candidates)) if candidates.contains(&candidate))
            && candidate.matches_filters(unit, self.zone)
    }

    pub(super) fn comparison_candidate(&self, unit: Option<SourceUnit>) -> Option<Candidate> {
        self.selected
            .filter(|candidate| unit.is_some() && self.allows_candidate(*candidate, unit))
    }

    #[cfg(test)]
    pub(super) fn simulate_worker(&mut self) -> mpsc::SyncSender<Detection> {
        let (send, receive) = mpsc::sync_channel(1);
        self.worker = Some(receive);
        send
    }

    pub(super) fn attach(
        &mut self,
        evidence: Option<&Arc<ArchiveEvidence>>,
        unit: Option<SourceUnit>,
    ) {
        if self.evidence.is_none()
            && let Some(evidence) = evidence
        {
            self.evidence = Some(Arc::clone(evidence));
            self.start(unit, None, false);
        }
    }

    fn start(&mut self, unit: Option<SourceUnit>, known: Option<LonLat>, online: bool) {
        let Some(evidence) = self.evidence.clone() else {
            return;
        };
        let (send, receive) = mpsc::sync_channel(1);
        self.result = None;
        self.selected = None;
        self.message.clear();
        self.ranking = None;
        self.rank_result = None;
        self.corridor_worker = None;
        self.corridor_result = None;
        self.online_requested = online;
        self.reviewed = None;
        match std::thread::Builder::new()
            .name("projection-suggestion".into())
            .spawn(move || {
                // No trusted CRS or unit declaration has been verified for the supported archives.
                let result = evaluate(&evidence, Declaration::Absent, unit, known);
                let _ = send.send(result);
            }) {
            Ok(_) => self.worker = Some(receive),
            Err(error) => self.message = format!("Could not start projection detection. {error}"),
        }
    }

    pub(super) fn set_corridor_reference(&mut self, reference: Option<PathBuf>) {
        self.corridor_references = reference.into_iter().collect();
    }

    pub(super) fn poll(
        &mut self,
        ctx: &egui::Context,
        unit: Option<SourceUnit>,
    ) -> Option<Candidate> {
        let units = unit.unwrap_or(SourceUnit::Metres);
        if self
            .corridor_units
            .is_some_and(|previous| previous != units)
        {
            self.corridor_worker = None;
            self.corridor_result = None;
            self.corridor_units = None;
            self.reviewed = None;
        }
        while let Some(worker) = &self.corridor_worker {
            match worker.try_recv() {
                Ok(SweepEvent::LoadingRoads) => self.downloading_roads = true,
                Ok(SweepEvent::Checking(step)) => {
                    self.downloading_roads = false;
                    self.corridor_progress = Some(step);
                }
                Ok(SweepEvent::Finished(result)) => {
                    self.corridor_result = Some(result);
                    self.corridor_worker = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(80));
                    break;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.corridor_result =
                        Some(Err("Road corridor comparison stopped unexpectedly.".into()));
                    self.corridor_worker = None;
                }
            }
        }
        if let Some(worker) = &self.ranking {
            match worker.try_recv() {
                Ok(result) => {
                    self.rank_result = Some(result);
                    self.ranking = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(80))
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.rank_result =
                        Some(Err("Reference comparison stopped unexpectedly.".into()));
                    self.ranking = None;
                }
            }
        }
        match self.worker.as_ref()?.try_recv() {
            Ok(result) => {
                self.worker = None;
                let assigned = match &result {
                    Detection::Assigned(candidate) => Some(*candidate),
                    Detection::Candidates(candidates) => {
                        self.selected = candidates.first().copied();
                        None
                    }
                    Detection::Inconclusive { .. } => None,
                };
                self.result = Some(result);
                if self.online_requested || !self.corridor_references.is_empty() {
                    self.start_corridor(
                        self.corridor_references.clone(),
                        units,
                        self.online_requested,
                    );
                    ctx.request_repaint_after(Duration::from_millis(80));
                }
                assigned
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(40));
                None
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.worker = None;
                self.message = "Projection detection stopped unexpectedly.".into();
                None
            }
        }
    }

    pub(super) fn show(
        &mut self,
        ui: &mut egui::Ui,
        unit: Option<SourceUnit>,
    ) -> Option<Candidate> {
        if ui
            .add_enabled(
                self.worker.is_none() && self.corridor_worker.is_none() && self.evidence.is_some(),
                egui::Button::new("Detect MGA"),
            )
            .clicked()
        {
            self.start(Some(unit.unwrap_or(SourceUnit::Metres)), None, true);
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        }
        ui.label("Checks compatible grids automatically. OpenStreetMap receives candidate area bounds when local coverage is missing.");
        if unit.is_none() {
            ui.label("Assuming metre source coordinates.");
        }
        if let Some(candidate) = self.corridor_comparison(ui, unit.unwrap_or(SourceUnit::Metres)) {
            return Some(candidate);
        }
        ui.collapsing("Manual grid selection", |ui| self.show_manual(ui, unit))
            .body_returned
            .flatten()
    }

    fn show_manual(&mut self, ui: &mut egui::Ui, unit: Option<SourceUnit>) -> Option<Candidate> {
        ui.label("Optional known project location (WGS 84)");
        ui.horizontal_wrapped(|ui| {
            for (value, label) in self.known.iter_mut().zip(["Latitude", "Longitude"]) {
                ui.add(
                    egui::TextEdit::singleline(value)
                        .desired_width(90.0)
                        .hint_text(label),
                );
            }
            if ui
                .add_enabled(
                    self.worker.is_none() && self.evidence.is_some(),
                    egui::Button::new("Check known location"),
                )
                .clicked()
            {
                if self.known.iter().all(|value| value.trim().is_empty()) {
                    self.start(unit, None, false);
                } else {
                    let [lat, lon] = self
                        .known
                        .each_ref()
                        .map(|value| value.trim().parse::<f64>().ok());
                    if let [Some(lat), Some(lon)] = [lat, lon]
                        && let Ok(location) = LonLat::new(lon, lat)
                    {
                        self.start(unit, Some(location), false);
                    } else {
                        self.message = "Enter a valid latitude and longitude in degrees.".into();
                    }
                }
            }
        });
        if !self.message.is_empty() {
            ui.label(&self.message);
        }
        match &self.result {
            Some(Detection::Assigned(candidate)) => {
                ui.label(format!(
                    "Verified declaration · {}",
                    candidate_label(*candidate)
                ));
            }
            Some(Detection::Candidates(candidates)) => {
                egui::ComboBox::from_label("Known MGA zone")
                    .selected_text(
                        self.zone
                            .map_or_else(|| "Any grid".into(), |zone| zone.to_string()),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.zone, None, "Any grid");
                        for zone in 46..=56 {
                            ui.selectable_value(&mut self.zone, Some(zone), zone.to_string());
                        }
                    });
                if self
                    .selected
                    .is_some_and(|candidate| !candidate.matches_filters(unit, self.zone))
                {
                    self.selected = None;
                }
                let candidates = candidates
                    .iter()
                    .copied()
                    .filter(|candidate| candidate.matches_filters(unit, self.zone));
                let count = candidates.clone().count();
                if count == 0 {
                    ui.label("No candidates match the selected units and MGA zone.");
                    return None;
                }
                ui.label(format!("{count} compatible candidates. Confirm the projection and source units before applying."));
                egui::ComboBox::from_id_salt("survey-projection-candidates")
                    .selected_text(
                        self.selected
                            .map_or_else(|| "Choose candidate".into(), candidate_label),
                    )
                    .show_ui(ui, |ui| {
                        for candidate in candidates {
                            ui.selectable_value(
                                &mut self.selected,
                                Some(candidate),
                                candidate_label(candidate),
                            );
                        }
                    });
                if let Some(candidate) = self.selected
                    && ui
                        .add_enabled(
                            unit.is_none_or(|unit| candidate.units == unit),
                            egui::Button::new("Apply candidate"),
                        )
                        .on_disabled_hover_text(
                            "Confirm the matching source units or run detection again.",
                        )
                        .clicked()
                {
                    return Some(candidate);
                }
            }
            Some(Detection::Inconclusive { reason, models }) => {
                ui.label(format!("Inconclusive. {reason}"));
                if !models.is_empty() {
                    ui.label(format!("Conflicting models: {}", models.join(", ")));
                }
            }
            None if self.worker.is_some() => {
                ui.label("Checking source coordinates…");
            }
            None => {}
        }
        None
    }

    fn corridor_comparison(&mut self, ui: &mut egui::Ui, units: SourceUnit) -> Option<Candidate> {
        if self.worker.is_some() {
            ui.label("Checking source coordinates…");
        }
        if self.corridor_worker.is_some() {
            if self.downloading_roads {
                ui.label("Loading OpenStreetMap road vectors…");
            } else if let Some(step) = self.corridor_progress {
                ui.add(
                    egui::ProgressBar::new(step.index as f32 / step.total as f32).text(format!(
                        "Checking {} · {} of {}",
                        crs_label(step.candidate.crs),
                        step.index + 1,
                        step.total
                    )),
                );
            } else {
                ui.label("Pairing survey road edges…");
            }
            if ui.button("Cancel comparison").clicked() {
                self.corridor_worker = None;
                self.corridor_result = None;
            }
        }

        if let Some(result) = &self.corridor_result {
            match result {
                Ok(result) => {
                    if let Some(candidate) = show_corridor_results(ui, result) {
                        self.reviewed = Some(candidate);
                        self.selected = Some(candidate);
                        if !candidate.matches_filters(None, self.zone) {
                            self.zone = None;
                        }
                    }
                }
                Err(error) => {
                    ui.label(format!("Inconclusive. {error}"));
                }
            }
        }
        if let Some(candidate) = self.reviewed {
            ui.label(format!("For review · {}", candidate_label(candidate)));
            if ui.button("Apply reviewed grid").clicked() {
                return Some(candidate);
            }
        }
        if matches!(&self.result, Some(Detection::Candidates(_)))
            && self.corridor_worker.is_none()
            && ui
                .collapsing("Local road references", |ui| {
                    ui.button("Choose local extracts…").clicked()
                })
                .body_returned
                == Some(true)
            && let Some(paths) = rfd::FileDialog::new()
                .add_filter("Road reference JSON", &["geojson", "json"])
                .pick_files()
            && !paths.is_empty()
        {
            self.corridor_references.clone_from(&paths);
            self.start_corridor(paths, units, false);
            ui.ctx().request_repaint_after(Duration::from_millis(80));
        }
        None
    }

    fn start_corridor(&mut self, paths: Vec<PathBuf>, units: SourceUnit, online: bool) {
        let (Some(Detection::Candidates(candidates)), Some(evidence)) =
            (&self.result, &self.evidence)
        else {
            return;
        };
        let candidates = candidates.clone();
        let evidence = Arc::clone(evidence);
        let (send, receive) = mpsc::channel();
        self.corridor_units = Some(units);
        self.corridor_result = None;
        self.corridor_progress = None;
        self.downloading_roads = false;
        self.reviewed = None;
        let spawned = std::thread::Builder::new()
            .name("road-corridor-comparison".into())
            .spawn(move || {
                let result = (|| {
                    let mut references = Vec::new();
                    let mut reference_warning = None;
                    for path in paths {
                        match corridor_match::load(&path) {
                            Ok(reference) => references.push(reference),
                            Err(error) if online => reference_warning = Some(error),
                            Err(error) => return Err(error),
                        }
                    }
                    let sweep = Sweep::prepare(&candidates, units, &evidence.road_sides)?;
                    if online {
                        let boxes = sweep.missing_boxes(&references);
                        if !boxes.is_empty() && send.send(SweepEvent::LoadingRoads).is_ok() {
                            match super::road_reference::fetch(&boxes) {
                                Ok(loaded) => references.extend(loaded),
                                Err(error) => reference_warning = Some(error),
                            }
                        }
                    }
                    let assessments = sweep.assess(&references, |step| {
                        send.send(SweepEvent::Checking(step)).is_ok()
                    })?;
                    Ok(SweepResult {
                        assessments,
                        reference_warning,
                    })
                })();
                let _ = send.send(SweepEvent::Finished(result));
            });
        match spawned {
            Ok(_) => self.corridor_worker = Some(receive),
            Err(error) => self.corridor_result = Some(Err(error.to_string())),
        }
    }

    pub(super) fn local_comparison(
        &mut self,
        ui: &mut egui::Ui,
        unit: Option<SourceUnit>,
        preview: Option<(&[PreviewRecord], u64)>,
    ) {
        let (Some(Detection::Candidates(candidates)), Some(unit), Some((preview, revision))) =
            (&self.result, unit, preview)
        else {
            ui.label("Confirm source units and wait for the selected survey models before comparing local vectors.");
            return;
        };
        let models: BTreeSet<_> = preview
            .iter()
            .filter(|record| record.kind == Kind::Polyline && record.points.len() > 1)
            .map(|record| record.model.as_str())
            .collect();
        if models.is_empty() {
            ui.label("Select a survey model with comparable road linework.");
            return;
        }
        for (id, label, value) in [
            (
                "comparison-road-model",
                "Road centreline model",
                &mut self.road_model,
            ),
            (
                "comparison-boundary-model",
                "Boundary model (optional)",
                &mut self.boundary_model,
            ),
        ] {
            egui::ComboBox::from_id_salt(id)
                .selected_text(if value.is_empty() {
                    label
                } else {
                    value.as_str()
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(value, String::new(), "None");
                    for &model in &models {
                        ui.selectable_value(value, model.to_owned(), model);
                    }
                });
        }
        let same_model = !self.boundary_model.is_empty() && self.road_model == self.boundary_model;
        let inputs = RankingInputs {
            revision,
            unit,
            zone: self.zone,
            road_model: self.road_model.clone(),
            boundary_model: self.boundary_model.clone(),
        };
        if self.rank_inputs.as_ref() != Some(&inputs) {
            self.ranking = None;
            self.rank_result = None;
            self.rank_inputs = Some(inputs);
        }
        if same_model {
            ui.label("Choose a different model for independent boundaries.");
        }
        let ready = self.ranking.is_none()
            && !same_model
            && models.contains(self.road_model.as_str())
            && (self.boundary_model.is_empty() || models.contains(self.boundary_model.as_str()))
            && candidates
                .iter()
                .any(|candidate| candidate.matches_filters(Some(unit), self.zone));
        if ui
            .add_enabled(
                ready,
                egui::Button::new("Compare local OpenStreetMap GeoJSON…"),
            )
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("GeoJSON", &["geojson", "json"])
                .pick_file()
        {
            let candidates: Vec<_> = candidates
                .iter()
                .filter(|candidate| candidate.matches_filters(Some(unit), self.zone))
                .copied()
                .collect();
            let roads = sample_segments(preview, &self.road_model);
            let boundaries = sample_segments(preview, &self.boundary_model);
            let (send, receive) = mpsc::sync_channel(1);
            self.rank_result = None;
            let spawned = std::thread::Builder::new()
                .name("openstreetmap-reference".into())
                .spawn(move || {
                    let outcome = road_match::load(&path).and_then(|reference| {
                        road_match::recommend(&candidates, &roads, &boundaries, &reference)
                            .map(|result| (result, reference.date))
                            .map_err(str::to_owned)
                    });
                    let _ = send.send(outcome);
                });
            match spawned {
                Ok(_) => self.ranking = Some(receive),
                Err(error) => self.rank_result = Some(Err(error.to_string())),
            }
        }
        if self.ranking.is_some() {
            ui.label("Comparing local reference vectors…");
        }
        match &self.rank_result {
            Some(Ok((result, date))) => {
                let grid = match result.grid {
                    Grid::Mga(zone) => format!("MGA zone {zone}"),
                    Grid::Brisbane => "Brisbane City Survey Grid".into(),
                };
                ui.label(format!("Likely {grid} · {} of {} road probes matched. Confirm the exact grid and datum.", result.road_matches, result.road_probes));
                if result.boundary_matches > 0 {
                    ui.label(format!(
                        "{} approximate boundary matches.",
                        result.boundary_matches
                    ));
                }
                ui.hyperlink_to(
                    format!("© OpenStreetMap contributors · extract {date}"),
                    "https://www.openstreetmap.org/copyright",
                );
            }
            Some(Err(error)) => {
                ui.label(format!("Inconclusive. {error}"));
            }
            None => {}
        }
    }
}

fn show_corridor_results(ui: &mut egui::Ui, result: &SweepResult) -> Option<Candidate> {
    let results = &result.assessments;
    let mut selected = None;
    let ranked = corridor_match::ranked_grids(results);
    if ranked.is_empty() {
        ui.label("No grid had positive measured road-corridor agreement.");
    } else {
        ui.strong("Best grids");
    }
    for grid in ranked.iter().take(3) {
        let name = match grid.grid {
            Grid::Mga(zone) => format!("MGA zone {zone}"),
            Grid::Brisbane => "Brisbane City Survey Grid".into(),
        };
        let Some((_, best)) = grid.variants.first() else {
            continue;
        };
        let status = if best.matches() { "Match" } else { "Review" };
        ui.collapsing(
            format!("{name} · {status} · {:.1}%", best.pooled_fraction() * 100.0),
            |ui| {
                for (variant, fit) in &grid.variants {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "{} · {:.1}%",
                            crs_label(variant.candidate.crs),
                            fit.pooled_fraction() * 100.0
                        ));
                        if ui
                            .push_id(variant.candidate.crs.epsg(), |ui| ui.button("Review grid"))
                            .inner
                            .clicked()
                        {
                            selected = Some(variant.candidate);
                        }
                    });
                }
            },
        );
    }
    let measured = results
        .iter()
        .filter(|assessment| matches!(assessment.status, Status::Measured { .. }))
        .count();
    let unchecked = results
        .iter()
        .filter(|assessment| matches!(assessment.status, Status::Unchecked))
        .count();
    ui.label(format!(
        "{} candidates checked · {measured} scored · {unchecked} without coverage",
        results.len()
    ));
    if let Some(warning) = &result.reference_warning {
        ui.label(warning);
    }
    ui.collapsing("Road-corridor results by grid and model", |ui| {
        for result in results {
            let label = crs_label(result.candidate.crs);
            match &result.status {
                Status::Unchecked => {
                    ui.label(format!("{label} · Unchecked"));
                }
                Status::Inconclusive(reason) => {
                    ui.label(format!("{label} · Inconclusive. {reason}"));
                }
                Status::Measured {
                    coverage,
                    by_model,
                    date,
                } => {
                    ui.collapsing(
                        format!("{} · {:.1}%", label, coverage.pooled_fraction() * 100.0),
                        |ui| {
                            ui.label(format!(
                                "Centreline {:.1}% of {:.0} m · Edges {:.1}% of {:.0} m",
                                coverage.centre_fraction() * 100.0,
                                coverage.design_centre_m,
                                coverage.edge_fraction() * 100.0,
                                coverage.design_edge_m
                            ));
                            for (name, coverage) in by_model {
                                ui.label(format!(
                                    "{name} · {:.1}%",
                                    coverage.pooled_fraction() * 100.0
                                ));
                            }
                            ui.hyperlink_to(
                                format!("© OpenStreetMap contributors · extract {date}"),
                                "https://www.openstreetmap.org/copyright",
                            );
                        },
                    );
                }
            }
        }
    });
    selected
}

pub(super) fn sample_segments(preview: &[PreviewRecord], model: &str) -> Vec<Segment> {
    let segments: Vec<_> = preview
        .iter()
        .filter(|p| p.model == model && p.kind == Kind::Polyline)
        .flat_map(|record| record.points.array_windows().copied())
        .filter(|[a, b]| a != b)
        .collect();
    let step = segments.len().div_ceil(96).max(1);
    segments.into_iter().step_by(step).collect()
}

fn candidate_label(candidate: Candidate) -> String {
    format!("{} · {}", crs_label(candidate.crs), candidate.units.label())
}

#[cfg(test)]
mod tests {
    use super::*;
    use az12_converter::map::CrsId;

    #[test]
    fn companion_reference_starts_in_background_without_assigning_a_projection() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("archive.12daz.osm-roads.geojson");
        std::fs::write(&path, "invalid JSON").unwrap();
        let mut check = ProjectionCheck {
            evidence: Some(Arc::new(ArchiveEvidence::default())),
            ..ProjectionCheck::default()
        };
        check.set_corridor_reference(Some(path));
        let candidate = Candidate {
            crs: CrsId::Gda2020MgaZone54,
            units: SourceUnit::Metres,
        };
        let sender = check.simulate_worker();
        sender.send(Detection::Candidates(vec![candidate])).unwrap();
        let ctx = egui::Context::default();
        assert!(check.poll(&ctx, None).is_none());
        let receiver = check.corridor_worker.take().unwrap();
        let result = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            matches!(result, SweepEvent::Finished(Err(error)) if error.contains("Invalid reference JSON"))
        );
    }
}
