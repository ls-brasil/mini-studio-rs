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
    active: Arc<AtomicBool>,
}

impl VirtualCamera {
    pub fn start(scene: Scene, ctx: egui::Context) -> Result<Self, String> {
        let scene = Arc::new(Mutex::new(Arc::new(scene)));
        let status = Arc::new(Mutex::new("Inicializando câmera virtual…".to_string()));
        let stopped = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicBool::new(false));
        let worker_active = active.clone();
        let (worker_scene, worker_status, worker_stop) =
            (scene.clone(), status.clone(), stopped.clone());
        let worker = thread::Builder::new()
            .name("mini-studio-virtual-camera".into())
            .spawn(move || {
                let result = run(
                    worker_scene,
                    worker_status.clone(),
                    &worker_stop,
                    &worker_active,
                    &ctx,
                );
                worker_active.store(false, Ordering::Release);
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
            active,
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
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Acquire) && !self.stopping()
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
pub fn installed() -> bool {
    installed_path().is_some_and(|p| p.is_file())
}
fn installed_path() -> Option<PathBuf> {
    use winreg::{
        RegKey,
        enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY},
    };
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SOFTWARE\Classes\CLSID\{F0C418C8-9211-479E-BAAE-3CBF90F15D37}\InprocServer32",
            KEY_READ | KEY_WOW64_64KEY,
        )
        .ok()?
        .get_value::<String, _>("")
        .ok()
        .map(PathBuf::from)
}

