#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod camera;
mod canvas_editor;
mod composition;
mod image_source;
mod native_capture;
mod preferences;
mod scene_ui;
mod scenes;
mod source_runtime;
mod studio_ui;
mod theme;
mod virtual_camera;

use camera::CameraDevice;
use composition::{Scene, SceneSource, Source, Transform};
use eframe::egui::{self, TextureHandle, TextureOptions};
use native_capture::{CaptureSource, SourceKind};
use scenes::{SavedScene, SavedSource, SourceInput};
use source_runtime::RuntimeSource;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
struct Options {
    benchmark: Option<(SourceKind, String)>,
    seconds: f64,
    report: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    image: Vec<PathBuf>,
    ui_size: Option<[f32; 2]>,
    ui_exit: bool,
    ui_dialog: Option<String>,
}

impl Options {
    fn parse() -> Result<Self, String> {
        let mut options = Self {
            seconds: 15.0,
            ..Self::default()
        };
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--benchmark-window" => {
                    options.benchmark = Some((
                        SourceKind::Window,
                        args.next().ok_or("Informe o título da janela.")?,
                    ))
                }
                "--benchmark-screen" => {
                    options.benchmark = Some((SourceKind::Screen, String::new()))
                }
                "--seconds" => {
                    options.seconds = args
                        .next()
                        .ok_or("Informe a duração.")?
                        .parse()
                        .map_err(|_| "Duração inválida.")?
                }
                "--report" => {
                    options.report =
                        Some(args.next().ok_or("Informe o caminho do relatório.")?.into())
                }
                "--image" => {
                    options
                        .image
                        .push(args.next().ok_or("Informe o caminho da imagem.")?.into());
                }
                "--ui-exit" => options.ui_exit = true,
                "--ui-dialog" => {
                    let dialog = args.next().ok_or("Informe help, setup ou scene.")?;
                    if !matches!(dialog.as_str(), "help" | "setup" | "scene") {
                        return Err("Diálogo inválido: use help, setup ou scene.".into());
                    }
                    options.ui_dialog = Some(dialog);
                }
                "--ui-size" => {
                    let size = args.next().ok_or("Informe largura x altura.")?;
                    let (w, h) = size
                        .split_once('x')
                        .ok_or("Use largura x altura, por exemplo 1440x900.")?;
                    let size = [
                        w.parse::<f32>().map_err(|_| "Largura inválida.")?,
                        h.parse::<f32>().map_err(|_| "Altura inválida.")?,
                    ];
                    if !size
                        .iter()
                        .all(|n| n.is_finite() && (600.0..=4096.0).contains(n))
                    {
                        return Err("Tamanho inválido.".into());
                    }
                    options.ui_size = Some(size);
                }
                "--ui-screenshot" => {
                    options.screenshot =
                        Some(args.next().ok_or("Informe o caminho da imagem.")?.into())
                }
                _ => return Err(format!("Argumento desconhecido: {arg}")),
            }
        }
        if !options.seconds.is_finite() || !(1.0..=300.0).contains(&options.seconds) {
            return Err("Duração deve estar entre 1 e 300 segundos.".into());
        }
        Ok(options)
    }
}

#[derive(Clone, Copy)]
enum PickerKind {
    Screen,
    Window,
    Camera,
}

#[derive(Clone)]
enum Choice {
    Desktop(SourceKind, CaptureSource),
    Camera(CameraDevice),
}

impl Choice {
    fn name(&self) -> &str {
        match self {
            Self::Desktop(_, source) => &source.name,
            Self::Camera(device) => &device.name,
        }
    }
    fn detail(&self) -> &str {
        match self {
            Self::Desktop(_, source) => &source.detail,
            Self::Camera(_) => "Câmera do Windows",
        }
    }
}

struct Picker {
    kind: PickerKind,
    receiver: Option<Receiver<Result<Vec<Choice>, String>>>,
    choices: Vec<Choice>,
    message: String,
    search: String,
}

