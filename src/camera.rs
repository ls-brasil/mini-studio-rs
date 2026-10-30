use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError, SyncSender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Media::MediaFoundation::*,
        System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize},
    },
    core::{GUID, HRESULT, Interface, PWSTR, Ref, implement},
};

#[derive(Clone, Debug)]
pub struct CameraDevice {
    pub id: String,
    pub name: String,
}

pub struct CameraFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub sequence: u64,
}

type Waker = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone, Copy, Default)]
struct VideoFormat {
    width: u32,
    height: u32,
    stride: i32,
}

#[derive(Default)]
struct CameraState {
    frame: Option<CameraFrame>,
    error: Option<String>,
    sequence: u64,
    format: VideoFormat,
    waker: Option<Waker>,
}

pub struct CameraCapture {
    state: Arc<Mutex<CameraState>>,
    stopped: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl CameraCapture {
    pub fn start(id: String) -> Result<Self, String> {
        let state = Arc::new(Mutex::new(CameraState::default()));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_state = state.clone();
        let worker_stopped = stopped.clone();
        let worker = thread::Builder::new()
            .name("mini-studio-camera".into())
            .spawn(move || {
                if let Err(error) = run_camera(&id, worker_state.clone(), worker_stopped.clone())
                    && !worker_stopped.load(Ordering::Acquire)
                {
                    let waker = {
                        let mut state = worker_state.lock().unwrap_or_else(|e| e.into_inner());
                        state.frame = None;
                        state.error = Some(format!(
                            "Não foi possível capturar a câmera. Verifique a conexão e o acesso em Configurações do Windows → Privacidade e segurança → Câmera. {error}"
                        ));
                        state.waker.clone()
                    };
                    if let Some(wake) = waker {
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
        let ready = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.waker = Some(waker.clone());
            state.frame.is_some() || state.error.is_some()
        };
        if ready {
            waker();
        }
    }

    /// Retira o quadro mais recente; quadros anteriores são descartados pelo produtor.
    pub fn latest(&self) -> Result<Option<CameraFrame>, String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(error) = &state.error {
            return Err(error.clone());
        }
        Ok(state.frame.take())
    }

    pub fn stop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.frame = None;
        state.waker = None;
        drop(state);
        // Flush/Shutdown e a destruição COM ficam na thread proprietária, nunca na UI.
        if let Some(worker) = self.worker.take()
            && worker.is_finished()
        {
            let _ = worker.join();
        }
    }
}

impl Drop for CameraCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

struct MediaFoundation;

impl MediaFoundation {
    fn start() -> Result<Self, String> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|e| e.to_string())?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(error.to_string());
            }
        }
        Ok(Self)
    }
}

impl Drop for MediaFoundation {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

fn attributes(capacity: u32) -> windows::core::Result<IMFAttributes> {
    let mut result = None;
    unsafe {
        MFCreateAttributes(&mut result, capacity)?;
    }
    result.ok_or_else(windows::core::Error::from_thread)
}

fn devices() -> windows::core::Result<Vec<IMFActivate>> {
    let attributes = attributes(1)?;
    unsafe {
        attributes.SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )?;
        let mut pointer = std::ptr::null_mut();
        let mut count = 0;
        MFEnumDeviceSources(&attributes, &mut pointer, &mut count)?;
        if pointer.is_null() {
            return Ok(Vec::new());
        }
        let result = std::slice::from_raw_parts_mut(pointer, count as usize)
            .iter_mut()
            .filter_map(Option::take)
            .collect();
        CoTaskMemFree(Some(pointer.cast()));
        Ok(result)
    }
}

fn device_string(device: &IMFActivate, key: &GUID) -> windows::core::Result<String> {
    let mut pointer = PWSTR::null();
    let mut length = 0;
    unsafe {
        device.GetAllocatedString(key, &mut pointer, &mut length)?;
        if pointer.is_null() {
            return Ok(String::new());
        }
        let value =
            String::from_utf16_lossy(std::slice::from_raw_parts(pointer.0, length as usize));
        CoTaskMemFree(Some(pointer.0.cast()));
        Ok(value)
    }
}

