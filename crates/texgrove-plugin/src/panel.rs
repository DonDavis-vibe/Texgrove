//! The panel: which models, which texture, how and where; a 3D view of the result; and the
//! button that makes the files.

use std::cell::RefCell;

use meshgrove_plugin_api::guest::{self, GuestPanel, MeshData, library};
use meshgrove_plugin_api::ui::{
    Choice, Effect, Event, FormRow, JobSpec, Level, Node, TableColumn, TableRow, TextStyle, Unit, View,
};
use meshgrove_plugin_api::{Bundle, BundleFile, Derive3mf, HostInfo, PartMesh};
use serde::{Deserialize, Serialize};

use crate::lang::{self, tr};
use crate::recipe::{Format, MappingChoice, Place, QualityChoice, Recipe, ReliefChoice};
use crate::tf;
use crate::work::{self, ExportJob, Exported, PreviewJob, Source};

/// What the panel holds. What is not marked to be skipped comes back after a restart.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelState {
    pub sources: Vec<Source>,
    /// The source shown in the 3D view.
    pub selected: usize,
    pub recipe: Recipe,
    /// Show the model as it is instead of textured.
    pub plain: bool,
    /// The gallery group shown.
    pub group: String,
    /// What the user typed into the search field, and what it found.
    pub search: String,
    #[serde(skip)]
    results: Vec<Source>,
    #[serde(skip)]
    view: ViewState,
    #[serde(skip)]
    batch: Batch,
    #[serde(skip)]
    message: Option<(Level, String)>,
    #[serde(skip)]
    host: Host,
}

/// What the 3D view shows and the preview being made.
#[derive(Debug, Default)]
struct ViewState {
    /// The preview file shown (below `/data/preview`), and how often it changed.
    shown: Option<String>,
    revision: u64,
    /// The preview job running: its key, and how far it is.
    running: Option<(String, f32, String)>,
    /// Why the last preview failed.
    failed: Option<String>,
    /// What the last preview had to say about the result.
    warnings: Vec<String>,
}

/// Making the files: what is left, what is done, what failed.
#[derive(Debug, Default)]
struct Batch {
    /// Sources still to do, in order.
    queue: Vec<usize>,
    /// The one being made, with its progress.
    current: Option<(usize, f32, String)>,
    /// Done but not yet handed over: the source and what came of it.
    done: Vec<(usize, Exported)>,
    failed: Vec<(usize, String)>,
    /// What the finished files had to say.
    warned: Vec<(usize, String)>,
    /// Files handed over so far in this batch.
    handed: usize,
}

#[derive(Debug, Clone)]
struct Host {
    nozzle: f32,
    bed: Option<[f32; 2]>,
}

impl Default for Host {
    fn default() -> Self {
        Host { nozzle: 0.4, bed: None }
    }
}

pub struct TexturePanel {
    state: RefCell<PanelState>,
}