struct Studio {
    scenes: scenes::SceneLibrary,
    saved_scenes: scenes::SceneLibrary,
    scenes_load_error: Option<String>,
    scene_dialog: Option<scene_ui::SceneDialog>,
    runtimes: HashMap<Source, Rc<RefCell<RuntimeSource>>>,
    next_source_id: u64,
    selected: Option<Source>,
    properties: Option<Source>,
    picker_target: Option<Source>,
    image_target: Option<Source>,
    guides: bool,
    scale: f32,
    help_open: bool,
    setup_open: bool,
    notice_error: bool,
    notice: String,
    notice_at: Instant,
    notice_seen: String,
    saved_path: Option<PathBuf>,
    preferences_at: Instant,
    saved_preferences: preferences::Preferences,
    canvas_drag: Option<canvas_editor::Drag>,
    transform_undo: Vec<(Source, Transform)>,
    transform_redo: Vec<(Source, Transform)>,
    image_receiver: Option<Receiver<Result<Option<image_source::LoadedImage>, String>>>,
    virtual_camera: Option<virtual_camera::VirtualCamera>,
    virtual_notice: String,
    picker: Option<Picker>,
    save_receiver: Option<Receiver<Result<Option<String>, String>>>,
    fps_at: Instant,
    fps_frames: u64,
    fps: f64,
    total_frames: u64,
    dropped_frames: u64,
    options: Options,
    started: Instant,
    first_frame: Option<Instant>,
    benchmark_start: Option<Instant>,
    benchmark_frames: u64,
    frame_intervals: Vec<f64>,
    last_frame_at: Option<Instant>,
    screenshot_requested: bool,
    adapter: String,
}

fn load_texture(
    ctx: &egui::Context,
    name: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> TextureHandle {
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], rgba),
        TextureOptions::LINEAR,
    )
}

