//! Recepção pela ABI pública do NDI SDK, carregada apenas quando necessária.
//! Declarações C adaptadas dos headers MIT do SDK: docs/NDI-headers-LICENSE.txt.

use crate::{composition::Pixels, scenes::NdiSource};
use libloading::Library;
use std::{
    ffi::{CStr, CString, c_char, c_void},
    path::PathBuf,
    ptr,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const DLL: &str = "Processing.NDI.Lib.x64.dll";
const RGBX: u32 = u32::from_le_bytes(*b"RGBX");
const RGBA: u32 = u32::from_le_bytes(*b"RGBA");
const SIGNAL_TIMEOUT: Duration = Duration::from_secs(3);
type Instance = *mut c_void;
type Waker = Arc<dyn Fn() + Send + Sync>;

#[repr(C)]
struct Source {
    name: *const c_char,
    url: *const c_char,
}

#[repr(C)]
struct FindCreate {
    show_local_sources: bool,
    groups: *const c_char,
    extra_ips: *const c_char,
}

#[repr(C)]
struct RecvCreate {
    source: Source,
    color_format: i32,
    bandwidth: i32,
    allow_video_fields: bool,
    receiver_name: *const c_char,
}

#[repr(C)]
#[derive(Default)]
struct VideoFrame {
    width: i32,
    height: i32,
    fourcc: u32,
    frame_rate_n: i32,
    frame_rate_d: i32,
    aspect: f32,
    frame_format: i32,
    timecode: i64,
    data: *mut u8,
    stride: i32,
    metadata: *const c_char,
    timestamp: i64,
}

struct Api {
    find_create: unsafe extern "C" fn(*const FindCreate) -> Instance,
    find_destroy: unsafe extern "C" fn(Instance),
    find_wait: unsafe extern "C" fn(Instance, u32) -> bool,
    find_sources: unsafe extern "C" fn(Instance, *mut u32) -> *const Source,
    recv_create: unsafe extern "C" fn(*const RecvCreate) -> Instance,
    recv_destroy: unsafe extern "C" fn(Instance),
    recv_capture:
        unsafe extern "C" fn(Instance, *mut VideoFrame, *mut c_void, *mut c_void, u32) -> i32,
    recv_free_video: unsafe extern "C" fn(Instance, *const VideoFrame),
    _library: Library,
}

impl Api {
    fn shared() -> Result<Arc<Self>, String> {
        // O SDK é global ao processo. Mantemos uma inicialização e a DLL vivas até
        // a saída, inclusive enquanto threads canceladas liberam seus receptores.
        // Falhas não são cacheadas: instalar o runtime permite tentar novamente.
        static API: OnceLock<Mutex<Option<Arc<Api>>>> = OnceLock::new();
        let mut shared = API
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(api) = &*shared {
            return Ok(api.clone());
        }
        let api = Arc::new(Self::load()?);
        *shared = Some(api.clone());
        Ok(api)
    }

    fn load() -> Result<Self, String> {
        let mut last_error = None;
        for path in runtime_paths() {
            if !path.is_file() {
                continue;
            }
            match unsafe { Self::load_from(&path) } {
                Ok(api) => return Ok(api),
                Err(error) => last_error = Some(error),
            }
        }
        Err(match last_error {
            Some(error) => format!("Não foi possível iniciar o NDI Runtime x64. Reinstale o NDI Runtime 6. {error}"),
            None => "NDI Runtime não encontrado. Instale o NDI Runtime 6 para Windows x64 e reabra o Mini Studio.".into(),
        })
    }

    unsafe fn load_from(path: &std::path::Path) -> Result<Self, String> {
        // Só caminhos absolutos conhecidos; não procurar DLL no diretório corrente.
        let library = unsafe { Library::new(path) }.map_err(|e| e.to_string())?;
        unsafe {
            let api = Self {
                find_create: *library
                    .get(b"NDIlib_find_create_v2\0")
                    .map_err(|e| e.to_string())?,
                find_destroy: *library
                    .get(b"NDIlib_find_destroy\0")
                    .map_err(|e| e.to_string())?,
                find_wait: *library
                    .get(b"NDIlib_find_wait_for_sources\0")
                    .map_err(|e| e.to_string())?,
                find_sources: *library
                    .get(b"NDIlib_find_get_current_sources\0")
                    .map_err(|e| e.to_string())?,
                recv_create: *library
                    .get(b"NDIlib_recv_create_v3\0")
                    .map_err(|e| e.to_string())?,
                recv_destroy: *library
                    .get(b"NDIlib_recv_destroy\0")
                    .map_err(|e| e.to_string())?,
                recv_capture: *library
                    .get(b"NDIlib_recv_capture_v3\0")
                    .map_err(|e| e.to_string())?,
                recv_free_video: *library
                    .get(b"NDIlib_recv_free_video_v2\0")
                    .map_err(|e| e.to_string())?,
                _library: library,
            };
            let initialize = api
                ._library
                .get::<unsafe extern "C" fn() -> bool>(b"NDIlib_initialize\0")
                .map_err(|e| e.to_string())?;
            if !initialize() {
                return Err("A inicialização do SDK NDI falhou.".into());
            }
            Ok(api)
        }
    }
}

fn runtime_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for version in [6, 5] {
        if let Some(dir) = std::env::var_os(format!("NDI_RUNTIME_DIR_V{version}")) {
            paths.push(PathBuf::from(dir).join(DLL));
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        paths.push(dir.join(DLL));
    }
    if let Some(dir) = std::env::var_os("ProgramFiles") {
        let dir = PathBuf::from(dir);
        paths.push(dir.join("NDI/NDI 6 Runtime/v6").join(DLL));
        paths.push(dir.join("NDI/NDI 5 Runtime/v5").join(DLL));
    }
    if let Some(dir) = std::env::var_os("NDI_SDK_DIR") {
        paths.push(PathBuf::from(dir).join("Bin/x64").join(DLL));
    }
    paths.retain(|p| p.is_absolute());
    paths
}

struct Handle {
    raw: Instance,
    destroy: unsafe extern "C" fn(Instance),
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.raw) };
    }
}

