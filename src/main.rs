#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod camera;
mod composition;
mod image_source;
mod native_capture;
mod preferences;
mod studio_ui;
mod theme;
mod virtual_camera;

use camera::{CameraCapture, CameraDevice, CameraFrame};
use composition::{CameraPosition, ImageSettings, Pixels, Scene, Source};
use eframe::egui::{self, TextureHandle, TextureOptions};
use native_capture::{CaptureSession, CaptureSource, NativeFrame, SourceKind};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
struct Options {
    benchmark: Option<(SourceKind, String)>,
    seconds: f64,
    report: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    image: Option<PathBuf>,
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
                    options.image = Some(args.next().ok_or("Informe o caminho da imagem.")?.into());
                }
                "--ui-exit" => options.ui_exit = true,
                "--ui-dialog" => {
                    let dialog = args.next().ok_or("Informe help ou setup.")?;
                    if !matches!(dialog.as_str(), "help" | "setup") {
                        return Err("Diálogo inválido: use help ou setup.".into());
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

struct DesktopReady {
    source: CaptureSource,
    kind: SourceKind,
    session: CaptureSession,
    frame: NativeFrame,
}
struct PendingDesktop {
    receiver: Receiver<Result<DesktopReady, String>>,
    cancelled: Arc<AtomicBool>,
}
impl Drop for PendingDesktop {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
struct Desktop {
    source: CaptureSource,
    kind: SourceKind,
    session: CaptureSession,
    frame: NativeFrame,
    texture: TextureHandle,
}
struct Camera {
    device: CameraDevice,
    capture: CameraCapture,
    frame: CameraFrame,
    texture: TextureHandle,
}

struct ImageSource {
    loaded: image_source::LoadedImage,
    texture: TextureHandle,
}

struct Studio {
    selected: Option<Source>,
    inspector_tab: bool,
    desktop_visible: bool,
    camera_visible: bool,
    guides: bool,
    scale: f32,
    help_open: bool,
    setup_open: bool,
    notice_error: bool,
    notice_seen: String,
    notice_at: Instant,
    saved_path: Option<PathBuf>,
    preferences_at: Instant,
    saved_preferences: preferences::Preferences,
    canvas_drag: Option<(Source, f32, f32)>,
    pending_camera_at: Option<Instant>,
    image: Option<ImageSource>,
    image_settings: ImageSettings,
    image_receiver: Option<Receiver<Result<Option<image_source::LoadedImage>, String>>>,
    virtual_camera: Option<virtual_camera::VirtualCamera>,
    virtual_notice: String,
    desktop: Option<Desktop>,
    camera: Option<Camera>,
    pending_desktop: Option<PendingDesktop>,
    pending_camera: Option<(CameraDevice, CameraCapture)>,
    picker: Option<Picker>,
    notice: String,
    mirror: bool,
    camera_size: u32,
    camera_position: CameraPosition,
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
            selected: None,
            inspector_tab: false,
            desktop_visible: true,
            camera_visible: true,
            guides: prefs.guides,
            scale: prefs.scale,
            help_open: options.ui_dialog.as_deref() == Some("help"),
            setup_open: options.ui_dialog.as_deref() == Some("setup"),
            notice_error: false,
            notice_seen: String::new(),
            notice_at: Instant::now(),
            saved_path: None,
            preferences_at: Instant::now(),
            saved_preferences: prefs.clone(),
            canvas_drag: None,
            pending_camera_at: None,
            image: None,
            image_settings: prefs.image,
            image_receiver: None,
            virtual_camera: None,
            virtual_notice: "Câmera virtual parada.".into(),
            desktop: None,
            camera: None,
            pending_desktop: None,
            pending_camera: None,
            picker: None,
            notice: String::new(),
            mirror: prefs.mirror,
            camera_size: prefs.camera_size,
            camera_position: prefs.camera_position,
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
            frame_intervals: Vec::new(),
            last_frame_at: None,
            screenshot_requested: false,
            adapter,
        };
        if let Some((kind, title)) = &app.options.benchmark {
            let result = native_capture::list_sources(*kind).and_then(|sources| {
                sources
                    .into_iter()
                    .find(|source| source.name.contains(title))
                    .ok_or_else(|| "Fonte de benchmark não encontrada.".into())
            });
            match result {
                Ok(source) => app.start_desktop(ctx, *kind, source),
                Err(error) => app.notice = error,
            }
        }
        if let Some(path) = app.options.image.clone() {
            app.load_image(ctx, Some(path));
        }
        app
    }

    fn scene(&self) -> Scene {
        Scene {
            desktop: self
                .desktop
                .as_ref()
                .filter(|_| self.desktop_visible)
                .map(|s| Pixels {
                    width: s.frame.width,
                    height: s.frame.height,
                    rgba: s.frame.rgba.clone(),
                }),
            camera: self
                .camera
                .as_ref()
                .filter(|_| self.camera_visible)
                .map(|s| Pixels {
                    width: s.frame.width,
                    height: s.frame.height,
                    rgba: s.frame.rgba.clone(),
                }),
            image: self.image.as_ref().map(|s| s.loaded.pixels.clone()),
            image_settings: self.image_settings,
            mirror: self.mirror,
            camera_size: self.camera_size,
            camera_position: self.camera_position,
        }
    }

    fn open_image(&mut self, ctx: &egui::Context) {
        self.load_image(ctx, None);
    }

    fn load_image(&mut self, ctx: &egui::Context, path: Option<PathBuf>) {
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
    }

    fn open_picker(&mut self, ctx: &egui::Context, kind: PickerKind) {
        self.cancel_pending();
        let (sender, receiver) = mpsc::channel();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = match kind {
                PickerKind::Camera => camera::list_cameras()
                    .map(|devices| devices.into_iter().map(Choice::Camera).collect()),
                _ => {
                    let native_kind = if matches!(kind, PickerKind::Screen) {
                        SourceKind::Screen
                    } else {
                        SourceKind::Window
                    };
                    native_capture::list_sources(native_kind).map(|sources| {
                        sources
                            .into_iter()
                            .map(|source| Choice::Desktop(native_kind, source))
                            .collect()
                    })
                }
            };
            let _ = sender.send(result);
            context.request_repaint();
        });
        self.picker = Some(Picker {
            kind,
            receiver: Some(receiver),
            choices: Vec::new(),
            message: "Procurando fontes…".into(),
            search: String::new(),
        });
    }

    fn cancel_pending(&mut self) {
        self.pending_desktop = None;
        self.pending_camera = None;
        self.pending_camera_at = None;
    }

    fn start_desktop(&mut self, ctx: &egui::Context, kind: SourceKind, source: CaptureSource) {
        self.cancel_pending();
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = cancelled.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let session = CaptureSession::start(kind, source.id)?;
                let deadline = Instant::now() + Duration::from_secs(5);
                while !cancellation.load(Ordering::Acquire) && Instant::now() < deadline {
                    if let Some(frame) = session.read(0)? {
                        let wake = context.clone();
                        session.set_waker(Arc::new(move || wake.request_repaint()));
                        return Ok(DesktopReady {
                            source,
                            kind,
                            session,
                            frame,
                        });
                    }
                }
                Err("A fonte não enviou imagens ou a seleção foi cancelada.".to_string())
            })();
            let _ = sender.send(result);
            context.request_repaint();
        });
        self.pending_desktop = Some(PendingDesktop {
            receiver,
            cancelled,
        });
        if let Some(picker) = &mut self.picker {
            picker.message = "Conectando à fonte…".into();
        }
    }

    fn start_camera(&mut self, ctx: &egui::Context, device: CameraDevice) {
        self.cancel_pending();
        match CameraCapture::start(device.id.clone()) {
            Ok(capture) => {
                let wake = ctx.clone();
                capture.set_waker(Arc::new(move || wake.request_repaint()));
                self.pending_camera = Some((device, capture));
                self.pending_camera_at = Some(Instant::now());
                if let Some(picker) = &mut self.picker {
                    picker.message = "Abrindo câmera…".into();
                }
            }
            Err(error) => self.show_error(error),
        }
    }

    fn show_error(&mut self, error: String) {
        self.notice_error = true;
        self.notice = error.clone();
        if let Some(picker) = &mut self.picker {
            picker.message = error;
        }
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
        if let Some(receiver) = &self.image_receiver
            && let Ok(result) = receiver.try_recv()
        {
            self.image_receiver = None;
            self.notice_at = Instant::now();
            match result {
                Ok(Some(loaded)) => {
                    let p = &loaded.pixels;
                    let texture = load_texture(ctx, "image", p.width, p.height, &p.rgba);
                    self.image = Some(ImageSource { loaded, texture });
                    self.image_settings.visible = true;
                    self.selected = Some(Source::Image);
                    self.inspector_tab = true;
                    self.notice_error = false;
                    self.notice = "Imagem carregada.".into();
                }
                Ok(None) => {
                    self.notice_error = false;
                    self.notice = "Seleção de imagem cancelada.".into();
                }
                Err(error) => self.show_error(error),
            }
        }
        if let Some(output) = &self.virtual_camera {
            self.virtual_notice = output.status();
            if output.finished() {
                self.virtual_camera = None;
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }

        if let Some(picker) = &mut self.picker
            && let Some(receiver) = &picker.receiver
            && let Ok(result) = receiver.try_recv()
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
                Err(error) => picker.message = error,
            }
        }
        if let Some(pending) = &self.pending_desktop
            && let Ok(result) = pending.receiver.try_recv()
        {
            self.pending_desktop = None;
            match result {
                Ok(ready) => {
                    let texture = load_texture(
                        ctx,
                        "desktop",
                        ready.frame.width,
                        ready.frame.height,
                        &ready.frame.rgba,
                    );
                    self.desktop = Some(Desktop {
                        source: ready.source,
                        kind: ready.kind,
                        session: ready.session,
                        frame: ready.frame,
                        texture,
                    });
                    self.picker = None;
                    self.selected = Some(Source::Desktop);
                    self.inspector_tab = true;
                    self.desktop_visible = true;
                    self.notice_error = false;
                    self.notice = "Captura adicionada à composição.".into();
                    self.count_frame();
                }
                Err(error) => self.show_error(error),
            }
        }
        if self
            .pending_camera_at
            .is_some_and(|at| at.elapsed() > Duration::from_secs(12))
            && self.pending_camera.is_some()
        {
            self.pending_camera = None;
            self.pending_camera_at = None;
            self.show_error("A câmera não respondeu. Feche outros aplicativos que possam estar usando o dispositivo e tente novamente.".into());
        }
        let pending_result = self
            .pending_camera
            .as_ref()
            .map(|(_, capture)| capture.latest());
        match pending_result {
            Some(Ok(Some(frame))) => {
                if let Some((device, capture)) = self.pending_camera.take() {
                    let texture =
                        load_texture(ctx, "camera", frame.width, frame.height, &frame.rgba);
                    self.camera = Some(Camera {
                        device,
                        capture,
                        frame,
                        texture,
                    });
                    self.picker = None;
                    self.selected = Some(Source::Camera);
                    self.inspector_tab = true;
                    self.camera_visible = true;
                    self.notice_error = false;
                    self.pending_camera_at = None;
                    self.notice = "Câmera adicionada à composição.".into();
                    if self.desktop.is_none() {
                        self.count_frame();
                    }
                }
            }
            Some(Err(error)) => {
                self.pending_camera = None;
                self.show_error(error);
            }
            _ => {}
        }
        let desktop_result = self
            .desktop
            .as_ref()
            .map(|desktop| desktop.session.try_read(desktop.frame.sequence));
        match desktop_result {
            Some(Ok(Some(frame))) => {
                if let Some(desktop) = &mut self.desktop {
                    self.dropped_frames += frame
                        .sequence
                        .saturating_sub(desktop.frame.sequence)
                        .saturating_sub(1) as u64;
                    desktop.texture.set(
                        egui::ColorImage::from_rgba_unmultiplied(
                            [frame.width as usize, frame.height as usize],
                            &frame.rgba,
                        ),
                        TextureOptions::LINEAR,
                    );
                    desktop.frame = frame;
                    self.count_frame();
                }
            }
            Some(Err(error)) => {
                self.desktop = None;
                self.show_error(error);
            }
            _ => {}
        }
        let camera_result = self.camera.as_ref().map(|camera| camera.capture.latest());
        match camera_result {
            Some(Ok(Some(frame))) => {
                if let Some(camera) = &mut self.camera
                    && frame.sequence != camera.frame.sequence
                {
                    camera.texture.set(
                        egui::ColorImage::from_rgba_unmultiplied(
                            [frame.width as usize, frame.height as usize],
                            &frame.rgba,
                        ),
                        TextureOptions::LINEAR,
                    );
                    camera.frame = frame;
                    if self.desktop.is_none() {
                        self.count_frame();
                    }
                }
            }
            Some(Err(error)) => {
                self.camera = None;
                self.show_error(error);
            }
            _ => {}
        }
        if let Some(receiver) = &self.save_receiver
            && let Ok(result) = receiver.try_recv()
        {
            self.save_receiver = None;
            self.notice_at = Instant::now();
            self.notice_error = false;
            match result {
                Ok(Some(path)) => {
                    self.saved_path = Some(path.into());
                    self.notice = "Sua composição foi salva.".into();
                }
                Ok(None) => self.notice = "Salvamento cancelado.".into(),
                Err(error) => self.show_error(format!("Não foi possível salvar: {error}")),
            }
        }
        if self.fps_at.elapsed() >= Duration::from_secs(1) {
            self.fps = self.fps_frames as f64 / self.fps_at.elapsed().as_secs_f64();
            self.fps_frames = 0;
            self.fps_at = Instant::now();
        }
        if self.desktop.is_some() || self.camera.is_some() || self.options.benchmark.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
        if self.pending_desktop.is_some()
            || self.pending_camera.is_some()
            || self.image_receiver.is_some()
            || self.save_receiver.is_some()
            || self
                .picker
                .as_ref()
                .is_some_and(|picker| picker.receiver.is_some())
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn stop_all(&mut self) {
        if let Some(output) = &self.virtual_camera {
            output.stop();
        }
        self.image = None;
        self.image_receiver = None;
        self.cancel_pending();
        self.desktop = None;
        self.camera = None;
        self.picker = None;
        self.fps = 0.0;
        self.fps_frames = 0;
        self.selected = None;
        self.notice_error = false;
        self.notice = "Composição limpa. Todos os dispositivos foram liberados.".into();
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
        let report = serde_json::json!({"native":true,"renderer":"wgpu / Direct3D 12","adapter":self.adapter,"elapsed_seconds":seconds,"presented_frames":self.benchmark_frames,"presented_fps":if seconds>0.0 {self.benchmark_frames as f64/seconds}else{0.0},"frames_skipped":self.dropped_frames,"p50_frame_ms":percentile(0.5),"p95_frame_ms":percentile(0.95),"max_frame_ms":intervals.last().copied().unwrap_or_default(),"source":self.desktop.as_ref().map(|source| &source.source.name),"error":if timeout{Some(&self.notice)}else{None}});
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