impl Studio {
    fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Self {
        let adapter = cc
            .wgpu_render_state
            .as_ref()
            .map(|state| state.adapter.get_info().name)
            .unwrap_or_else(|| "GPU".into());
        Self::initialize(&cc.egui_ctx, adapter, options)
    }
    fn initialize(ctx: &egui::Context, adapter: String, options: Options) -> Self {
        theme::configure(ctx);
        let prefs = if options.screenshot.is_some() || options.benchmark.is_some() {
            preferences::Preferences::default()
        } else {
            preferences::Preferences::load()
        };
        ctx.set_zoom_factor(prefs.scale);
        let mut app = Self {
            scenes: scenes::SceneLibrary::default(),
            saved_scenes: scenes::SceneLibrary::default(),
            scenes_load_error: None,
            scene_dialog: None,
            runtimes: HashMap::new(),
            next_source_id: 1,
            selected: None,
            properties: None,
            picker_target: None,
            image_target: None,
            guides: prefs.guides,
            scale: prefs.scale,
            help_open: options.ui_dialog.as_deref() == Some("help"),
            setup_open: options.ui_dialog.as_deref() == Some("setup"),
            notice_error: false,
            notice: String::new(),
            notice_at: Instant::now(),
            notice_seen: String::new(),
            saved_path: None,
            preferences_at: Instant::now(),
            saved_preferences: prefs,
            canvas_drag: None,
            transform_undo: Vec::new(),
            transform_redo: Vec::new(),
            image_receiver: None,
            virtual_camera: None,
            virtual_notice: "Câmera virtual parada.".into(),
            picker: None,
            save_receiver: None,
            fps_at: Instant::now(),
            fps_frames: 0,
            fps: 0.0,
            total_frames: 0,
            dropped_frames: 0,
            options,
            started: Instant::now(),
            first_frame: None,
            benchmark_start: None,
            benchmark_frames: 0,
            frame_intervals: vec![],
            last_frame_at: None,
            screenshot_requested: false,
            adapter,
        };
        app.initialize_scenes();
        if app.options.ui_dialog.as_deref() == Some("scene") {
            app.scene_dialog = Some(scene_ui::SceneDialog::Create {
                name: "Nova cena".into(),
            });
        }
        if let Some((kind, title)) = &app.options.benchmark {
            let result = native_capture::list_sources(*kind).and_then(|sources| {
                sources
                    .into_iter()
                    .find(|s| s.name.contains(title))
                    .ok_or_else(|| "Fonte de benchmark não encontrada.".into())
            });
            match result {
                Ok(source) => app.start_desktop(ctx, *kind, source),
                Err(error) => app.show_error(error),
            }
        }
        for path in app.options.image.clone() {
            app.add_input(ctx, SourceInput::Image(path));
        }
        app
    }
    fn allocate_source_id(&mut self) -> Source {
        let id = self.next_source_id;
        self.next_source_id += 1;
        id
    }
    fn current_scene(&self) -> &SavedScene {
        &self.scenes.scenes[self.scenes.active]
    }
    fn current_scene_mut(&mut self) -> &mut SavedScene {
        &mut self.scenes.scenes[self.scenes.active]
    }
    fn source(&self, id: Source) -> Option<&SavedSource> {
        self.current_scene().sources.iter().find(|s| s.id == id)
    }
    fn source_mut(&mut self, id: Source) -> Option<&mut SavedSource> {
        self.current_scene_mut()
            .sources
            .iter_mut()
            .find(|s| s.id == id)
    }
    fn scene(&self) -> Scene {
        Scene {
            sources: self
                .current_scene()
                .sources
                .iter()
                .filter(|s| s.visible)
                .filter_map(|source| {
                    let runtime = self.runtimes.get(&source.id)?.borrow();
                    Some(SceneSource {
                        id: source.id,
                        pixels: runtime.pixels.clone()?,
                        transform: source.transform,
                    })
                })
                .collect(),
        }
    }
    fn activate_source(&mut self, ctx: &egui::Context, id: Source) {
        let Some(source) = self.source(id) else {
            return;
        };
        let input = source.input.clone();
        let shared = self
            .current_scene()
            .sources
            .iter()
            .filter(|s| s.id != id && s.input == input)
            .find_map(|s| {
                self.runtimes
                    .get(&s.id)
                    .filter(|r| r.borrow().error.is_none())
                    .cloned()
            });
        let runtime =
            shared.unwrap_or_else(|| Rc::new(RefCell::new(RuntimeSource::loading(ctx, input))));
        self.runtimes.insert(id, runtime);
    }
    fn add_input(&mut self, ctx: &egui::Context, input: SourceInput) -> Source {
        let id = self.allocate_source_id();
        let name = input.name();
        self.current_scene_mut().sources.push(SavedSource {
            id,
            name,
            input,
            visible: true,
            locked: false,
            transform: Transform::default(),
        });
        self.selected = Some(id);
        self.activate_source(ctx, id);
        id
    }
    fn accept_input(&mut self, ctx: &egui::Context, input: SourceInput) {
        if let Some(id) = self.picker_target.take() {
            if let Some(source) = self.source_mut(id) {
                source.input = input;
                self.runtimes.remove(&id);
                self.activate_source(ctx, id);
            }
        } else {
            self.add_input(ctx, input);
        }
        self.picker = None;
    }
    fn open_image(&mut self, ctx: &egui::Context) {
        self.load_image(ctx, None, None);
    }
    fn load_image(&mut self, ctx: &egui::Context, path: Option<PathBuf>, target: Option<Source>) {
        let (sender, receiver) = mpsc::channel();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = path
                .or_else(|| {
                    rfd::FileDialog::new()
                        .set_title("Adicionar imagem")
                        .add_filter("PNG ou JPEG", &["png", "jpg", "jpeg"])
                        .pick_file()
                })
                .map(|path| image_source::load(&path))
                .transpose();
            let _ = sender.send(result);
            context.request_repaint();
        });
        self.image_receiver = Some(receiver);
        self.image_target = target;
    }
    fn accept_image(
        &mut self,
        ctx: &egui::Context,
        loaded: image_source::LoadedImage,
        target: Option<Source>,
    ) {
        let id = if let Some(id) = target {
            let Some(source) = self.source_mut(id) else {
                return;
            };
            source.input = SourceInput::Image(loaded.path);
            id
        } else {
            let id = self.allocate_source_id();
            self.current_scene_mut().sources.push(SavedSource {
                id,
                name: loaded.name,
                input: SourceInput::Image(loaded.path),
                visible: true,
                locked: false,
                transform: Transform::default(),
            });
            id
        };
        self.runtimes.insert(
            id,
            Rc::new(RefCell::new(RuntimeSource::image(ctx, loaded.pixels))),
        );
        self.selected = Some(id);
        self.notice_error = false;
        self.notice = "Imagem adicionada à cena.".into();
    }
    fn open_picker(&mut self, ctx: &egui::Context, kind: PickerKind, target: Option<Source>) {
        let (sender, receiver) = mpsc::channel();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = match kind {
                PickerKind::Camera => camera::list_cameras()
                    .map(|devices| devices.into_iter().map(Choice::Camera).collect()),
                _ => {
                    let kind = if matches!(kind, PickerKind::Screen) {
                        SourceKind::Screen
                    } else {
                        SourceKind::Window
                    };
                    native_capture::list_sources(kind).map(|sources| {
                        sources
                            .into_iter()
                            .map(|s| Choice::Desktop(kind, s))
                            .collect()
                    })
                }
            };
            let _ = sender.send(result);
            context.request_repaint();
        });
        self.picker_target = target;
        self.picker = Some(Picker {
            kind,
            receiver: Some(receiver),
            choices: vec![],
            message: "Procurando fontes…".into(),
            search: String::new(),
        });
    }
    fn start_desktop(&mut self, ctx: &egui::Context, kind: SourceKind, source: CaptureSource) {
        self.accept_input(
            ctx,
            SourceInput::Desktop(scenes::DesktopSource {
                selected_id: Some(source.id),
                window: kind == SourceKind::Window,
                name: source.name,
                detail: source.detail,
            }),
        );
    }
    fn start_camera(&mut self, ctx: &egui::Context, device: CameraDevice) {
        self.accept_input(
            ctx,
            SourceInput::Camera(scenes::CameraSource {
                id: device.id,
                name: device.name,
            }),
        );
    }
    fn show_error(&mut self, error: String) {
        self.notice_error = true;
        self.notice = error;
        self.notice_at = Instant::now();
    }
    fn count_frame(&mut self) {
        let now = Instant::now();
        self.fps_frames += 1;
        self.total_frames += 1;
        let first = *self.first_frame.get_or_insert(now);
        if self.options.benchmark.is_some() && now.duration_since(first).as_secs_f64() >= 2.0 {
            self.benchmark_start.get_or_insert(now);
            self.benchmark_frames += 1;
            if let Some(last) = self.last_frame_at {
                self.frame_intervals
                    .push(now.duration_since(last).as_secs_f64() * 1000.0);
            }
            self.last_frame_at = Some(now);
        }
    }

    fn receive_frames(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.image_receiver.as_ref().and_then(|r| r.try_recv().ok()) {
            self.image_receiver = None;
            let target = self.image_target.take();
            match result {
                Ok(Some(loaded)) => self.accept_image(ctx, loaded, target),
                Ok(None) => {}
                Err(e) => self.show_error(e),
            }
        }
        if let Some(picker) = &mut self.picker
            && let Some(result) = picker.receiver.as_ref().and_then(|r| r.try_recv().ok())
        {
            picker.receiver = None;
            match result {
                Ok(choices) => {
                    picker.choices = choices;
                    picker.message = if picker.choices.is_empty() {
                        "Nenhuma fonte disponível."
                    } else {
                        "Selecione uma fonte."
                    }
                    .into();
                }
                Err(e) => picker.message = e,
            }
        }
        let mut updated = false;
        let mut seen = HashSet::new();
        let mut errors = Vec::new();
        for (id, runtime) in &self.runtimes {
            if !seen.insert(Rc::as_ptr(runtime)) {
                continue;
            }
            let (changed, dropped, error) = runtime.borrow_mut().poll(ctx);
            updated |= changed;
            self.dropped_frames += dropped;
            if let Some(error) = error {
                let name = self.source(*id).map(|s| s.name.as_str()).unwrap_or("Fonte");
                errors.push(format!("{name}: {error}"));
            }
        }
        if updated {
            self.count_frame();
        }
        if !errors.is_empty() {
            self.show_error(errors.join(" "));
        }
        if let Some(output) = &self.virtual_camera {
            self.virtual_notice = output.status();
            if output.finished() {
                self.virtual_camera = None;
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(result) = self.save_receiver.as_ref().and_then(|r| r.try_recv().ok()) {
            self.save_receiver = None;
            self.notice_at = Instant::now();
            match result {
                Ok(Some(path)) => {
                    self.saved_path = Some(path.into());
                    self.notice_error = false;
                    self.notice = "Sua composição foi salva.".into();
                }
                Ok(None) => {}
                Err(e) => self.show_error(e),
            }
        }
        if self.fps_at.elapsed() >= Duration::from_secs(1) {
            self.fps = self.fps_frames as f64 / self.fps_at.elapsed().as_secs_f64();
            self.fps_frames = 0;
            self.fps_at = Instant::now();
        }
        if self.runtimes.values().any(|r| r.borrow().loading_now())
            || self.image_receiver.is_some()
            || self.save_receiver.is_some()
            || self.picker.as_ref().is_some_and(|p| p.receiver.is_some())
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn clear_scene_runtime(&mut self) {
        self.runtimes.clear();
        self.picker = None;
        self.picker_target = None;
        self.image_receiver = None;
        self.image_target = None;
        self.selected = None;
        self.properties = None;
        self.canvas_drag = None;
        self.transform_undo.clear();
        self.transform_redo.clear();
        self.fps = 0.0;
        self.fps_frames = 0;
    }
    fn stop_all(&mut self) {
        if let Some(output) = &self.virtual_camera {
            output.stop();
        }
        self.clear_scene_runtime();
    }
    fn save_snapshot(&mut self, ctx: &egui::Context) {
        let scene = self.scene();
        let (sender, receiver) = mpsc::channel();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let image = scene.compose()?;
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let Some(path) = rfd::FileDialog::new()
                    .set_title("Salvar composição")
                    .set_file_name(format!("mini-studio-{stamp}.png"))
                    .add_filter("Imagem PNG", &["png"])
                    .save_file()
                else {
                    return Ok(None);
                };
                image
                    .save_with_format(&path, image::ImageFormat::Png)
                    .map_err(|error| error.to_string())?;
                Ok(Some(path.display().to_string()))
            })();
            let _ = sender.send(result);
            context.request_repaint();
        });
        self.save_receiver = Some(receiver);
    }

    fn benchmark_tick(&mut self, ctx: &egui::Context) {
        if let Some(path) = &self.options.screenshot {
            if !self.screenshot_requested {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if !self.screenshot_requested && self.started.elapsed() > Duration::from_secs(3) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.screenshot_requested = true;
            }
            let mut close = false;
            ctx.input(|input| {
                for event in &input.events {
                    if let egui::Event::Screenshot { image, .. } = event {
                        let bytes: Vec<u8> = image
                            .pixels
                            .iter()
                            .flat_map(|pixel| pixel.to_array())
                            .collect();
                        let result = image::save_buffer_with_format(
                            path,
                            &bytes,
                            image.width() as u32,
                            image.height() as u32,
                            image::ColorType::Rgba8,
                            image::ImageFormat::Png,
                        );
                        if result.is_ok() && self.options.ui_exit {
                            close = true;
                        }
                    }
                }
            });
            if close {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.options.benchmark.is_none() {
            return;
        }
        let complete = self
            .benchmark_start
            .is_some_and(|start| start.elapsed().as_secs_f64() >= self.options.seconds);
        let timeout =
            self.benchmark_start.is_none() && self.started.elapsed() > Duration::from_secs(15);
        if !complete && !timeout {
            return;
        }
        let seconds = self
            .benchmark_start
            .map(|start| start.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        let mut intervals = self.frame_intervals.clone();
        intervals.sort_by(f64::total_cmp);
        let percentile = |p: f64| -> f64 {
            if intervals.is_empty() {
                0.0
            } else {
                intervals[((intervals.len() - 1) as f64 * p).round() as usize]
            }
        };
        let report = serde_json::json!({"native":true,"renderer":"wgpu / Direct3D 12","adapter":self.adapter,"elapsed_seconds":seconds,"presented_frames":self.benchmark_frames,"presented_fps":if seconds>0.0 {self.benchmark_frames as f64/seconds}else{0.0},"frames_skipped":self.dropped_frames,"p50_frame_ms":percentile(0.5),"p95_frame_ms":percentile(0.95),"max_frame_ms":intervals.last().copied().unwrap_or_default(),"source":self.current_scene().sources.first().map(|source| &source.name),"error":if timeout{Some(&self.notice)}else{None}});
        let path = self
            .options
            .report
            .clone()
            .unwrap_or_else(|| PathBuf::from("target/benchmark-native.json"));
        if let Ok(json) = serde_json::to_string_pretty(&report) {
            let _ = std::fs::write(path, json);
        }
        self.stop_all();
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        rfd::MessageDialog::new()
            .set_title("Mini Studio — não foi possível iniciar")
            .set_description(format!("Confira se o Windows e o driver de vídeo estão atualizados. O Mini Studio precisa de uma GPU compatível com Direct3D 12.\n\nDetalhes: {error}"))
            .set_level(rfd::MessageLevel::Error)
            .show();
        std::process::exit(1);
    }
}

fn run() -> eframe::Result {
    let options = Options::parse().map_err(|error| eframe::Error::AppCreation(error.into()))?;
    let window_size = options.ui_size.unwrap_or([1440.0, 900.0]);
    let mut native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(window_size)
            .with_min_inner_size([1024.0, 720.0])
            .with_icon(theme::app_icon())
            .with_app_id("br.lsbrasil.ministudio"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    native_options.wgpu_options.desired_maximum_frame_latency = Some(1);
    if options.benchmark.is_some() {
        native_options.viewport = native_options.viewport.with_position([1300.0, 40.0]);
    }
    eframe::run_native(
        "Mini Studio",
        native_options,
        Box::new(move |cc| Ok(Box::new(Studio::new(cc, options)))),
    )
}