pub fn list_cameras() -> Result<Vec<CameraDevice>, String> {
    let _runtime = MediaFoundation::start()?;
    devices()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|device| {
            Ok(CameraDevice {
                id: device_string(
                    device,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                )
                .map_err(|e| e.to_string())?,
                name: device_string(device, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
                    .map_err(|e| e.to_string())?,
            })
        })
        .collect()
}

fn run_camera(
    id: &str,
    state: Arc<Mutex<CameraState>>,
    stopped: Arc<AtomicBool>,
) -> Result<(), String> {
    let _runtime = MediaFoundation::start()?;
    let activation = devices()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|device| {
            device_string(
                device,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            )
            .is_ok_and(|value| value == id)
        })
        .ok_or_else(|| "A câmera selecionada não está mais disponível.".to_owned())?;
    if stopped.load(Ordering::Acquire) {
        return Ok(());
    }
    let source: IMFMediaSource =
        unsafe { activation.ActivateObject() }.map_err(|e| e.to_string())?;
    let result = read_camera(&source, state, stopped);
    unsafe {
        let _ = source.Shutdown();
    }
    result
}

fn read_camera(
    source: &IMFMediaSource,
    state: Arc<Mutex<CameraState>>,
    stopped: Arc<AtomicBool>,
) -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let callback: IMFSourceReaderCallback = ReaderCallback {
        state: state.clone(),
        stopped: stopped.clone(),
        sender,
    }
    .into();
    let attributes = attributes(3).map_err(|e| e.to_string())?;
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    let reader = unsafe {
        attributes
            .SetUnknown(&MF_SOURCE_READER_ASYNC_CALLBACK, &callback)
            .map_err(|e| e.to_string())?;
        attributes
            .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
            .map_err(|e| e.to_string())?;
        attributes
            .SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)
            .map_err(|e| e.to_string())?;
        MFCreateSourceReaderFromMediaSource(source, &attributes).map_err(|e| e.to_string())?
    };
    let format = configure_reader(&reader).map_err(|e| e.to_string())?;
    state.lock().unwrap_or_else(|e| e.into_inner()).format = format;
    let result = (|| {
        while !stopped.load(Ordering::Acquire) {
            unsafe { reader.ReadSample(stream, 0, None, None, None, None) }
                .map_err(|e| e.to_string())?;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if stopped.load(Ordering::Acquire) {
                    return Ok(());
                }
                match receiver.recv_timeout(Duration::from_millis(50)) {
                    Ok(result) => {
                        result?;
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
                    Err(RecvTimeoutError::Timeout) => {
                        return Err("A câmera ficou sem enviar imagens por 5 segundos.".into());
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        return Err("A conexão com a câmera foi encerrada.".into());
                    }
                }
            }
        }
        Ok(())
    })();
    unsafe {
        let _ = reader.Flush(stream);
    }
    result
}