pub fn list_sources() -> Result<Vec<NdiSource>, String> {
    let api = Api::shared()?;
    let settings = FindCreate {
        show_local_sources: true,
        groups: ptr::null(),
        extra_ips: ptr::null(),
    };
    let raw = unsafe { (api.find_create)(&settings) };
    if raw.is_null() {
        return Err("Não foi possível procurar canais NDI.".into());
    }
    let finder = Handle {
        raw,
        destroy: api.find_destroy,
    };
    // Uma janela limitada permite que anúncios mDNS de vários emissores cheguem.
    // Este trabalho roda no worker do seletor, nunca na thread da interface.
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        unsafe { (api.find_wait)(finder.raw, 200) };
        let mut count = 0;
        unsafe { (api.find_sources)(finder.raw, &mut count) };
    }
    let mut count = 0;
    let sources = unsafe { (api.find_sources)(finder.raw, &mut count) };
    if sources.is_null() || count == 0 {
        return Ok(Vec::new());
    }
    let mut result: Vec<_> = unsafe { std::slice::from_raw_parts(sources, count as usize) }
        .iter()
        .filter_map(|source| {
            if source.name.is_null() {
                return None;
            }
            let name = unsafe { CStr::from_ptr(source.name) }
                .to_string_lossy()
                .into_owned();
            (!name.is_empty()).then_some(NdiSource { name })
        })
        .collect();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result.dedup_by(|a, b| a.name == b.name);
    Ok(result)
}

pub struct Frame {
    pub pixels: Pixels,
    pub sequence: u64,
}

pub struct Update {
    pub frame: Option<Frame>,
    pub waiting: bool,
}

