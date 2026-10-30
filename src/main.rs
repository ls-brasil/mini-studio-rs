#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod camera;
mod composition;
mod native_capture;

use camera::{CameraCapture, CameraDevice, CameraFrame};
use composition::{CameraPosition, camera_rect, compose_snapshot, fit_rect};
use eframe::egui::{self, Color32, Rect, TextureHandle, TextureOptions, Vec2, pos2};
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

struct Studio {
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
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let adapter = cc
            .wgpu_render_state
            .as_ref()
            .map(|state| state.adapter.get_info().name)
            .unwrap_or_else(|| "GPU".into());
        let mut app = Self {
            desktop: None,
            camera: None,
            pending_desktop: None,
            pending_camera: None,
            picker: None,
            notice: "Escolha uma tela, janela ou câmera para começar.".into(),
            mirror: false,
            camera_size: 25,
            camera_position: CameraPosition::BottomRight,
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
                Ok(source) => app.start_desktop(&cc.egui_ctx, *kind, source),
                Err(error) => app.notice = error,
            }
        }
        app
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
        });
    }

    fn cancel_pending(&mut self) {
        self.pending_desktop = None;
        self.pending_camera = None;
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
                if let Some(picker) = &mut self.picker {
                    picker.message = "Abrindo câmera…".into();
                }
            }
            Err(error) => self.show_error(error),
        }
    }

    fn show_error(&mut self, error: String) {
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
                    self.notice = "Fonte conectada. Prévia nativa com alvo de 30 FPS.".into();
                    self.count_frame();
                }
                Err(error) => self.show_error(error),
            }
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
                    self.notice = "Câmera conectada.".into();
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
                self.notice = error;
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
                self.notice = error;
            }
            _ => {}
        }
        if let Some(receiver) = &self.save_receiver
            && let Ok(result) = receiver.try_recv()
        {
            self.save_receiver = None;
            self.notice = match result {
                Ok(Some(path)) => format!("Imagem salva em {path}"),
                Ok(None) => "Salvamento cancelado.".into(),
                Err(error) => error,
            };
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
        self.cancel_pending();
        self.desktop = None;
        self.camera = None;
        self.picker = None;
        self.fps = 0.0;
        self.fps_frames = 0;
        self.notice = "Todas as capturas foram encerradas.".into();
    }

    fn save_snapshot(&mut self, ctx: &egui::Context) {
        let base = self.desktop.as_ref().map(|source| {
            (
                source.frame.width,
                source.frame.height,
                source.frame.rgba.clone(),
            )
        });
        let camera = self.camera.as_ref().map(|source| {
            (
                source.frame.width,
                source.frame.height,
                source.frame.rgba.clone(),
            )
        });
        let (mirror, size, position) = (self.mirror, self.camera_size, self.camera_position);
        let (sender, receiver) = mpsc::channel();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let image = compose_snapshot(
                    base.as_ref().map(|(w, h, p)| (*w, *h, p.as_slice())),
                    camera.as_ref().map(|(w, h, p)| (*w, *h, p.as_slice())),
                    mirror,
                    size,
                    position,
                )?;
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

    fn draw_picker(&mut self, ctx: &egui::Context) {
        let busy = self.pending_desktop.is_some() || self.pending_camera.is_some();
        let mut selected = None;
        let mut open = true;
        let mut refresh = None;
        if let Some(picker) = &mut self.picker {
            let title = match picker.kind {
                PickerKind::Screen => "Selecionar tela",
                PickerKind::Window => "Selecionar janela",
                PickerKind::Camera => "Selecionar câmera",
            };
            egui::Window::new(title)
                .open(&mut open)
                .collapsible(false)
                .default_width(540.0)
                .show(ctx, |ui| {
                    ui.label(&picker.message);
                    ui.add_space(8.0);
                    if busy || picker.receiver.is_some() {
                        ui.spinner();
                    }
                    egui::ScrollArea::vertical()
                        .max_height(380.0)
                        .show(ui, |ui| {
                            for choice in &picker.choices {
                                let text = format!("{}\n{}", choice.name(), choice.detail());
                                if ui
                                    .add_enabled(
                                        !busy,
                                        egui::Button::new(text)
                                            .min_size(Vec2::new(ui.available_width(), 48.0)),
                                    )
                                    .clicked()
                                {
                                    selected = Some(choice.clone());
                                }
                            }
                        });
                    ui.add_space(10.0);
                    if ui
                        .add_enabled(
                            !busy && picker.receiver.is_none(),
                            egui::Button::new("Atualizar lista"),
                        )
                        .clicked()
                    {
                        refresh = Some(picker.kind);
                    }
                });
        }
        if !open {
            self.picker = None;
            self.cancel_pending();
        }
        if let Some(kind) = refresh {
            self.open_picker(ctx, kind);
        }
        match selected {
            Some(Choice::Desktop(kind, source)) => self.start_desktop(ctx, kind, source),
            Some(Choice::Camera(device)) => self.start_camera(ctx, device),
            None => {}
        }
    }

    fn benchmark_tick(&mut self, ctx: &egui::Context) {
        if let Some(path) = &self.options.screenshot {
            if !self.screenshot_requested && self.started.elapsed() > Duration::from_secs(3) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
                self.screenshot_requested = true;
            }
            ctx.input(|input| {
                for event in &input.events {
                    if let egui::Event::Screenshot { image, .. } = event {
                        let bytes: Vec<u8> = image
                            .pixels
                            .iter()
                            .flat_map(|pixel| pixel.to_array())
                            .collect();
                        let _ = image::save_buffer_with_format(
                            path,
                            &bytes,
                            image.width() as u32,
                            image.height() as u32,
                            image::ColorType::Rgba8,
                            image::ImageFormat::Png,
                        );
                    }
                }
            });
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

fn as_rect(rect: [f32; 4]) -> Rect {
    Rect::from_min_size(pos2(rect[0], rect[1]), Vec2::new(rect[2], rect[3]))
}
fn draw_source(
    painter: &egui::Painter,
    texture: &TextureHandle,
    width: u32,
    height: u32,
    area: [f32; 4],
    mirror: bool,
) {
    let uv = if mirror {
        Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0))
    } else {
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
    };
    painter.image(
        texture.id(),
        as_rect(fit_rect(width, height, area)),
        uv,
        Color32::WHITE,
    );
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_frames(ctx);
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("Mini Studio");
                ui.label("MVP nativo");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(if self.desktop.is_some() || self.camera.is_some() {
                        "Capturando"
                    } else {
                        "Pronto para capturar"
                    });
                });
            });
            ui.add_space(8.0);
        });
        egui::SidePanel::left("sources")
            .exact_width(250.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.heading("Fontes de captura");
                ui.add_space(10.0);
                for (label, kind) in [
                    ("Capturar tela", PickerKind::Screen),
                    ("Capturar janela", PickerKind::Window),
                    ("Capturar câmera", PickerKind::Camera),
                ] {
                    if ui
                        .add_sized([ui.available_width(), 36.0], egui::Button::new(label))
                        .clicked()
                    {
                        self.open_picker(ctx, kind);
                    }
                }
                ui.add_space(14.0);
                ui.separator();
                if let Some(desktop) = &self.desktop {
                    ui.label(&desktop.source.name);
                    ui.small(if desktop.kind == SourceKind::Screen {
                        "Tela"
                    } else {
                        "Janela"
                    });
                    if ui.button("Remover captura").clicked() {
                        self.desktop = None;
                    }
                    ui.separator();
                }
                if let Some(camera) = &self.camera {
                    ui.label(&camera.device.name);
                    if ui.button("Remover câmera").clicked() {
                        self.camera = None;
                    }
                    ui.separator();
                }
                if self.camera.is_some() {
                    ui.add_space(12.0);
                    ui.strong("Ajustes da câmera");
                    ui.checkbox(&mut self.mirror, "Espelhar câmera");
                    egui::ComboBox::from_label("Canto")
                        .selected_text(match self.camera_position {
                            CameraPosition::BottomRight => "Inferior direito",
                            CameraPosition::BottomLeft => "Inferior esquerdo",
                            CameraPosition::TopRight => "Superior direito",
                            CameraPosition::TopLeft => "Superior esquerdo",
                        })
                        .show_ui(ui, |ui| {
                            for (value, label) in [
                                (CameraPosition::BottomRight, "Inferior direito"),
                                (CameraPosition::BottomLeft, "Inferior esquerdo"),
                                (CameraPosition::TopRight, "Superior direito"),
                                (CameraPosition::TopLeft, "Superior esquerdo"),
                            ] {
                                ui.selectable_value(&mut self.camera_position, value, label);
                            }
                        });
                    ui.add(
                        egui::Slider::new(&mut self.camera_size, 15..=40)
                            .suffix("%")
                            .text("Tamanho"),
                    );
                }
                ui.add_space(20.0);
                ui.small("1 tela ou janela + 1 câmera");
                ui.small("Sem áudio, gravação ou transmissão neste MVP.");
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Prévia ao vivo");
            ui.add_space(8.0);
            let width = ui
                .available_width()
                .min((ui.available_height() - 130.0).max(180.0) * 16.0 / 9.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(width, width * 9.0 / 16.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, Color32::from_rgb(8, 11, 16));
            let area = [rect.left(), rect.top(), rect.width(), rect.height()];
            if let Some(desktop) = &self.desktop {
                draw_source(
                    &painter,
                    &desktop.texture,
                    desktop.frame.width,
                    desktop.frame.height,
                    area,
                    false,
                );
            }
            if let Some(camera) = &self.camera {
                let camera_area = if self.desktop.is_some() {
                    camera_rect(self.camera_size, self.camera_position, area)
                } else {
                    area
                };
                if self.desktop.is_some() {
                    painter.rect_filled(as_rect(camera_area).expand(1.0), 0.0, Color32::WHITE);
                    painter.rect_filled(as_rect(camera_area), 0.0, Color32::from_rgb(8, 11, 16));
                }
                draw_source(
                    &painter,
                    &camera.texture,
                    camera.frame.width,
                    camera.frame.height,
                    camera_area,
                    self.mirror,
                );
            }
            if self.desktop.is_none() && self.camera.is_none() {
                painter.text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Escolha uma fonte para iniciar a prévia",
                    egui::FontId::proportional(18.0),
                    Color32::GRAY,
                );
            }
            ui.horizontal(|ui| {
                ui.small("1280 × 720 · GPU");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{:.0} FPS", self.fps)).on_hover_text(
                        "Quadros novos exibidos. Uma fonte parada pode enviar menos quadros.",
                    );
                });
            });
            ui.add_space(12.0);
            let active = self.desktop.is_some() || self.camera.is_some();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        active && self.save_receiver.is_none(),
                        egui::Button::new("Salvar imagem PNG"),
                    )
                    .clicked()
                {
                    self.save_snapshot(ctx);
                }
                if ui
                    .add_enabled(
                        active || self.pending_desktop.is_some() || self.pending_camera.is_some(),
                        egui::Button::new("Parar tudo"),
                    )
                    .clicked()
                {
                    self.stop_all();
                }
                if self.save_receiver.is_some() {
                    ui.spinner();
                }
            });
            ui.add_space(10.0);
            ui.label(&self.notice);
        });
        self.draw_picker(ctx);
        self.benchmark_tick(ctx);
    }
}

fn main() -> eframe::Result {
    let options = Options::parse().map_err(|error| eframe::Error::AppCreation(error.into()))?;
    let mut native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([900.0, 650.0]),
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