fn configure_reader(reader: &IMFSourceReader) -> windows::core::Result<VideoFormat> {
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    unsafe {
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
        reader.SetStreamSelection(stream, true)?;
        let mut candidates = Vec::new();
        for index in 0..256 {
            let Ok(media_type) = reader.GetNativeMediaType(stream, index) else {
                break;
            };
            let Ok(size) = media_type.GetUINT64(&MF_MT_FRAME_SIZE) else {
                continue;
            };
            let width = (size >> 32) as u32;
            let height = size as u32;
            if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 3840 * 2160 {
                continue;
            }
            let rate = media_type
                .GetUINT64(&MF_MT_FRAME_RATE)
                .unwrap_or((30_u64 << 32) | 1);
            let fps = ((rate >> 32) as f64 / (rate as u32).max(1) as f64 * 1000.0).round() as u32;
            let score = (
                width > 1280 || height > 720,
                fps.abs_diff(30_000),
                (width * height).abs_diff(1280 * 720),
            );
            candidates.push((score, media_type));
        }
        candidates.sort_by_key(|(score, _)| *score);
        for (_, media_type) in candidates {
            if reader
                .SetCurrentMediaType(stream, None, &media_type)
                .is_ok()
            {
                break;
            }
        }
        let output = MFCreateMediaType()?;
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        reader.SetCurrentMediaType(stream, None, &output)?;
        let current = reader.GetCurrentMediaType(stream)?;
        let size = current.GetUINT64(&MF_MT_FRAME_SIZE)?;
        let width = (size >> 32) as u32;
        let height = size as u32;
        let stride = current
            .GetUINT32(&MF_MT_DEFAULT_STRIDE)
            .unwrap_or(width * 4) as i32;
        Ok(VideoFormat {
            width,
            height,
            stride,
        })
    }
}

#[implement(IMFSourceReaderCallback)]
struct ReaderCallback {
    state: Arc<Mutex<CameraState>>,
    stopped: Arc<AtomicBool>,
    sender: SyncSender<Result<(), String>>,
}

#[allow(non_snake_case)]
impl IMFSourceReaderCallback_Impl for ReaderCallback_Impl {
    fn OnReadSample(
        &self,
        status: HRESULT,
        _stream: u32,
        flags: u32,
        _timestamp: i64,
        sample: Ref<IMFSample>,
    ) -> windows::core::Result<()> {
        if self.stopped.load(Ordering::Acquire) {
            return Ok(());
        }
        let result = (|| {
            status.ok().map_err(|e| e.to_string())?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Err("A câmera encerrou o envio de imagens.".into());
            }
            let Some(sample) = sample.as_ref() else {
                return Ok(());
            };
            let format = self.state.lock().unwrap_or_else(|e| e.into_inner()).format;
            let rgba = sample_rgba(sample, format)?;
            let waker = {
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                if self.stopped.load(Ordering::Acquire) {
                    return Ok(());
                }
                state.sequence += 1;
                state.frame = Some(CameraFrame {
                    width: format.width,
                    height: format.height,
                    rgba,
                    sequence: state.sequence,
                });
                state.waker.clone()
            };
            if let Some(wake) = waker {
                wake();
            }
            Ok(())
        })();
        let _ = self.sender.try_send(result);
        Ok(())
    }

    fn OnFlush(&self, _stream: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn OnEvent(&self, _stream: u32, _event: Ref<IMFMediaEvent>) -> windows::core::Result<()> {
        Ok(())
    }
}

fn sample_rgba(sample: &IMFSample, format: VideoFormat) -> Result<Vec<u8>, String> {
    unsafe {
        let buffer = sample
            .ConvertToContiguousBuffer()
            .map_err(|e| e.to_string())?;
        if let Ok(surface) = buffer.cast::<IMF2DBuffer2>() {
            let mut scanline = std::ptr::null_mut();
            let mut start = std::ptr::null_mut();
            let mut stride = 0;
            let mut length = 0;
            surface
                .Lock2DSize(
                    MF2DBuffer_LockFlags_Read,
                    &mut scanline,
                    &mut stride,
                    &mut start,
                    &mut length,
                )
                .map_err(|e| e.to_string())?;
            let result = copy_bgra(
                scanline,
                stride,
                start,
                length as usize,
                format.width,
                format.height,
            );
            let _ = surface.Unlock2D();
            return result;
        }
        let mut start = std::ptr::null_mut();
        let mut length = 0;
        buffer
            .Lock(&mut start, None, Some(&mut length))
            .map_err(|e| e.to_string())?;
        let first_offset = if format.stride < 0 {
            (format.height.saturating_sub(1) as usize) * format.stride.unsigned_abs() as usize
        } else {
            0
        };
        let scanline = start.wrapping_add(first_offset);
        let result = copy_bgra(
            scanline,
            format.stride,
            start,
            length as usize,
            format.width,
            format.height,
        );
        let _ = buffer.Unlock();
        result
    }
}