struct State {
    frame: Option<Frame>,
    waiting: bool,
    error: Option<String>,
    waker: Option<Waker>,
}

pub struct NdiCapture {
    state: Arc<Mutex<State>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl NdiCapture {
    pub fn start(source: NdiSource) -> Result<Self, String> {
        if source.name.trim().is_empty() {
            return Err("Escolha um canal NDI.".into());
        }
        let name = CString::new(source.name).map_err(|_| "Nome de canal NDI inválido.")?;
        let api = Api::shared()?;
        let state = Arc::new(Mutex::new(State {
            frame: None,
            waiting: true,
            error: None,
            waker: None,
        }));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_state = state.clone();
        let worker_stopped = stopped.clone();
        let worker = thread::Builder::new()
            .name("mini-studio-ndi".into())
            .spawn(move || {
                if let Err(error) = receive(&api, &name, &worker_state, &worker_stopped)
                    && !worker_stopped.load(Ordering::Acquire)
                {
                    let wake = {
                        let mut state = worker_state.lock().unwrap_or_else(|e| e.into_inner());
                        state.frame = None;
                        state.error = Some(error);
                        state.waker.clone()
                    };
                    if let Some(wake) = wake {
                        wake();
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            state,
            stopped,
            worker: Some(worker),
        })
    }

    pub fn set_waker(&self, waker: Waker) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).waker = Some(waker.clone());
        waker();
    }

    pub fn latest(&self) -> Result<Update, String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(error) = &state.error {
            return Err(error.clone());
        }
        Ok(Update {
            frame: state.frame.take(),
            waiting: state.waiting,
        })
    }
}

impl Drop for NdiCapture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.frame = None;
        state.waker = None;
        drop(state);
        // recv_capture tem timeout curto; a destruição nativa fica no worker.
        if let Some(worker) = self.worker.take()
            && worker.is_finished()
        {
            let _ = worker.join();
        }
    }
}