impl Session {
    fn open() -> Result<Self, String> {
        let path =
            installed_path().ok_or("Não foi possível localizar a instalação da câmera virtual.")?;
        unsafe {
            let library=Library::new(&path).map_err(|e|format!("Não foi possível carregar o componente. Repare a instalação do Mini Studio e verifique se o Windows é a versão 11. {e}"))?;
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
    active: &AtomicBool,
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
        if !active.swap(true, Ordering::AcqRel) {
            *status.lock().unwrap_or_else(|e| e.into_inner()) = "Publicando sua composição.".into();
            ctx.request_repaint();
        }
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
    #[ignore = "Requer componente instalado e FFmpeg; testa consumidor DirectShow em processo separado"]
    fn installed_virtual_camera_ffmpeg() {
        use crate::{
            camera,
            composition::{CameraPosition, ImageSettings, Pixels},
        };
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        assert!(installed());
        let scene = Scene {
            desktop: None,
            camera: None,
            image: Some(Pixels {
                width: 2,
                height: 2,
                rgba: [50, 200, 100, 255].repeat(4).into(),
            }),
            image_settings: ImageSettings::default(),
            mirror: false,
            camera_size: 25,
            camera_position: CameraPosition::BottomRight,
        };
        let output = VirtualCamera::start(scene, egui::Context::default()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let device = loop {
            assert!(!output.finished(), "{}", output.status());
            if let Some(device) = camera::list_all_cameras_for_test()
                .unwrap()
                .into_iter()
                .find(|d| d.name.starts_with(CAMERA_NAME))
            {
                break device;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(100));
        };
        std::fs::create_dir_all("target/qa").unwrap();
        let log = std::fs::File::create("target/qa/ffmpeg-vcam.log").unwrap();
        let mut consumer = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-nostdin",
                "-y",
                "-f",
                "dshow",
                "-i",
                &format!("video={}", device.name),
                "-t",
                "5",
                "-f",
                "framemd5",
                "target/qa/ffmpeg-vcam.framemd5",
            ])
            .creation_flags(0x08000000)
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("FFmpeg indisponível");
        let deadline = Instant::now() + Duration::from_secs(30);
        let status = loop {
            if let Some(status) = consumer.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                let _ = consumer.kill();
                let _ = consumer.wait();
                panic!("O FFmpeg não concluiu em 30 segundos.");
            }
            thread::sleep(Duration::from_millis(50));
        };
        let log = std::fs::read_to_string("target/qa/ffmpeg-vcam.log").unwrap();
        assert!(status.success(), "{log}");
        let frames = std::fs::read_to_string("target/qa/ffmpeg-vcam.framemd5")
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .count();
        assert!(frames >= 140, "Somente {frames} quadros recebidos. {log}");
        println!(
            "FFmpeg / DirectShow: {frames} quadros recebidos em 5 segundos; saída verificada por framemd5."
        );
        drop(output);
    }
    #[test]
    #[ignore = "Requer componente instalado; publica somente quadros sintéticos e consome a câmera virtual real"]
    fn installed_virtual_camera_round_trip() {
        use crate::{
            camera,
            composition::{CameraPosition, ImageSettings, Pixels},
        };
        assert!(
            installed(),
            "Instale o componente antes do teste de integração."
        );
        let make_scene = |color: [u8; 4]| Scene {
            desktop: None,
            camera: None,
            image: Some(Pixels {
                width: 2,
                height: 2,
                rgba: color.repeat(4).into(),
            }),
            image_settings: ImageSettings::default(),
            mirror: false,
            camera_size: 25,
            camera_position: CameraPosition::BottomRight,
        };
        let context = egui::Context::default();
        let output =
            VirtualCamera::start(make_scene([50, 200, 100, 255]), context.clone()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let device = loop {
            assert!(!output.finished(), "{}", output.status());
            if let Some(device) = camera::list_all_cameras_for_test()
                .unwrap()
                .into_iter()
                .find(|d| d.name.starts_with(CAMERA_NAME))
            {
                break device;
            }
            assert!(
                Instant::now() < deadline,
                "Dispositivo não apareceu: {}",
                output.status()
            );
            thread::sleep(Duration::from_millis(100));
        };
        assert!(
            camera::list_cameras()
                .unwrap()
                .iter()
                .all(|d| !d.name.starts_with(CAMERA_NAME))
        );
        println!("Dispositivo registrado: {}", device.name);
        let capture = camera::CameraCapture::start(device.id).unwrap();
        let seconds = std::env::var("MINISTUDIO_VCAM_TEST_SECONDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(10)
            .clamp(5, 600);
        let warmup = Instant::now();
        while capture.latest().unwrap().is_none() {
            assert!(
                warmup.elapsed() < Duration::from_secs(15),
                "Nenhum quadro após a ativação."
            );
            thread::sleep(Duration::from_millis(5));
        }
        println!(
            "Primeiro quadro após {:.2}s; iniciando medição de cadência.",
            warmup.elapsed().as_secs_f64()
        );
        let start = Instant::now();
        let mut frames = 0u64;
        let mut last_frame = Instant::now();
        let mut last_report = Instant::now();
        let mut green = false;
        let mut blue = false;
        let mut last_phase = 0;
        while start.elapsed() < Duration::from_secs(seconds) {
            assert!(!output.finished(), "{}", output.status());
            let phase = (start.elapsed().as_secs() / 2) % 2;
            if phase != last_phase {
                output.update(make_scene(if phase == 0 {
                    [50, 200, 100, 255]
                } else {
                    [70, 110, 220, 255]
                }));
                last_phase = phase;
            }
            if let Some(frame) = capture.latest().unwrap() {
                assert_eq!((frame.width, frame.height), (1280, 720));
                let index = ((frame.height / 2 * frame.width + frame.width / 2) * 4) as usize;
                let rgb = &frame.rgba[index..index + 3];
                green |= rgb[1] > 170 && rgb[0] < 90;
                blue |= rgb[2] > 190 && rgb[0] < 110;
                frames += 1;
                last_frame = Instant::now();
            }
            assert!(
                last_frame.elapsed() < Duration::from_secs(12),
                "A câmera virtual deixou de entregar quadros."
            );
            if last_report.elapsed() > Duration::from_secs(10) {
                println!(
                    "{} s: {} quadros; {:.2} FPS recebidos; {}",
                    start.elapsed().as_secs(),
                    frames,
                    frames as f64 / start.elapsed().as_secs_f64(),
                    output.status()
                );
                last_report = Instant::now();
            }
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            green && blue,
            "A mudança da composição não chegou ao consumidor."
        );
        let fps = frames as f64 / start.elapsed().as_secs_f64();
        assert!(
            fps > 24.0,
            "Cadência insuficiente: {fps:.2} FPS; {}",
            output.status()
        );
        drop(capture);
        drop(output);
        let deadline = Instant::now() + Duration::from_secs(5);
        while camera::list_all_cameras_for_test()
            .unwrap()
            .iter()
            .any(|d| d.name.starts_with(CAMERA_NAME))
        {
            assert!(
                Instant::now() < deadline,
                "Dispositivo permaneceu enumerável após parar."
            );
            thread::sleep(Duration::from_millis(100));
        }
        let restarted = VirtualCamera::start(make_scene([50, 200, 100, 255]), context).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !restarted.active() {
            assert!(!restarted.finished(), "{}", restarted.status());
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(50));
        }
        drop(restarted);
        println!(
            "Integração real: {frames} quadros em {seconds}s, {fps:.2} FPS; cores, troca de conteúdo, filtro, parada e reinício OK."
        );
    }
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
