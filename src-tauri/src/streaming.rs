use std::{
    collections::HashMap,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
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

use crate::capture::SourceKind;

const FRAME_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 30);
const FRAME_WAIT: Duration = Duration::from_millis(120);
const HEADER_BYTES: usize = 12;

#[derive(Default)]
struct LatestFrame {
    sequence: u32,
    packet: Option<Vec<u8>>,
    ended: Option<String>,
}

#[derive(Default)]
struct SharedFrame {
    latest: Mutex<LatestFrame>,
    changed: Condvar,
    stopped: AtomicBool,
    #[cfg(test)]
    callbacks: AtomicU64,
    #[cfg(test)]
    processing_micros: AtomicU64,
}

impl SharedFrame {
    fn finish(&self, reason: &str) {
        self.stopped.store(true, Ordering::Release);
        let mut latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if latest.ended.is_none() {
            latest.ended = Some(reason.to_owned());
        }
        latest.packet = None;
        self.changed.notify_all();
    }

    fn publish(&self, mut packet: Vec<u8>) {
        let mut latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        if latest.ended.is_some() {
            return;
        }
        latest.sequence = latest.sequence.wrapping_add(1).max(1);
        packet[..4].copy_from_slice(&latest.sequence.to_le_bytes());
        latest.packet = Some(packet);
        self.changed.notify_all();
    }