fn receive(
    api: &Api,
    name: &CStr,
    state: &Mutex<State>,
    stopped: &AtomicBool,
) -> Result<(), String> {
    let settings = RecvCreate {
        // Só o nome é persistido: o SDK redescobre o endereço e reconecta.
        source: Source {
            name: name.as_ptr(),
            url: ptr::null(),
        },
        color_format: 2,
        bandwidth: 100,
        allow_video_fields: false,
        receiver_name: c"Mini Studio".as_ptr(),
    };
    let raw = unsafe { (api.recv_create)(&settings) };
    if raw.is_null() {
        return Err("Não foi possível criar o receptor NDI.".into());
    }
    let receiver = Handle {
        raw,
        destroy: api.recv_destroy,
    };
    let mut last_frame = Instant::now();
    let mut sequence = 0;
    while !stopped.load(Ordering::Acquire) {
        let mut video = VideoFrame::default();
        let kind = unsafe {
            (api.recv_capture)(
                receiver.raw,
                &mut video,
                ptr::null_mut(),
                ptr::null_mut(),
                100,
            )
        };
        if kind == 1 {
            let pixels = unsafe { copy_video(&video) };
            // Libera também os frames recusados por formato/dimensões inválidos.
            unsafe { (api.recv_free_video)(receiver.raw, &video) };
            let pixels = pixels?;
            last_frame = Instant::now();
            sequence += 1;
            let wake = {
                let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                state.frame = Some(Frame { pixels, sequence });
                state.waiting = false;
                state.waker.clone()
            };
            if let Some(wake) = wake {
                wake();
            }
        } else {
            if last_frame.elapsed() >= SIGNAL_TIMEOUT {
                let wake = {
                    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
                    let changed = !state.waiting;
                    state.frame = None;
                    state.waiting = true;
                    changed.then(|| state.waker.clone()).flatten()
                };
                if let Some(wake) = wake {
                    wake();
                }
            }
            if kind == 4 {
                // Erros de conexão são transitórios. Evita busy loop sem destruir
                // o receptor que o SDK usa para reconectar automaticamente.
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

fn frame_layout(frame: &VideoFrame) -> Result<(usize, usize, isize), String> {
    if frame.width <= 0
        || frame.height <= 0
        || frame.width > 8192
        || frame.height > 8192
        || i64::from(frame.width) * i64::from(frame.height) > 16_777_216
    {
        return Err(
            "Resolução NDI inválida ou acima do limite de 16 milhões de pixels / 8192 por lado."
                .into(),
        );
    }
    if !matches!(frame.fourcc, RGBX | RGBA) {
        return Err(
            "O NDI Runtime entregou um formato de vídeo incompatível; esperado RGBX ou RGBA."
                .into(),
        );
    }
    let row = frame.width as usize * 4;
    let stride = if frame.stride == 0 {
        row as isize
    } else {
        frame.stride as isize
    };
    if stride.unsigned_abs() < row
        || stride.unsigned_abs() * frame.height as usize > 256 * 1024 * 1024
    {
        return Err("Espaçamento de linhas NDI inválido.".into());
    }
    if frame.data.is_null() {
        return Err("O NDI Runtime entregou um quadro sem pixels.".into());
    }
    Ok((row, row * frame.height as usize, stride))
}

/// `data` deve apontar para as linhas descritas pelo SDK, ainda não liberadas.
unsafe fn copy_video(frame: &VideoFrame) -> Result<Pixels, String> {
    let (row, len, stride) = frame_layout(frame)?;
    let mut rgba = vec![0; len];
    for (y, target) in rgba.chunks_exact_mut(row).enumerate() {
        // Copia somente pixels, sem ler padding após a última linha.
        let source =
            unsafe { std::slice::from_raw_parts(frame.data.offset(y as isize * stride), row) };
        target.copy_from_slice(source);
    }
    if frame.fourcc == RGBX {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
    }
    Ok(Pixels {
        width: frame.width as u32,
        height: frame.height as u32,
        rgba: rgba.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_loss_clears_output_and_recovers_without_replacing_receiver() {
        use crate::source_runtime::{RuntimeSource, Stream};
        let ctx = eframe::egui::Context::default();
        let state = Arc::new(Mutex::new(State {
            frame: None,
            waiting: true,
            error: None,
            waker: None,
        }));
        let stopped = Arc::new(AtomicBool::new(false));
        let capture = NdiCapture {
            state: state.clone(),
            stopped: stopped.clone(),
            worker: None,
        };
        let mut runtime = RuntimeSource::image(
            &ctx,
            Pixels {
                width: 1,
                height: 1,
                rgba: vec![255; 4].into(),
            },
        );
        runtime.stream = Stream::Ndi(capture, 0);
        assert!(runtime.poll(&ctx).0);
        assert!(runtime.pixels.is_none() && runtime.texture.is_none());
        assert!(runtime.waiting_ndi() && runtime.loading_now() && runtime.live());
        assert!(runtime.error.is_none());
        {
            let mut state = state.lock().unwrap();
            state.waiting = false;
            state.frame = Some(Frame {
                pixels: Pixels {
                    width: 2,
                    height: 1,
                    rgba: vec![255; 8].into(),
                },
                sequence: 4,
            });
        }
        let (changed, dropped, error) = runtime.poll(&ctx);
        assert!(changed && error.is_none());
        assert_eq!(dropped, 3);
        assert_eq!(runtime.texture.as_ref().unwrap().size(), [2, 1]);
        assert!(!runtime.waiting_ndi());
        drop(runtime);
        assert!(stopped.load(Ordering::Acquire));
    }

    #[test]
    #[ignore = "Requer NDI Runtime x64; anuncia vídeo sintético na rede local e valida descoberta, recepção e reconexão"]
    fn installed_ndi_video_round_trip() {
        use crate::source_runtime::{RuntimeSource, Stream};
        #[repr(C)]
        struct SendCreate {
            name: *const c_char,
            groups: *const c_char,
            clock_video: bool,
            clock_audio: bool,
        }
        struct Sender<'a> {
            handle: Handle,
            api: &'a Api,
        }
        impl<'a> Sender<'a> {
            fn new(api: &'a Api, name: &CStr) -> Self {
                unsafe {
                    let create = api
                        ._library
                        .get::<unsafe extern "C" fn(*const SendCreate) -> Instance>(
                            b"NDIlib_send_create\0",
                        )
                        .unwrap();
                    let destroy = *api
                        ._library
                        .get::<unsafe extern "C" fn(Instance)>(b"NDIlib_send_destroy\0")
                        .unwrap();
                    let raw = create(&SendCreate {
                        name: name.as_ptr(),
                        groups: ptr::null(),
                        clock_video: false,
                        clock_audio: false,
                    });
                    assert!(!raw.is_null());
                    Self {
                        handle: Handle { raw, destroy },
                        api,
                    }
                }
            }
            fn send(&self, width: i32, height: i32, color: [u8; 4]) {
                let mut data = color.repeat((width * height) as usize);
                let frame = VideoFrame {
                    width,
                    height,
                    fourcc: RGBA,
                    frame_rate_n: 30,
                    frame_rate_d: 1,
                    aspect: width as f32 / height as f32,
                    frame_format: 1,
                    timecode: i64::MAX,
                    data: data.as_mut_ptr(),
                    stride: width * 4,
                    ..Default::default()
                };
                unsafe {
                    let send = self
                        .api
                        ._library
                        .get::<unsafe extern "C" fn(Instance, *const VideoFrame)>(
                            b"NDIlib_send_send_video_v2\0",
                        )
                        .unwrap();
                    send(self.handle.raw, &frame);
                }
            }
        }
        fn wait_video(
            sender: &Sender<'_>,
            runtime: &mut RuntimeSource,
            ctx: &eframe::egui::Context,
            size: (i32, i32),
            color: [u8; 4],
        ) {
            let deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < deadline {
                sender.send(size.0, size.1, color);
                let (_, _, error) = runtime.poll(ctx);
                assert!(error.is_none(), "{error:?}");
                if let Some(pixels) = &runtime.pixels
                    && (pixels.width, pixels.height) == (size.0 as u32, size.1 as u32)
                    && pixels
                        .rgba
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|p| p.iter().zip(color).all(|(a, b)| a.abs_diff(b) <= 8))
                {
                    assert_eq!(
                        runtime.texture.as_ref().unwrap().size(),
                        [size.0 as usize, size.1 as usize]
                    );
                    assert!(!runtime.waiting_ndi());
                    return;
                }
                thread::sleep(Duration::from_millis(33));
            }
            panic!(
                "Não recebeu pixels NDI esperados: {}x{} {color:?}",
                size.0, size.1
            );
        }
        let api = Api::shared().expect("Instale o NDI Runtime para este teste opt-in.");
        let name = CString::new(format!("Mini Studio teste {}", std::process::id())).unwrap();
        let sender = Sender::new(&api, &name);
        let deadline = Instant::now() + Duration::from_secs(12);
        let source = loop {
            if let Some(source) = list_sources()
                .unwrap()
                .into_iter()
                .find(|s| s.name.ends_with(&format!("({})", name.to_str().unwrap())))
            {
                break source;
            }
            assert!(
                Instant::now() < deadline,
                "Emissor de teste não foi descoberto."
            );
        };
        println!("Canal descoberto: {}", source.name);
        let ctx = eframe::egui::Context::default();
        let mut runtime = RuntimeSource::loading(&ctx, crate::scenes::SourceInput::Ndi(source));
        wait_video(&sender, &mut runtime, &ctx, (320, 180), [220, 40, 70, 255]);
        wait_video(&sender, &mut runtime, &ctx, (640, 360), [30, 200, 90, 128]);
        println!("Vídeo, cores, alpha e mudança de resolução recebidos.");
        drop(sender);
        let deadline = Instant::now() + Duration::from_secs(12);
        while !runtime.waiting_ndi() && Instant::now() < deadline {
            assert!(runtime.poll(&ctx).2.is_none());
            thread::sleep(Duration::from_millis(50));
        }
        assert!(runtime.waiting_ndi(), "Perda de sinal não foi detectada.");
        assert!(runtime.pixels.is_none() && runtime.texture.is_none());
        let sender = Sender::new(&api, &name);
        wait_video(&sender, &mut runtime, &ctx, (320, 180), [20, 70, 220, 255]);
        println!("Reconectou após destruir e recriar o emissor com o mesmo nome.");
        let stream = std::mem::replace(&mut runtime.stream, Stream::Offline);
        let Stream::Ndi(mut capture, _) = stream else {
            panic!("Receptor foi substituído");
        };
        let worker = capture.worker.take().unwrap();
        let start = Instant::now();
        drop(capture);
        worker.join().unwrap();
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "Encerramento do receptor demorou demais."
        );
        println!("Receptor encerrado em {:?}.", start.elapsed());
    }

    #[test]
    fn c_abi_matches_ndi_x64_headers() {
        use std::mem::{offset_of, size_of};
        assert_eq!(size_of::<Source>(), 16);
        assert_eq!(size_of::<FindCreate>(), 24);
        assert_eq!(size_of::<RecvCreate>(), 40);
        assert_eq!(size_of::<VideoFrame>(), 72);
        assert_eq!(offset_of!(VideoFrame, timecode), 32);
        assert_eq!(offset_of!(VideoFrame, data), 40);
        assert_eq!(offset_of!(VideoFrame, stride), 48);
        assert_eq!(offset_of!(VideoFrame, metadata), 56);
        assert_eq!(offset_of!(VideoFrame, timestamp), 64);
    }

    #[test]
    fn copies_padding_alpha_and_negative_stride() {
        let mut data = [10, 20, 30, 40, 99, 99, 99, 99, 50, 60, 70, 80];
        let mut frame = VideoFrame {
            width: 1,
            height: 2,
            fourcc: RGBA,
            stride: 8,
            data: data.as_mut_ptr(),
            ..Default::default()
        };
        assert_eq!(
            &*unsafe { copy_video(&frame) }.unwrap().rgba,
            &[10, 20, 30, 40, 50, 60, 70, 80]
        );
        frame.fourcc = RGBX;
        assert_eq!(
            &*unsafe { copy_video(&frame) }.unwrap().rgba,
            &[10, 20, 30, 255, 50, 60, 70, 255]
        );
        frame.data = unsafe { data.as_mut_ptr().add(8) };
        frame.stride = -8;
        assert_eq!(
            &*unsafe { copy_video(&frame) }.unwrap().rgba,
            &[50, 60, 70, 255, 10, 20, 30, 255]
        );
        frame.data = data.as_mut_ptr();
        frame.stride = 0;
        frame.height = 1;
        assert_eq!(
            &*unsafe { copy_video(&frame) }.unwrap().rgba,
            &[10, 20, 30, 255]
        );
    }

    #[test]
    fn rejects_invalid_frames_before_dereferencing() {
        let mut frame = VideoFrame {
            width: 1920,
            height: 1080,
            fourcc: RGBA,
            ..Default::default()
        };
        assert!(frame_layout(&frame).is_err());
        frame.data = ptr::dangling_mut();
        for (width, height, stride, fourcc) in [
            (0, 1080, 0, RGBA),
            (1920, -1, 0, RGBA),
            (8192, 8192, 0, RGBA),
            (1920, 1080, 4, RGBA),
            (1920, 1080, i32::MAX, RGBA),
            (1920, 1080, 0, 0),
        ] {
            frame.width = width;
            frame.height = height;
            frame.stride = stride;
            frame.fourcc = fourcc;
            assert!(frame_layout(&frame).is_err());
        }
    }
}