// Os limites de cada linha são validados antes de ler o buffer COM bloqueado.
unsafe fn copy_bgra(
    scanline: *const u8,
    stride: i32,
    start: *const u8,
    length: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, String> {
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || pixels > 3840 * 2160 || scanline.is_null() || start.is_null() {
        return Err("A câmera retornou dimensões ou buffer inválidos.".into());
    }
    let row_bytes = width as usize * 4;
    if (stride.unsigned_abs() as usize) < row_bytes {
        return Err("A câmera retornou passo de linha inválido.".into());
    }
    let begin = start as usize;
    let end = begin
        .checked_add(length)
        .ok_or("Buffer da câmera inválido.")?;
    let mut rgba = vec![0; pixels as usize * 4];
    for y in 0..height as usize {
        let address = (scanline as usize)
            .checked_add_signed(y as isize * stride as isize)
            .ok_or("Linha da câmera inválida.")?;
        if address < begin
            || address
                .checked_add(row_bytes)
                .is_none_or(|value| value > end)
        {
            return Err("O buffer da câmera não contém todos os pixels esperados.".into());
        }
        let source = unsafe { std::slice::from_raw_parts(address as *const u8, row_bytes) };
        let target = &mut rgba[y * row_bytes..(y + 1) * row_bytes];
        for (bgra, output) in source
            .as_chunks::<4>()
            .0
            .iter()
            .zip(target.as_chunks_mut::<4>().0.iter_mut())
        {
            output.copy_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
        }
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_rows_preserve_colors_and_handle_negative_stride() {
        let bytes = [30, 20, 10, 0, 0, 0, 0, 0, 60, 50, 40, 0, 0, 0, 0, 0];
        let rgba =
            unsafe { copy_bgra(bytes.as_ptr().add(8), -8, bytes.as_ptr(), bytes.len(), 1, 2) }
                .unwrap();
        assert_eq!(rgba, [40, 50, 60, 255, 10, 20, 30, 255]);
        assert!(unsafe { copy_bgra(bytes.as_ptr(), 8, bytes.as_ptr(), 10, 1, 2) }.is_err());
    }

    #[test]
    #[ignore = "Requer dispositivos de câmera locais"]
    fn enumerate_real_cameras() {
        for camera in list_cameras().unwrap() {
            eprintln!("Câmera: {}", camera.name);
        }
    }

    #[test]
    #[ignore = "Abre a primeira câmera local para validar captura e encerramento"]
    fn capture_real_camera_and_stop() {
        let device = list_cameras()
            .unwrap()
            .into_iter()
            .next()
            .expect("Nenhuma câmera disponível");
        let mut camera = CameraCapture::start(device.id).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut count = 0;
        let mut started = None;
        while Instant::now() < deadline {
            if let Some(frame) = camera.latest().unwrap() {
                assert_eq!(frame.rgba.len(), (frame.width * frame.height * 4) as usize);
                count += 1;
                if count == 1 {
                    eprintln!("Câmera nativa: {} × {}", frame.width, frame.height);
                    started = Some(Instant::now());
                }
                if started.is_some_and(|time| time.elapsed() >= Duration::from_secs(2)) {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(3));
        }
        assert!(count > 0, "A câmera não enviou imagens");
        let elapsed = started.unwrap().elapsed().as_secs_f64();
        eprintln!(
            "Câmera nativa: {} quadros / {:.2}s = {:.1} FPS",
            count,
            elapsed,
            (count - 1) as f64 / elapsed
        );
        let worker = camera.worker.take().unwrap();
        camera.stop();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !worker.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            worker.is_finished(),
            "A thread da câmera não encerrou em 3 segundos"
        );
        worker.join().unwrap();
    }
}