impl TexturePanel {
    fn effects(&self, event: Event) -> Vec<Effect> {
        let mut s = self.state.borrow_mut();
        let mut effects = Vec::new();
        let mut changed = false;
        match event {
            Event::HostChanged { info } => {
                s.host_changed(&info);
                changed = true;
            }
            Event::ModelsOpened { ids } => {
                let first = s.sources.len();
                for id in ids {
                    s.add(Source { name: file_name(&id), reference: id, ..Source::default() });
                }
                s.selected = first.min(s.sources.len().saturating_sub(1));
                changed = true;
            }
            Event::FilePicked { id, name, path } if id == "picture" => match crate::textures::add(&path, &name) {
                Ok(texture) => {
                    s.recipe.texture = texture;
                    s.group = "own".into();
                    changed = true;
                }
                Err(e) => s.message = Some((Level::Error, e)),
            },
            Event::FilePicked { name, path, .. } => {
                s.add(Source { reference: path, name, ..Source::default() });
                s.selected = s.sources.len() - 1;
                changed = true;
            }
            Event::Click { id } => {
                if let Some(texture) = id.strip_prefix("texture:") {
                    s.recipe.texture = texture.to_owned();
                    changed = true;
                }
                match id.as_str() {
                    "pick" => effects.push(Effect::OpenFileDialog {
                        id: "model".into(),
                        title: Some(tr("Modell wählen", "Choose a model").into()),
                        extensions: ["stl", "3mf", "obj", "ply", "step", "stp", "glb", "gltf"]
                            .map(String::from)
                            .to_vec(),
                        multiple: true,
                        into: meshgrove_plugin_api::ui::PickInto::Data,
                    }),
                    "remove" if s.selected < s.sources.len() && s.batch.idle() => {
                        let at = s.selected;
                        s.sources.remove(at);
                        s.selected = s.selected.min(s.sources.len().saturating_sub(1));
                        changed = true;
                    }
                    "own" => effects.push(Effect::OpenFileDialog {
                        id: "picture".into(),
                        title: Some(
                            tr("Bild als Textur wählen (hell = hoch)", "Choose a picture as texture (bright = high)")
                                .into(),
                        ),
                        extensions: ["png", "jpg", "jpeg"].map(String::from).to_vec(),
                        multiple: false,
                        into: meshgrove_plugin_api::ui::PickInto::Data,
                    }),
                    "forget" if s.recipe.texture.starts_with(crate::textures::OWN) => {
                        if let Err(e) = crate::textures::remove(&s.recipe.texture) {
                            s.message = Some((Level::Error, e));
                        }
                        s.recipe.texture = "knurl".into();
                        changed = true;
                    }
                    "plain" => {
                        s.plain = !s.plain;
                        changed = true;
                    }
                    "apply" => effects.extend(s.start_batch()),
                    "cancel" => {
                        if let Some((i, ..)) = s.batch.current {
                            effects.push(Effect::CancelJob { job: format!("export:{i}") });
                        }
                        s.batch = Batch::default();
                        s.message = Some((Level::Info, tr("Abgebrochen", "Cancelled").into()));
                    }
                    _ => {}
                }
            }
            Event::TableSelect { id, row: Some(row) } => match id.as_str() {
                "sources" => {
                    if let Ok(i) = row.parse::<usize>()
                        && i < s.sources.len()
                    {
                        s.selected = i;
                        changed = true;
                    }
                }
                "results" => {
                    if let Some(found) = row.parse::<usize>().ok().and_then(|i| s.results.get(i).cloned()) {
                        s.add(found);
                        s.selected = s.sources.len() - 1;
                        changed = true;
                    }
                }
                _ => {}
            },
            Event::Changed { id, value } => {
                changed = s.change(&id, &value);
            }
            Event::JobProgress { job, fraction, message } => {
                if job.starts_with("preview:") {
                    if let Some(running) = &mut s.view.running {
                        (running.1, running.2) = (fraction, message);
                    }
                } else if let Some(current) = &mut s.batch.current {
                    (current.1, current.2) = (fraction, message);
                }
            }
            Event::JobFinished { job, result, error } => {
                if let Some(key) = job.strip_prefix("preview:") {
                    s.preview_finished(key, result, error);
                    changed = true;
                } else if job.starts_with("export:") {
                    effects.extend(s.export_finished(result, error));
                }
            }
            Event::BundleFinished { outcome, .. } => {
                use meshgrove_plugin_api::ui::BundleOutcome;
                s.message = Some(match outcome {
                    BundleOutcome::Applied { .. } => (
                        Level::Success,
                        tf!("{} Datei(en) liegen in der Bibliothek", "{} file(s) are in the library", s.batch.handed),
                    ),
                    BundleOutcome::Queued => (Level::Info, tr("Wartet im Review", "Waiting in the review").into()),
                    BundleOutcome::Refused => (
                        Level::Error,
                        tr(
                            "Texgrove darf keine Dateien in die Bibliothek legen (Einstellungen → Plugins)",
                            "Texgrove may not put files into the library (Settings → Plugins)",
                        )
                        .into(),
                    ),
                    BundleOutcome::Failed { error } => (Level::Error, error),
                });
            }
            _ => {}
        }
        if changed {
            effects.extend(s.preview_effects());
        }
        effects
    }
}

impl PanelState {
    fn host_changed(&mut self, info: &HostInfo) {
        lang::set(&info.language);
        let printer = info.printers.first();
        self.host = Host {
            nozzle: printer.and_then(|p| p.nozzle).filter(|n| *n > 0.0).unwrap_or(0.4),
            bed: printer.map(|p| [p.bed[0], p.bed[1]]),
        };
    }

