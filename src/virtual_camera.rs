use crate::composition::{HEIGHT, Scene, WIDTH};
use eframe::egui;
use libloading::Library;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

pub const CAMERA_NAME: &str = "Mini Studio RS Cam";
const PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 30);

pub struct VirtualCamera {
    scene: Arc<Mutex<Arc<Scene>>>,
    status: Arc<Mutex<String>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl VirtualCamera {
    pub fn start(scene: Scene, ctx: egui::Context) -> Result<Self, String> {
        let scene = Arc::new(Mutex::new(Arc::new(scene)));
        let status = Arc::new(Mutex::new("Inicializando câmera virtual…".to_string()));
        let stopped = Arc::new(AtomicBool::new(false));
        let (worker_scene, worker_status, worker_stop) =
            (scene.clone(), status.clone(), stopped.clone());
        let worker = thread::Builder::new()
            .name("mini-studio-virtual-camera".into())
            .spawn(move || {
                let result = run(worker_scene, worker_status.clone(), &worker_stop, &ctx);
                *worker_status.lock().unwrap_or_else(|e| e.into_inner()) = match result {
                    Ok(()) => "Câmera virtual parada.".into(),
                    Err(error) => format!("Câmera virtual indisponível: {error}"),
                };
                ctx.request_repaint();
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            scene,
            status,
            stopped,
            worker: Some(worker),
        })
    }
    pub fn update(&self, scene: Scene) {
        let mut current = self.scene.lock().unwrap_or_else(|e| e.into_inner());
        if !current.same_content(&scene) {
            *current = Arc::new(scene);
        }
    }
    pub fn status(&self) -> String {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub fn stopping(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        if !self.finished() {
            *self.status.lock().unwrap_or_else(|e| e.into_inner()) =
                "Parando câmera virtual…".into();
        }
        if let Some(worker) = &self.worker {
            worker.thread().unpark();
        }
    }
}
impl Drop for VirtualCamera {
    fn drop(&mut self) {
        self.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

type Start = unsafe extern "system" fn() -> i32;
type Publish = unsafe extern "system" fn(*const u8, u32) -> i32;
type Stop = unsafe extern "system" fn() -> i32;

struct Session {
    _library: Library,
    publish: Publish,
    stop: Stop,
}
impl Session {
    fn open() -> Result<Self, String> {
        let base = std::env::var_os("ProgramW6432")
            .or_else(|| std::env::var_os("ProgramFiles"))
            .ok_or("Pasta Program Files não encontrada.")?;
        let path = PathBuf::from(base)
            .join("MiniStudioRS")
            .join("MiniStudioRSVCam.dll");
        unsafe {
            let library=Library::new(&path).map_err(|e|format!("Requer Windows 11 e o componente instalado. Execute scripts/install-vcam.ps1 como administrador. {e}"))?;
            let start: Start = *library
                .get(b"MSVCam_StartSession\0")
                .map_err(|e| e.to_string())?;
            let publish: Publish = *library
                .get(b"MSVCam_Publish\0")
                .map_err(|e| e.to_string())?;
            let stop: Stop = *library
                .get(b"MSVCam_StopSession\0")
                .map_err(|e| e.to_string())?;
            let session = Self {
                _library: library,
                publish,
                stop,
            };
            let hr = start();
            if hr < 0 {
                return Err(format!(
                    "Não foi possível iniciar (0x{:08X}). Verifique a instalação, as permissões de câmera e se outra instância está publicando.",
                    hr as u32
                ));
            }
            Ok(session)
        }
    }
    fn publish(&self, nv12: &[u8]) -> Result<(), String> {
        let hr = unsafe { (self.publish)(nv12.as_ptr(), nv12.len() as u32) };
        if hr < 0 {
            Err(format!("Falha ao publicar quadro (0x{:08X}).", hr as u32))
        } else {
            Ok(())
        }
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            (self.stop)();
        }
    }
}
struct Com;
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn run(
    scene: Arc<Mutex<Arc<Scene>>>,
    status: Arc<Mutex<String>>,
    stopped: &AtomicBool,
    ctx: &egui::Context,
) -> Result<(), String> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
    }
    let _com = Com;
    if stopped.load(Ordering::Acquire) {
        return Ok(());
    }
    let session = Session::open()?;
    let mut last: Option<Arc<Scene>> = None;
    let mut nv12 = vec![0; (WIDTH * HEIGHT * 3 / 2) as usize];
    let mut deadline = Instant::now();
    let mut stats_at = deadline;
    let mut frames = 0;
    while !stopped.load(Ordering::Acquire) {
        let current = scene.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if last
            .as_ref()
            .is_none_or(|last| !Arc::ptr_eq(last, &current))
        {
            rgba_to_nv12(
                current.compose()?.as_raw(),
                WIDTH as usize,
                HEIGHT as usize,
                &mut nv12,
            )?;
            last = Some(current);
        }
        session.publish(&nv12)?;
        frames += 1;
        if stats_at.elapsed() >= Duration::from_secs(1) {
            *status.lock().unwrap_or_else(|e| e.into_inner()) = format!(
                "Câmera virtual ativa · {:.1} FPS publicados",
                frames as f64 / stats_at.elapsed().as_secs_f64()
            );
            frames = 0;
            stats_at = Instant::now();
            ctx.request_repaint();
        }
        deadline += PERIOD;
        let now = Instant::now();
        if deadline <= now {
            deadline = now + PERIOD;
        }
        thread::park_timeout(deadline.saturating_duration_since(now));
    }
    Ok(())
}

// BT.709, faixa limitada; média de croma por bloco 2 × 2, como anunciado pela DLL.
fn rgba_to_nv12(rgba: &[u8], width: usize, height: usize, out: &mut [u8]) -> Result<(), String> {
    let pixels = width.checked_mul(height).ok_or("Dimensões inválidas.")?;
    if width == 0
        || height == 0
        || !width.is_multiple_of(2)
        || !height.is_multiple_of(2)
        || pixels.checked_mul(4) != Some(rgba.len())
        || pixels.checked_mul(3).map(|n| n / 2) != Some(out.len())
    {
        return Err("Quadro inválido para conversão NV12.".into());
    }
    for (pixel, y) in rgba.as_chunks::<4>().0.iter().zip(out[..pixels].iter_mut()) {
        *y =
            (((11966 * pixel[0] as i32 + 40254 * pixel[1] as i32 + 4064 * pixel[2] as i32 + 32768)
                >> 16)
                + 16)
                .clamp(16, 235) as u8;
    }
    for y in (0..height).step_by(2) {
        for x in (0..width).step_by(2) {
            let (mut r, mut g, mut b) = (0, 0, 0);
            for dy in 0..2 {
                for dx in 0..2 {
                    let i = ((y + dy) * width + x + dx) * 4;
                    r += rgba[i] as i32;
                    g += rgba[i + 1] as i32;
                    b += rgba[i + 2] as i32;
                }
            }
            let i = pixels + (y / 2) * width + x;
            out[i] =
                (((-6596 * r - 22188 * g + 28784 * b + 131072) >> 18) + 128).clamp(16, 240) as u8;
            out[i + 1] =
                (((28784 * r - 26145 * g - 2639 * b + 131072) >> 18) + 128).clamp(16, 240) as u8;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nv12_range_and_chroma() {
        for (rgb, expected) in [
            ([0, 0, 0, 255], [16, 128, 128]),
            ([255, 255, 255, 255], [235, 128, 128]),
            ([255, 0, 0, 255], [63, 102, 240]),
            ([0, 0, 255, 255], [32, 240, 118]),
        ] {
            let mut output = vec![0; 6];
            rgba_to_nv12(&rgb.repeat(4), 2, 2, &mut output).unwrap();
            assert_eq!([output[0], output[4], output[5]], expected);
        }
        assert!(rgba_to_nv12(&[0; 4], 1, 1, &mut [0; 1]).is_err());
    }
}
