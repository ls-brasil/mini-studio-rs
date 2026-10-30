use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use fast_image_resize::{
    FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer,
    images::{Image, ImageRef},
};
use windows_capture::{
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    monitor::Monitor,
    settings::{
        ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
        GraphicsCaptureItemType, MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
    },
    window::Window,
};

#[cfg(test)]
use std::sync::atomic::AtomicU64;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SourceKind {
    Screen,
    Window,
}

#[derive(Debug, Clone)]
pub struct CaptureSource {
    pub id: u32,
    pub name: String,
    pub detail: String,
}

#[derive(Debug)]
pub struct NativeFrame {
    pub sequence: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

const FRAME_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 30);
const FRAME_WAIT: Duration = Duration::from_millis(120);

#[derive(Default)]
struct LatestFrame {
    sequence: u32,
    packet: Option<NativeFrame>,
    ended: Option<String>,
}

#[derive(Default)]
struct SharedFrame {
    latest: Mutex<LatestFrame>,
    changed: Condvar,
    stopped: AtomicBool,
    waker: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    #[cfg(test)]
    callbacks: AtomicU64,
    #[cfg(test)]
    processing_micros: AtomicU64,
    #[cfg(test)]
    readback_micros: AtomicU64,
}

impl SharedFrame {
    fn finish(&self, reason: &str) {
        self.stopped.store(true, Ordering::Release);
        let mut latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        let newly_finished = latest.ended.is_none();
        if latest.ended.is_none() {
            latest.ended = Some(reason.to_owned());
        }
        latest.packet = None;
        self.changed.notify_all();
        drop(latest);
        if newly_finished {
            self.wake();
        }
    }

    fn publish(&self, mut packet: NativeFrame) {
        let mut latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if latest.ended.is_some() {
            return;
        }
        latest.sequence = latest.sequence.wrapping_add(1).max(1);
        packet.sequence = latest.sequence;
        latest.packet = Some(packet);
        self.changed.notify_all();
        drop(latest);
        self.wake();
    }

    fn wake(&self) {
        let waker = self.waker.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(waker) = waker {
            waker();
        }
    }

    fn try_read(&self, after_sequence: u32) -> Result<Option<NativeFrame>, String> {
        let mut latest = self
            .latest
            .lock()
            .map_err(|_| "A captura está indisponível.")?;
        if let Some(reason) = &latest.ended {
            return Err(reason.clone());
        }
        if latest.sequence == after_sequence {
            return Ok(None);
        }
        Ok(latest.packet.take())
    }

    fn read(&self, after_sequence: u32) -> Result<Option<NativeFrame>, String> {
        let latest = self
            .latest
            .lock()
            .map_err(|_| "A captura está indisponível.")?;
        let (mut latest, _) = self
            .changed
            .wait_timeout_while(latest, FRAME_WAIT, |latest| {
                latest.ended.is_none()
                    && (latest.sequence == after_sequence || latest.packet.is_none())
            })
            .map_err(|_| "A captura foi interrompida.")?;
        if let Some(reason) = &latest.ended {
            return Err(reason.clone());
        }
        if latest.sequence == after_sequence {
            return Ok(None);
        }
        Ok(latest.packet.take())
    }
}

struct CaptureHandler {
    shared: Arc<SharedFrame>,
    next_frame: Instant,
    packed: Vec<u8>,
    resizer: Resizer,
}

fn preview_size(width: u32, height: u32) -> (u32, u32) {
    if width <= 1280 && height <= 720 {
        return (width, height);
    }
    let scale = (1280.0 / f64::from(width)).min(720.0 / f64::from(height));
    (
        (f64::from(width) * scale).round().max(1.0) as u32,
        (f64::from(height) * scale).round().max(1.0) as u32,
    )
}