    /// Adds a model unless it is there already; a 3MF with the objects it is made of.
    fn add(&mut self, mut source: Source) {
        if let Some(i) = self.sources.iter().position(|s| s.reference == source.reference) {
            self.selected = i;
        } else {
            source.find_parts();
            self.sources.push(source);
        }
    }

    /// Takes a changed setting; whether the preview has to follow.
    fn change(&mut self, id: &str, value: &serde_json::Value) -> bool {
        if let Some(index) = id.strip_prefix("part:").and_then(|i| i.parse::<usize>().ok()) {
            let Some(source) = self.sources.get_mut(self.selected) else { return false };
            let Some(token) = source.parts.get(index).map(|p| p.token.clone()) else { return false };
            source.skipped.retain(|t| *t != token);
            if value.as_bool() == Some(false) {
                source.skipped.push(token);
            }
            return true;
        }
        let number = value.as_f64().map(|v| v as f32).filter(|v| v.is_finite());
        let text = value.as_str().unwrap_or_default();
        let r = &mut self.recipe;
        match (id, number) {
            ("tile", Some(v)) => r.tile = v.clamp(0.2, 200.0),
            ("depth", Some(v)) => r.depth = v.clamp(0.0, 20.0),
            ("rotation", Some(v)) => r.rotation = v.clamp(-360.0, 360.0),
            ("blend", Some(v)) => r.blend = v.clamp(0.0, 100.0),
            ("smooth", Some(v)) => r.smooth = v.clamp(0.0, 25.0),
            ("bottom", Some(v)) => r.bottom = v.clamp(0.0, 90.0),
            ("top", Some(v)) => r.top = v.clamp(0.0, 90.0),
            ("mapping", _) => r.mapping = MappingChoice::from_id(text).unwrap_or(r.mapping),
            ("relief", _) => r.relief = ReliefChoice::from_id(text).unwrap_or(r.relief),
            ("place", _) => r.place = Place::from_id(text).unwrap_or(r.place),
            ("invert", _) => r.invert = value.as_bool().unwrap_or(r.invert),
            ("quality", _) => {
                r.quality = QualityChoice::from_id(text).unwrap_or(r.quality);
                return false; // only the file to keep depends on it
            }
            ("format", _) => {
                r.format = if text == "stl" { Format::Stl } else { Format::ThreeMf };
                return false;
            }
            ("group", _) => {
                self.group = text.to_owned();
                return false;
            }
            ("search", _) => {
                self.search = text.trim().to_owned();
                self.run_search();
                return false;
            }
            _ => return false,
        }
        true
    }

    fn run_search(&mut self) {
        self.results.clear();
        if self.search.is_empty() {
            return;
        }
        match library::search(&self.search, 30) {
            Ok(json) => {
                let value: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
                self.results = hits(&value);
                if self.results.is_empty() {
                    self.message = Some((Level::Info, tr("Nichts gefunden", "Nothing found").into()));
                }
            }
            Err(e) => self.message = Some((Level::Warn, e)),
        }
    }

    /// Shows the preview for what is set, from the cache or by starting a job for it.
    fn preview_effects(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let Some(source) = self.sources.get(self.selected).cloned() else {
            self.view.shown = None;
            return effects;
        };
        let plain = format!("p-{}", work::cache_key(&source.reference));
        let look = Recipe { quality: QualityChoice::Normal, format: Format::ThreeMf, ..self.recipe.clone() };
        // The parts left out change the look as well.
        let key = work::cache_key(&(
            &source.reference,
            &source.skipped,
            serde_json::to_string(&look).unwrap_or_default(),
            self.host.nozzle.to_bits(),
        ));
        let textured = format!("t-{key}");
        let wanted = if self.plain { &plain } else { &textured };
        if work::exists(&textured) && work::exists(&plain) {
            if let Some((running, ..)) = self.view.running.take() {
                effects.push(Effect::CancelJob { job: format!("preview:{running}") });
            }
            self.show(wanted.clone());
            return effects;
        }
        // Meanwhile the model as it is, if that is there.
        if work::exists(&plain) {
            self.show(plain.clone());
        }
        if self.view.running.as_ref().is_some_and(|(running, ..)| *running == key) {
            return effects;
        }
        if let Some((running, ..)) = self.view.running.take() {
            effects.push(Effect::CancelJob { job: format!("preview:{running}") });
        }
        let job = PreviewJob { source, recipe: look, nozzle: self.host.nozzle, plain, textured };
        self.view.running = Some((key.clone(), 0.0, String::new()));
        self.view.failed = None;
        effects.push(Effect::StartJobs {
            jobs: vec![JobSpec {
                id: format!("preview:{key}"),
                kind: "preview".into(),
                params: serde_json::to_value(&job).unwrap_or_default(),
                label: Some(tr("Vorschau", "Preview").into()),
            }],
        });
        effects
    }