    fn read(&self, after_sequence: u32) -> Result<Vec<u8>, String> {
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
            return Ok(Vec::new());
        }
        Ok(latest.packet.take().unwrap_or_default())
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

fn prepare_packet(
    width: u32,
    height: u32,
    rgba: &[u8],
    resizer: &mut Resizer,
) -> Result<Vec<u8>, String> {
    if width == 0 || height == 0 || width > 16384 || height > 16384 {
        return Err("A fonte retornou dimensões inválidas.".into());
    }
    let (out_width, out_height) = preview_size(width, height);
    let mut packet = vec![0; HEADER_BYTES + (out_width * out_height * 4) as usize];
    packet[4..8].copy_from_slice(&out_width.to_le_bytes());
    packet[8..12].copy_from_slice(&out_height.to_le_bytes());
    if (width, height) == (out_width, out_height) {
        let pixels = rgba
            .get(..(width * height * 4) as usize)
            .ok_or("Quadro incompleto.")?;
        packet[HEADER_BYTES..].copy_from_slice(pixels);
    } else {
        let source =
            ImageRef::new(width, height, rgba, PixelType::U8x4).map_err(|e| e.to_string())?;
        let mut target = Image::from_slice_u8(
            out_width,
            out_height,
            &mut packet[HEADER_BYTES..],
            PixelType::U8x4,
        )
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
    Ok(packet)
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
            prepare_packet(width, height, pixels, &mut self.resizer)
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

struct Session {
    shared: Arc<SharedFrame>,
    control: Mutex<Option<CaptureControl<CaptureHandler, String>>>,
}

impl Session {
    fn stop(&self) {
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

    fn read(&self, after_sequence: u32) -> Result<Vec<u8>, String> {
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
        self.shared.read(after_sequence)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Default)]
pub struct CaptureSessions {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU64,
    shutting_down: AtomicBool,
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

impl CaptureSessions {
    fn start(&self, kind: SourceKind, id: u32) -> Result<String, String> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| "Não foi possível iniciar a captura.")?;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err("O aplicativo está encerrando.".into());
        }
        sessions.retain(|_, session| !session.shared.stopped.load(Ordering::Acquire));
        if sessions.len() >= 2 {
            return Err("Encerre uma captura antes de adicionar outra fonte.".into());
        }
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
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        sessions.insert(
            id.clone(),
            Arc::new(Session {
                shared,
                control: Mutex::new(Some(control)),
            }),
        );
        Ok(id)
    }

    fn get(&self, id: &str) -> Result<Arc<Session>, String> {
        self.sessions
            .lock()
            .map_err(|_| "A captura está indisponível.")?
            .get(id)
            .cloned()
            .ok_or_else(|| "A sessão de captura foi encerrada.".into())
    }

    fn stop(&self, id: &str) {
        let session = self
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
        if let Some(session) = session {
            session.stop();
        }
    }

    pub fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        let sessions =
            std::mem::take(&mut *self.sessions.lock().unwrap_or_else(|e| e.into_inner()));
        for session in sessions.into_values() {
            session.stop();
        }
    }
}

#[tauri::command]
pub async fn start_capture(
    kind: SourceKind,
    id: u32,
    state: tauri::State<'_, Arc<CaptureSessions>>,
) -> Result<String, String> {
    let sessions = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || sessions.start(kind, id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn capture_frame(
    session_id: String,
    after_sequence: u32,
    state: tauri::State<'_, Arc<CaptureSessions>>,
) -> Result<tauri::ipc::Response, String> {
    let session = state.get(&session_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        session.read(after_sequence).map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn stop_capture(
    session_id: String,
    state: tauri::State<'_, Arc<CaptureSessions>>,
) -> Result<(), String> {
    let sessions = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || sessions.stop(&session_id))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_packet_preserves_channels_and_aspect() {
        let pixels = [255, 12, 70, 255].repeat(1080 * 1920);
        let packet = prepare_packet(1080, 1920, &pixels, &mut Resizer::new()).unwrap();
        assert_eq!(u32::from_le_bytes(packet[4..8].try_into().unwrap()), 405);
        assert_eq!(u32::from_le_bytes(packet[8..12].try_into().unwrap()), 720);
        assert_eq!(&packet[12..16], &[255, 12, 70, 255]);
        assert_eq!(packet.len(), 12 + 405 * 720 * 4);
        assert_eq!(preview_size(320, 180), (320, 180));
    }

    #[test]
    fn latest_frame_drops_stale_frames_and_stop_wakes_reader() {
        let shared = Arc::new(SharedFrame::default());
        shared.publish(vec![0; 16]);
        shared.publish(vec![0; 20]);
        let packet = shared.read(0).unwrap();
        assert_eq!(u32::from_le_bytes(packet[..4].try_into().unwrap()), 2);
        assert_eq!(packet.len(), 20);
        let reader = shared.clone();
        let waiter = std::thread::spawn(move || reader.read(2));
        shared.finish("Encerrado");
        assert_eq!(waiter.join().unwrap().unwrap_err(), "Encerrado");
    }

    #[test]
    fn unchanged_frame_times_out_without_repeating_packet() {
        let shared = SharedFrame::default();
        shared.publish(vec![0; 16]);
        let _ = shared.read(0).unwrap();
        let started = Instant::now();
        assert!(shared.read(1).unwrap().is_empty());
        assert!(started.elapsed() >= FRAME_WAIT);
    }

    #[test]
    #[ignore = "Requer desktop interativo; mover conteúdo durante 5 segundos para medir FPS"]
    fn benchmark_real_monitor_stream() {
        let sessions = CaptureSessions::default();
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
        let id = sessions.start(kind, source_id).unwrap();
        let session = sessions.get(&id).unwrap();
        let start = Instant::now();
        let mut sequence = 0;
        let mut count = 0;
        while start.elapsed() < Duration::from_secs(5) {
            let packet = session.read(sequence).unwrap();
            if packet.is_empty() {
                continue;
            }
            sequence = u32::from_le_bytes(packet[..4].try_into().unwrap());
            let width = u32::from_le_bytes(packet[4..8].try_into().unwrap());
            let height = u32::from_le_bytes(packet[8..12].try_into().unwrap());
            assert_eq!(packet.len(), 12 + (width * height * 4) as usize);
            count += 1;
        }
        sessions.stop(&id);
        sessions.stop(&id);
        assert!(count > 0);
        eprintln!(
            "WGC: {count} quadros em {:.2}s ({:.1} FPS; conteúdo estático produz menos quadros)",
            start.elapsed().as_secs_f64(),
            count as f64 / start.elapsed().as_secs_f64()
        );
        eprintln!(
            "Callbacks WGC: {}; processamento médio: {:.2}ms/quadro",
            session.shared.callbacks.load(Ordering::Relaxed),
            session.shared.processing_micros.load(Ordering::Relaxed) as f64 / count as f64 / 1000.0
        );
    }
}