fn prepare_frame(
    width: u32,
    height: u32,
    rgba: &[u8],
    resizer: &mut Resizer,
) -> Result<NativeFrame, String> {
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err("A fonte retornou dimensões inválidas.".into());
    }
    let (out_width, out_height) = preview_size(width, height);
    let mut output = vec![0; (out_width * out_height * 4) as usize];
    if (width, height) == (out_width, out_height) {
        let pixels = rgba
            .get(..(width * height * 4) as usize)
            .ok_or("Quadro incompleto.")?;
        output.copy_from_slice(pixels);
    } else {
        let source =
            ImageRef::new(width, height, rgba, PixelType::U8x4).map_err(|e| e.to_string())?;
        let mut target = Image::from_slice_u8(out_width, out_height, &mut output, PixelType::U8x4)
            .map_err(|e| e.to_string())?;
        resizer
            .resize(
                &source,
                &mut target,
                &ResizeOptions::new()
                    .resize_alg(ResizeAlg::Convolution(FilterType::Bilinear))
                    .use_alpha(false),
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(NativeFrame {
        sequence: 0,
        width: out_width,
        height: out_height,
        rgba: output,
    })
}

impl GraphicsCaptureApiHandler for CaptureHandler {
    type Flags = Arc<SharedFrame>;
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        Ok(Self {
            shared: ctx.flags,
            next_frame: Instant::now(),
            packed: Vec::new(),
            resizer: Resizer::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut Frame,
        control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        #[cfg(test)]
        self.shared.callbacks.fetch_add(1, Ordering::Relaxed);
        if self.shared.stopped.load(Ordering::Acquire) {
            control.stop();
            return Ok(());
        }
        let now = Instant::now();
        // A tolerância absorve o jitter do compositor sem cair de 30 para 20 FPS.
        if now + Duration::from_millis(2) < self.next_frame {
            return Ok(());
        }
        self.next_frame += FRAME_PERIOD;
        if self.next_frame < now {
            self.next_frame = now + FRAME_PERIOD;
        }
        let result = (|| {
            let width = frame.width();
            let height = frame.height();
            let mut buffer = frame.buffer().map_err(|e| e.to_string())?;
            #[cfg(test)]
            self.shared
                .readback_micros
                .fetch_add(now.elapsed().as_micros() as u64, Ordering::Relaxed);
            let stride = buffer.row_pitch() as usize;
            let row_bytes = width as usize * 4;
            let raw = buffer.as_raw_buffer();
            let pixels = if stride == row_bytes {
                &raw[..row_bytes * height as usize]
            } else {
                self.packed.resize(row_bytes * height as usize, 0);
                for (source, target) in raw
                    .chunks_exact(stride)
                    .zip(self.packed.chunks_exact_mut(row_bytes))
                {
                    target.copy_from_slice(&source[..row_bytes]);
                }
                &self.packed
            };
            prepare_frame(width, height, pixels, &mut self.resizer)
        })();
        match result {
            Ok(packet) => {
                #[cfg(test)]
                self.shared
                    .processing_micros
                    .fetch_add(now.elapsed().as_micros() as u64, Ordering::Relaxed);
                self.shared.publish(packet);
                Ok(())
            }
            Err(error) => {
                self.shared
                    .finish(&format!("Falha na captura acelerada: {error}"));
                control.stop();
                Err(error)
            }
        }
    }

    fn on_closed(&mut self) -> Result<(), Self::Error> {
        self.shared
            .finish("A fonte foi fechada ou desconectada. Selecione outra fonte.");
        Ok(())
    }
}

pub fn list_sources(kind: SourceKind) -> Result<Vec<CaptureSource>, String> {
    match kind {
        SourceKind::Screen => {
            let monitors = xcap::Monitor::all()
                .map_err(|e| format!("Não foi possível listar as telas: {e}"))?;
            Ok(monitors
                .iter()
                .enumerate()
                .filter_map(|(index, monitor)| {
                    let width = monitor.width().ok()?;
                    let height = monitor.height().ok()?;
                    Some(CaptureSource {
                        id: monitor.id().ok()?,
                        name: format!(
                            "Tela {} — {}",
                            index + 1,
                            monitor.friendly_name().unwrap_or_else(|_| "Monitor".into())
                        ),
                        detail: format!(
                            "{width} × {height}{}",
                            if monitor.is_primary().unwrap_or(false) {
                                " · principal"
                            } else {
                                ""
                            }
                        ),
                    })
                })
                .collect())
        }
        SourceKind::Window => {
            let windows = xcap::Window::all()
                .map_err(|e| format!("Não foi possível listar as janelas: {e}"))?;
            Ok(windows
                .iter()
                .filter_map(|window| {
                    let name = window.title().ok()?;
                    if name.trim().is_empty()
                        || window.pid().ok()? == std::process::id()
                        || window.is_minimized().unwrap_or(true)
                    {
                        return None;
                    }
                    let width = window.width().ok()?;
                    let height = window.height().ok()?;
                    if width == 0 || height == 0 {
                        return None;
                    }
                    Some(CaptureSource {
                        id: window.id().ok()?,
                        name,
                        detail: format!(
                            "{} · {width} × {height}",
                            window.app_name().unwrap_or_else(|_| "Aplicativo".into())
                        ),
                    })
                })
                .collect())
        }
    }
}

fn begin<T: TryInto<GraphicsCaptureItemType> + Send + 'static>(
    item: T,
    shared: Arc<SharedFrame>,
) -> Result<CaptureControl<CaptureHandler, String>, String> {
    let settings = Settings::new(
        item,
        CursorCaptureSettings::Default,
        DrawBorderSettings::Default,
        SecondaryWindowSettings::Default,
        MinimumUpdateIntervalSettings::Default,
        DirtyRegionSettings::Default,
        ColorFormat::Rgba8,
        shared,
    );
    CaptureHandler::start_free_threaded(settings)
        .map_err(|e| format!("Não foi possível iniciar Windows Graphics Capture: {e}"))
}

/// Sessão persistente. read espera até 120 ms; stop acorda o leitor e libera WGC.
/// Ao compartilhar via Arc, chame stop antes de encerrar o leitor em background.
pub struct CaptureSession {
    shared: Arc<SharedFrame>,
    control: Mutex<Option<CaptureControl<CaptureHandler, String>>>,
}

impl CaptureSession {
    pub fn start(kind: SourceKind, id: u32) -> Result<Self, String> {
        let shared = Arc::new(SharedFrame::default());
        let control = match kind {
            SourceKind::Screen => {
                let monitor = Monitor::enumerate()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .find(|monitor| monitor.as_raw_hmonitor() as usize as u32 == id)
                    .ok_or("A tela foi desconectada. Selecione outra fonte.")?;
                begin(monitor, shared.clone())?
            }
            SourceKind::Window => {
                let window = Window::enumerate()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .find(|window| window.as_raw_hwnd() as usize as u32 == id)
                    .ok_or("A janela foi fechada ou está indisponível.")?;
                if window.process_id().ok() == Some(std::process::id()) {
                    return Err("Selecione a janela de outro aplicativo.".into());
                }
                begin(window, shared.clone())?
            }
        };
        Ok(Self {
            shared,
            control: Mutex::new(Some(control)),
        })
    }

    pub fn stop(&self) {
        self.shared.finish("Captura encerrada.");
        if let Some(control) = self
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = control.stop();
        }
    }

    fn check_running(&self) -> Result<(), String> {
        let finished = self
            .control
            .lock()
            .map_err(|_| "A captura está indisponível.")?
            .as_ref()
            .is_none_or(CaptureControl::is_finished);
        if finished {
            self.shared
                .finish("A captura foi encerrada. Selecione a fonte novamente.");
        }
        Ok(())
    }

    pub fn set_waker(&self, waker: Arc<dyn Fn() + Send + Sync>) {
        *self.shared.waker.lock().unwrap_or_else(|e| e.into_inner()) = Some(waker);
        self.shared.wake();
    }

    pub fn try_read(&self, after_sequence: u32) -> Result<Option<NativeFrame>, String> {
        self.check_running()?;
        self.shared.try_read(after_sequence)
    }

    pub fn read(&self, after_sequence: u32) -> Result<Option<NativeFrame>, String> {
        self.check_running()?;
        self.shared.read(after_sequence)
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_frame_preserves_channels_and_aspect() {
        let pixels = [255, 12, 70, 255].repeat(1080 * 1920);
        let frame = prepare_frame(1080, 1920, &pixels, &mut Resizer::new()).unwrap();
        assert_eq!((frame.width, frame.height), (405, 720));
        assert_eq!(&frame.rgba[..4], &[255, 12, 70, 255]);
        assert_eq!(frame.rgba.len(), 405 * 720 * 4);
        assert_eq!(preview_size(320, 180), (320, 180));
    }

    fn fixture(width: u32) -> NativeFrame {
        NativeFrame {
            sequence: 0,
            width,
            height: 1,
            rgba: vec![0; width as usize * 4],
        }
    }

    #[test]
    fn latest_frame_drops_stale_frames_and_stop_wakes_reader() {
        let shared = Arc::new(SharedFrame::default());
        shared.publish(fixture(1));
        shared.publish(fixture(2));
        let frame = shared.read(0).unwrap().unwrap();
        assert_eq!(frame.sequence, 2);
        assert_eq!(frame.width, 2);
        let reader = shared.clone();
        let waiter = std::thread::spawn(move || reader.read(2));
        shared.finish("Encerrado");
        assert_eq!(waiter.join().unwrap().unwrap_err(), "Encerrado");
    }

    #[test]
    fn unchanged_frame_times_out_without_repeating_frame() {
        let shared = SharedFrame::default();
        shared.publish(fixture(1));
        let _ = shared.read(0).unwrap();
        let started = Instant::now();
        assert!(shared.read(1).unwrap().is_none());
        assert!(started.elapsed() >= FRAME_WAIT);
    }

    #[test]
    fn waker_runs_after_unlock_and_try_read_never_waits_for_a_frame() {
        let shared = Arc::new(SharedFrame::default());
        let weak = Arc::downgrade(&shared);
        let calls = Arc::new(AtomicU64::new(0));
        let wake_calls = calls.clone();
        *shared.waker.lock().unwrap() = Some(Arc::new(move || {
            let shared = weak.upgrade().unwrap();
            assert!(shared.latest.try_lock().is_ok());
            wake_calls.fetch_add(1, Ordering::Relaxed);
        }));
        assert!(shared.try_read(0).unwrap().is_none());
        shared.publish(fixture(1));
        assert_eq!(shared.try_read(0).unwrap().unwrap().sequence, 1);
        assert!(shared.try_read(1).unwrap().is_none());
        shared.finish("Encerrado");
        assert!(shared.try_read(1).is_err());
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    #[ignore = "Requer desktop interativo; mover conteúdo durante 5 segundos para medir FPS"]
    fn benchmark_real_monitor_stream() {
        let (kind, source_id) = if let Ok(title) = std::env::var("MINI_STUDIO_BENCH_WINDOW") {
            let window = Window::from_contains_name(&title).unwrap();
            (SourceKind::Window, window.as_raw_hwnd() as usize as u32)
        } else {
            let monitor = Monitor::primary().unwrap();
            (
                SourceKind::Screen,
                monitor.as_raw_hmonitor() as usize as u32,
            )
        };
        let session = CaptureSession::start(kind, source_id).unwrap();
        let start = Instant::now();
        let mut sequence = 0;
        let mut count = 0;
        while start.elapsed() < Duration::from_secs(5) {
            let Some(frame) = session.read(sequence).unwrap() else {
                continue;
            };
            sequence = frame.sequence;
            assert_eq!(frame.rgba.len(), (frame.width * frame.height * 4) as usize);
            count += 1;
        }
        session.stop();
        session.stop();
        assert!(count > 0);
        eprintln!(
            "WGC: {count} quadros em {:.2}s ({:.1} FPS; conteúdo estático produz menos quadros)",
            start.elapsed().as_secs_f64(),
            count as f64 / start.elapsed().as_secs_f64()
        );
        eprintln!(
            "Callbacks WGC: {}; processamento médio: {:.2}ms; readback médio: {:.2}ms",
            session.shared.callbacks.load(Ordering::Relaxed),
            session.shared.processing_micros.load(Ordering::Relaxed) as f64 / count as f64 / 1000.0,
            session.shared.readback_micros.load(Ordering::Relaxed) as f64 / count as f64 / 1000.0
        );
    }
}