    fn show(&mut self, file: String) {
        if self.view.shown.as_ref() != Some(&file) {
            self.view.shown = Some(file);
            self.view.revision += 1;
        }
    }

    fn preview_finished(&mut self, key: &str, result: Option<serde_json::Value>, error: Option<String>) {
        if self.view.running.as_ref().is_some_and(|(running, ..)| running == key) {
            self.view.running = None;
            self.view.warnings = result
                .and_then(|r| r.get("warnings").cloned())
                .and_then(|w| serde_json::from_value(w).ok())
                .unwrap_or_default();
            if let Some(error) = error.filter(|e| !e.contains("cancel")) {
                self.view.failed = Some(error);
            }
        }
    }

    fn start_batch(&mut self) -> Vec<Effect> {
        if self.sources.is_empty() {
            self.message = Some((Level::Warn, tr("Erst ein Modell wählen", "Choose a model first").into()));
            return Vec::new();
        }
        if !self.batch.idle() {
            return Vec::new();
        }
        // Library by library, so each lands as one step in the history: the files a bundle
        // takes are all that is in the staging folder.
        let mut order: Vec<usize> = (0..self.sources.len()).collect();
        order.sort_by_key(|&i| (self.sources[i].library().map(str::to_owned), i));
        self.batch = Batch { queue: order, ..Batch::default() };
        self.message = None;
        self.next_export()
    }

    /// Starts the next file, or hands over what is done when a library's turn is over.
    fn next_export(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let library_of = |s: &PanelState, i: usize| s.sources[i].library().map(str::to_owned);
        let next = self.batch.queue.first().copied();
        let turn_over = match (next, self.batch.done.last()) {
            (_, None) => false,
            (None, Some(_)) => true,
            (Some(next), Some((last, _))) => library_of(self, next) != library_of(self, *last),
        };
        if turn_over {
            effects.extend(self.hand_over());
        }
        let Some(next) = next else {
            self.batch.current = None;
            if !self.batch.warned.is_empty() {
                let lines: Vec<String> =
                    self.batch.warned.iter().map(|(i, w)| format!("{}: {w}", self.sources[*i].name)).collect();
                self.message = Some((Level::Warn, lines.join("\n")));
            }
            if !self.batch.failed.is_empty() {
                let names: Vec<String> =
                    self.batch.failed.iter().map(|(i, e)| format!("{}: {e}", self.sources[*i].name)).collect();
                self.message = Some((Level::Error, names.join("\n")));
            }
            return effects;
        };
        self.batch.queue.remove(0);
        let source = self.sources[next].clone();
        let job =
            ExportJob { file: self.file_name(next), source, recipe: self.recipe.clone(), nozzle: self.host.nozzle };
        self.batch.current = Some((next, 0.0, String::new()));
        effects.push(Effect::StartJobs {
            jobs: vec![JobSpec {
                id: format!("export:{next}"),
                kind: "export".into(),
                params: serde_json::to_value(&job).unwrap_or_default(),
                label: Some(self.sources[next].name.clone()),
            }],
        });
        effects
    }

    /// The new file's name: the model's, the texture's, the format's ending. Two models of the
    /// same name in one batch get a number.
    fn file_name(&self, i: usize) -> String {
        let source = &self.sources[i];
        let same = self.sources[..i].iter().filter(|s| s.stem() == source.stem()).count();
        let number = if same > 0 { format!(" {}", same + 1) } else { String::new() };
        let texture = crate::textures::name(&self.recipe.texture);
        let name = format!("{}{number} – {texture}.{}", source.stem(), self.recipe.format.extension());
        name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
    }

    fn export_finished(&mut self, result: Option<serde_json::Value>, error: Option<String>) -> Vec<Effect> {
        let Some((i, ..)) = self.batch.current.take() else { return Vec::new() };
        match (result.and_then(|r| serde_json::from_value::<Exported>(r).ok()), error) {
            (Some(done), _) => {
                for warning in &done.warnings {
                    self.batch.warned.push((i, warning.clone()));
                }
                self.batch.done.push((i, done));
            }
            (None, Some(error)) => self.batch.failed.push((i, error)),
            (None, None) => self.batch.failed.push((i, tr("keine Antwort", "no answer").into())),
        }
        self.next_export()
    }

    /// The files made so far, as one bundle.
    fn hand_over(&mut self) -> Vec<Effect> {
        let done = std::mem::take(&mut self.batch.done);
        let Some((first, _)) = done.first() else { return Vec::new() };
        let in_library = self.sources[*first].in_library();
        let tag = tr("Texturiert", "Textured").to_owned();
        let files = done
            .iter()
            .map(|(i, made)| {
                let source = &self.sources[*i];
                let note = tf!(
                    "Texgrove {}: {}. Aus {}, {} Dreiecke.",
                    "Texgrove {}: {}. Made from {}, {} triangles.",
                    env!("CARGO_PKG_VERSION"),
                    self.recipe.describe(),
                    source.name,
                    lang::count(made.triangles),
                );
                // A 3MF whose objects were textured one by one: Meshgrove copies it and swaps them.
                let derive_3mf = (!made.replace.is_empty()).then(|| Derive3mf {
                    source: source.reference.clone(),
                    replace: made
                        .replace
                        .iter()
                        .map(|(part, mesh)| PartMesh { part: part.clone(), mesh: mesh.clone() })
                        .collect(),
                });
                BundleFile {
                    path: made.file.clone(),
                    note: Some(note),
                    next_to: in_library.then(|| source.reference.clone()),
                    derive_3mf,
                    ..BundleFile::default()
                }
            })
            .collect::<Vec<_>>();
        self.batch.handed += files.len();
        let texture = crate::textures::name(&self.recipe.texture);
        let title = match &done[..] {
            [(i, _)] => format!("{} – {texture}", self.sources[*i].stem()),
            more => tf!("{} auf {} Modellen", "{} on {} models", texture, more.len()),
        };
        vec![Effect::ApplyBundle {
            bundle: Bundle {
                title,
                folder: (!in_library).then(|| "Texgrove".to_owned()),
                files,
                tags: vec![tag],
                ..Bundle::default()
            },
        }]
    }
}

impl Batch {
    fn idle(&self) -> bool {
        self.current.is_none() && self.queue.is_empty()
    }
}

/// The last part of an id.
fn file_name(id: &str) -> String {
    id.rsplit('/').next().unwrap_or(id).to_owned()
}

/// The models in a search's answer: the first list of things with an id in it.
fn hits(value: &serde_json::Value) -> Vec<Source> {
    let list = match value {
        serde_json::Value::Array(list) => Some(list),
        serde_json::Value::Object(map) => {
            map.values().find_map(|v| v.as_array().filter(|a| a.iter().any(|x| x.get("id").is_some())))
        }
        _ => None,
    };
    list.into_iter()
        .flatten()
        .filter_map(|hit| {
            let id = hit.get("id")?.as_str()?.to_owned();
            let name = hit.get("name").and_then(|n| n.as_str()).map_or_else(|| file_name(&id), str::to_owned);
            Some(Source { reference: id, name, ..Source::default() })
        })
        .collect()
}

// ---- Drawing ----

fn choices<T: Copy>(all: &[T], id: impl Fn(T) -> &'static str, label: impl Fn(T) -> &'static str) -> Vec<Choice> {
    all.iter().map(|&x| Choice { value: id(x).into(), label: label(x).into() }).collect()
}

fn number(id: &str, value: f32, min: f64, max: f64, step: f64, unit: Unit, decimals: u8) -> Node {
    Node::NumberInput {
        id: id.into(),
        value: f64::from(value),
        min: Some(min),
        max: Some(max),
        step: Some(step),
        unit,
        decimals: Some(decimals),
        enabled: true,
    }
}

impl PanelState {
    fn render(&self) -> Node {
        Node::Split {
            vertical: false,
            ratio: 0.6,
            first: Box::new(self.view_part()),
            second: Box::new(self.settings_part()),
        }
    }

    fn view_part(&self) -> Node {
        let mut children = Vec::new();
        match (&self.view.shown, self.sources.get(self.selected)) {
            (_, None) => children.push(Node::Notice {
                level: Level::Info,
                text: tr(
                    "Ein Modell in der Galerie rechts anklicken und „Textur auftragen …“ wählen, oder hier eine Datei wählen.",
                    "Right-click a model in the gallery and choose “Apply texture …”, or choose a file here.",
                )
                .into(),
            }),
            (Some(shown), Some(_)) => children.push(Node::MeshView {
                id: "view".into(),
                key: shown.clone(),
                revision: self.view.revision,
                height: Some(460.0),
                bed: self.host.bed,
                frame: None,
                paint: None,
                overlay: None,
            }),
            (None, Some(_)) => children.push(Node::Progress { fraction: None, text: Some(tr("Modell wird geladen …", "Loading the model …").into()) }),
        }
        if let Some((_, fraction, message)) = &self.view.running {
            children.push(Node::Progress {
                fraction: Some(*fraction),
                text: Some(tf!("Vorschau: {}", "Preview: {}", message)),
            });
        }
        if let Some(error) = &self.view.failed {
            children.push(Node::Notice { level: Level::Error, text: error.clone() });
        }
        for warning in &self.view.warnings {
            children.push(Node::Notice { level: Level::Warn, text: warning.clone() });
        }
        if !self.sources.is_empty() {
            let label = if self.plain {
                tr("Mit Textur zeigen", "Show with texture")
            } else {
                tr("Ohne Textur zeigen", "Show without texture")
            };
            children.push(Node::row(vec![
                Node::Button { id: "plain".into(), label: label.into(), primary: false, enabled: true },
                Node::Label {
                    text: tr(
                        "Die Vorschau ist gröber als die fertige Datei.",
                        "The preview is coarser than the finished file.",
                    )
                    .into(),
                    style: TextStyle::Weak,
                },
            ]));
        }
        Node::column(children)
    }

    fn settings_part(&self) -> Node {
        let r = &self.recipe;
        let row = |label: &str, node: Node| FormRow { label: label.into(), node };
        let mut children = vec![Node::heading(tr("Modelle", "Models"))];
        if !self.sources.is_empty() {
            children.push(Node::Table {
                id: "sources".into(),
                columns: vec![TableColumn { title: tr("Datei", "File").into(), width: None, numeric: false }],
                rows: self
                    .sources
                    .iter()
                    .enumerate()
                    .map(|(i, s)| TableRow { id: i.to_string(), cells: vec![s.name.clone()] })
                    .collect(),
                selected: Some(self.selected.to_string()),
                max_height: Some(120.0),
            });
        }
        children.push(Node::row(vec![
            Node::Button {
                id: "pick".into(),
                label: tr("Datei wählen …", "Choose a file …").into(),
                primary: false,
                enabled: true,
            },
            Node::Button {
                id: "remove".into(),
                label: tr("Entfernen", "Remove").into(),
                primary: false,
                enabled: !self.sources.is_empty() && self.batch.idle(),
            },
        ]));
        children.push(Node::TextInput {
            id: "search".into(),
            value: self.search.clone(),
            hint: Some(tr("In der Bibliothek suchen (Enter)", "Search the library (Enter)").into()),
            multiline: false,
            enabled: true,
        });
        if !self.results.is_empty() {
            children.push(Node::Table {
                id: "results".into(),
                columns: vec![TableColumn {
                    title: tr("Gefunden – anklicken zum Hinzufügen", "Found – click to add").into(),
                    width: None,
                    numeric: false,
                }],
                rows: self
                    .results
                    .iter()
                    .enumerate()
                    .map(|(i, s)| TableRow { id: i.to_string(), cells: vec![s.reference.clone()] })
                    .collect(),
                selected: None,
                max_height: Some(140.0),
            });
        }

        if let Some(source) = self.sources.get(self.selected).filter(|s| s.parts.len() > 1) {
            let mut boxes = vec![Node::Label {
                text: tr(
                    "Nur die angehakten Teile bekommen die Textur; als 3MF bleibt der Rest der Datei, wie er ist.",
                    "Only the ticked parts get the texture; as 3MF the rest of the file stays as it is.",
                )
                .into(),
                style: TextStyle::Small,
            }];
            boxes.extend(source.parts.iter().enumerate().map(|(i, part)| Node::Checkbox {
                id: format!("part:{i}"),
                label: tf!("{} ({} Dreiecke)", "{} ({} triangles)", part.name, lang::count(part.triangles as usize)),
                checked: !source.skipped.contains(&part.token),
                enabled: true,
            }));
            children.push(Node::Group { title: Some(tr("Teile", "Parts").into()), collapsible: true, children: boxes });
        }

        children.push(Node::Separator);
        children.push(Node::heading(tr("Textur", "Texture")));
        use crate::textures::Group;
        let group = if self.group.is_empty() { Group::of(&r.texture) } else { Group::from_id(&self.group) };
        children.push(Node::Select {
            id: "group".into(),
            options: Group::ALL.iter().map(|g| Choice { value: g.id().into(), label: g.label().into() }).collect(),
            selected: group.id().into(),
            enabled: true,
        });
        let tiles: Vec<Node> = group
            .textures()
            .iter()
            .filter_map(|(id, name)| {
                Some(Node::Image {
                    data: crate::thumbs::of(id)?,
                    mime: "image/png".into(),
                    width: Some(72.0),
                    alt: Some(name.clone()),
                    id: Some(format!("texture:{id}")),
                    selected: r.texture == *id,
                })
            })
            .collect();
        if tiles.is_empty() && group == Group::Own {
            children.push(Node::Label {
                text: tr(
                    "Noch keine eigenen Bilder: „Eigenes Bild …“ wählt eins.",
                    "No pictures of your own yet: “Own picture …” picks one.",
                )
                .into(),
                style: TextStyle::Weak,
            });
        }
        for line in tiles.chunks(5) {
            children.push(Node::row(line.to_vec()));
        }
        let own = r.texture.starts_with(crate::textures::OWN);
        let credit = crate::textures::credit(&r.texture);
        children.push(Node::row(vec![
            Node::Label { text: crate::textures::name(&r.texture), style: TextStyle::Strong },
            Node::Button {
                id: "own".into(),
                label: tr("Eigenes Bild …", "Own picture …").into(),
                primary: false,
                enabled: true,
            },
            Node::Button { id: "forget".into(), label: tr("Entfernen", "Remove").into(), primary: false, enabled: own },
        ]));
        if let Some(credit) = credit {
            children.push(Node::Label { text: credit.into(), style: TextStyle::Small });
        }

        children.push(Node::Form {
            id: "how".into(),
            rows: vec![
                row(
                    tr("Auftragen", "Laid on"),
                    Node::Select {
                        id: "mapping".into(),
                        options: choices(&MappingChoice::ALL, MappingChoice::id, MappingChoice::label),
                        selected: r.mapping.id().into(),
                        enabled: true,
                    },
                ),
                row(tr("Kachelgröße", "Tile size"), number("tile", r.tile, 0.2, 200.0, 0.5, Unit::Mm, 1)),
                row(tr("Tiefe", "Depth"), number("depth", r.depth, 0.0, 20.0, 0.1, Unit::Mm, 2)),
                row(
                    tr("Richtung", "Direction"),
                    Node::Radio {
                        id: "relief".into(),
                        options: choices(&ReliefChoice::ALL, ReliefChoice::id, ReliefChoice::label),
                        selected: r.relief.id().into(),
                    },
                ),
                row(tr("Drehung", "Rotation"), number("rotation", r.rotation, -360.0, 360.0, 15.0, Unit::Degrees, 0)),
                row(tr("Glätten", "Smoothing"), number("smooth", r.smooth, 0.0, 25.0, 0.5, Unit::Percent, 1)),
                row(
                    tr("Übergang an Kanten", "Blend at edges"),
                    Node::NumberInput {
                        id: "blend".into(),
                        value: f64::from(r.blend),
                        min: Some(0.0),
                        max: Some(100.0),
                        step: Some(5.0),
                        unit: Unit::Percent,
                        decimals: Some(0),
                        enabled: r.mapping.blends(),
                    },
                ),
                row(
                    "",
                    Node::Checkbox {
                        id: "invert".into(),
                        label: tr("Hoch und tief tauschen", "Swap high and low").into(),
                        checked: r.invert,
                        enabled: true,
                    },
                ),
            ],
            submit: None,
        });

        children.push(Node::Separator);
        children.push(Node::heading(tr("Wohin", "Where")));
        children.push(Node::Radio {
            id: "place".into(),
            options: choices(&Place::ALL, Place::id, Place::label),
            selected: r.place.id().into(),
        });
        if r.place != Place::All {
            children.push(Node::Label {
                text: tr(
                    "Innen ist, was über einen Hohlraum auf eine andere Wand blickt (Bohrungen, Becher). Rillen außen zählen auch dazu.",
                    "Inside is what looks across a hollow at another wall (bores, cups). Grooves on the outside count as well.",
                )
                .into(),
                style: TextStyle::Small,
            });
        }
        children.push(Node::Form {
            id: "where".into(),
            rows: vec![
                row(
                    tr("Unterseite frei bis", "Bottom smooth up to"),
                    number("bottom", r.bottom, 0.0, 90.0, 5.0, Unit::Degrees, 0),
                ),
                row(
                    tr("Oberseite frei bis", "Top smooth up to"),
                    number("top", r.top, 0.0, 90.0, 5.0, Unit::Degrees, 0),
                ),
            ],
            submit: None,
        });

        children.push(Node::Separator);
        children.push(Node::heading(tr("Datei", "File")));
        children.push(Node::Form {
            id: "file".into(),
            rows: vec![
                row(
                    tr("Qualität", "Quality"),
                    Node::Radio {
                        id: "quality".into(),
                        options: choices(&QualityChoice::ALL, QualityChoice::id, QualityChoice::label),
                        selected: r.quality.id().into(),
                    },
                ),
                row(
                    tr("Format", "Format"),
                    Node::Radio {
                        id: "format".into(),
                        options: vec![
                            Choice { value: "3mf".into(), label: "3MF".into() },
                            Choice { value: "stl".into(), label: "STL".into() },
                        ],
                        selected: r.format.extension().into(),
                    },
                ),
            ],
            submit: None,
        });
        children.push(Node::Label {
            text: tf!(
                "Düse {} mm (vom Drucker in Meshgrove)",
                "Nozzle {} mm (from the printer in Meshgrove)",
                lang::number(self.host.nozzle, 2)
            ),
            style: TextStyle::Weak,
        });

        let count = self.sources.len();
        let label = match count {
            0 | 1 => tr("Textur auftragen", "Apply texture").to_owned(),
            n => tf!("Auf alle {} Modelle auftragen", "Apply to all {} models", n),
        };
        if let Some((i, fraction, message)) = &self.batch.current {
            let left = self.batch.queue.len();
            children.push(Node::Progress {
                fraction: Some(*fraction),
                text: Some(tf!("{}: {} (noch {})", "{}: {} ({} more)", self.sources[*i].name, message, left)),
            });
            children.push(Node::Button {
                id: "cancel".into(),
                label: tr("Abbrechen", "Cancel").into(),
                primary: false,
                enabled: true,
            });
        } else {
            children.push(Node::Button { id: "apply".into(), label, primary: true, enabled: count > 0 });
        }
        if let Some((level, text)) = &self.message {
            children.push(Node::Notice { level: *level, text: text.clone() });
        }
        Node::Scroll { max_height: None, children }
    }
}

impl GuestPanel for TexturePanel {
    fn new(_id: String, state: Option<String>) -> TexturePanel {
        let mut state: PanelState = state.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        state.host_changed(&guest::info());
        TexturePanel { state: RefCell::new(state) }
    }

    fn render(&self) -> String {
        guest::json(&View::new(self.state.borrow().render()))
    }

    fn event(&self, event: String) -> String {
        let Ok(event) = serde_json::from_str::<Event>(&event) else { return guest::effects(&[]) };
        guest::effects(&self.effects(event))
    }

    fn mesh(&self, key: String) -> Option<MeshData> {
        // Only names the panel made itself: no way out of the preview folder.
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
        work::read_mesh(&format!("{}/{key}", work::PREVIEW_DIR)).map(Into::into)
    }

    fn snapshot(&self) -> String {
        guest::json(&*self.state.borrow())
    }
}
