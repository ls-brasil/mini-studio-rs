use crate::{
    camera::CameraCapture,
    composition::Pixels,
    image_source, load_texture,
    native_capture::{self, CaptureSession, SourceKind},
    ndi::NdiCapture,
    scenes::SourceInput,
};
use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

pub struct Pending {
    receiver: Receiver<Result<Ready, String>>,
    cancelled: Arc<AtomicBool>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
pub enum Stream {
    Image,
    Desktop(CaptureSession, u32),
    Camera(CameraCapture, u64),
    Ndi(NdiCapture, u64),
    Loading(Pending),
    Offline,
}
struct Ready {
    pixels: Option<Pixels>,
    stream: Stream,
}

#[derive(Clone, Default)]
pub(crate) struct RetryState {
    input: Option<SourceInput>,
    next: Option<Instant>,
    delay: Duration,
}

pub struct RuntimeSource {
    pub(crate) retry: RetryState,
    pub pixels: Option<Pixels>,
    pub texture: Option<TextureHandle>,
    pub stream: Stream,
    pub error: Option<String>,
}
impl RuntimeSource {
    pub fn image(ctx: &Context, pixels: Pixels) -> Self {
        let texture = load_texture(ctx, "source", pixels.width, pixels.height, &pixels.rgba);
        Self {
            pixels: Some(pixels),
            texture: Some(texture),
            stream: Stream::Image,
            error: None,
            retry: RetryState::default(),
        }
    }
    pub fn loading(ctx: &Context, input: SourceInput) -> Self {
        let retry_input = input.clone();
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = cancelled.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| match input {
                SourceInput::Image(path) => Ok(Ready {
                    pixels: Some(image_source::load(&path)?.pixels),
                    stream: Stream::Image,
                }),
                SourceInput::Desktop(saved) => {
                    let kind = if saved.window {
                        SourceKind::Window
                    } else {
                        SourceKind::Screen
                    };
                    let source = saved
                        .resolve(native_capture::list_sources(kind)?)
                        .ok_or_else(|| {
                            format!(
                                "Aguardando ‘{}’ ficar disponível e ser identificada sem ambiguidade.",
                                saved.name
                            )
                        })?;
                    let capture = CaptureSession::start(kind, source.id, saved.capture_cursor)?;
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while !cancellation.load(Ordering::Acquire) && Instant::now() < deadline {
                        if let Some(frame) = capture.read(0)? {
                            let wake = context.clone();
                            capture.set_waker(Arc::new(move || wake.request_repaint()));
                            return Ok(Ready {
                                pixels: Some(Pixels {
                                    width: frame.width,
                                    height: frame.height,
                                    rgba: frame.rgba,
                                }),
                                stream: Stream::Desktop(capture, frame.sequence),
                            });
                        }
                    }
                    Err("A captura não enviou imagens.".into())
                }
                SourceInput::Camera(saved) => {
                    let capture = CameraCapture::start(saved.id)?;
                    let deadline = Instant::now() + Duration::from_secs(12);
                    while !cancellation.load(Ordering::Acquire) && Instant::now() < deadline {
                        if let Some(frame) = capture.latest()? {
                            let wake = context.clone();
                            capture.set_waker(Arc::new(move || wake.request_repaint()));
                            return Ok(Ready {
                                pixels: Some(Pixels {
                                    width: frame.width,
                                    height: frame.height,
                                    rgba: frame.rgba,
                                }),
                                stream: Stream::Camera(capture, frame.sequence),
                            });
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err("A câmera não respondeu. A reconexão será automática.".into())
                }
                SourceInput::Ndi(saved) => {
                    let capture = NdiCapture::start(saved)?;
                    let wake = context.clone();
                    capture.set_waker(Arc::new(move || wake.request_repaint()));
                    Ok(Ready {
                        pixels: None,
                        stream: Stream::Ndi(capture, 0),
                    })
                }
            })();
            if !cancellation.load(Ordering::Acquire) {
                let _ = sender.send(result);
                context.request_repaint();
            }
        });
        Self {
            pixels: None,
            texture: None,
            stream: Stream::Loading(Pending {
                receiver,
                cancelled,
            }),
            error: None,
            retry: RetryState {
                input: Some(retry_input),
                ..Default::default()
            },
        }
    }
    pub fn loading_now(&self) -> bool {
        matches!(self.stream, Stream::Loading(_)) || self.waiting_ndi()
    }
    pub fn waiting_ndi(&self) -> bool {
        matches!(self.stream, Stream::Ndi(..)) && self.pixels.is_none()
    }
    pub fn live(&self) -> bool {
        matches!(
            self.stream,
            Stream::Desktop(..) | Stream::Camera(..) | Stream::Ndi(..)
        )
    }
    pub fn poll(&mut self, ctx: &Context) -> (bool, u64, Option<String>) {
        if matches!(self.stream, Stream::Offline)
            && let Some(next) = self.retry.next
        {
            if Instant::now() >= next {
                if let Some(input) = self.retry.input.clone() {
                    let mut retry = self.retry.clone();
                    retry.next = None;
                    *self = Self::loading(ctx, input);
                    self.retry = retry;
                }
            } else {
                ctx.request_repaint_after(next.saturating_duration_since(Instant::now()));
            }
        }
        let ready = if let Stream::Loading(pending) = &self.stream {
            match pending.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("A abertura da fonte foi interrompida.".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            }
        } else {
            None
        };
        if let Some(result) = ready {
            match result {
                Ok(ready) => {
                    self.retry.delay = Duration::ZERO;
                    self.retry.next = None;
                    self.stream = ready.stream;
                    if let Some(pixels) = ready.pixels {
                        self.set_pixels(ctx, pixels);
                    }
                    return (true, 0, None);
                }
                Err(error) => return self.fail(ctx, error),
            }
        }
        let mut dropped = 0;
        let mut waiting = false;
        let frame = match &mut self.stream {
            Stream::Desktop(capture, sequence) => capture.try_read(*sequence).map(|frame| {
                frame.map(|frame| {
                    dropped = frame.sequence.saturating_sub(*sequence).saturating_sub(1) as u64;
                    *sequence = frame.sequence;
                    Pixels {
                        width: frame.width,
                        height: frame.height,
                        rgba: frame.rgba,
                    }
                })
            }),
            Stream::Camera(capture, sequence) => capture.latest().map(|frame| {
                frame
                    .filter(|frame| frame.sequence != *sequence)
                    .map(|frame| {
                        *sequence = frame.sequence;
                        Pixels {
                            width: frame.width,
                            height: frame.height,
                            rgba: frame.rgba,
                        }
                    })
            }),
            Stream::Ndi(capture, sequence) => capture.latest().map(|update| {
                waiting = update.waiting;
                update.frame.map(|frame| {
                    dropped = frame.sequence.saturating_sub(*sequence).saturating_sub(1);
                    *sequence = frame.sequence;
                    frame.pixels
                })
            }),
            _ => Ok(None),
        };
        match frame {
            Ok(Some(pixels)) => {
                self.set_pixels(ctx, pixels);
                (true, dropped, None)
            }
            Ok(None) => {
                let changed = waiting && self.pixels.is_some();
                if waiting {
                    self.pixels = None;
                    self.texture = None;
                }
                (changed, 0, None)
            }
            Err(error) => self.fail(ctx, error),
        }
    }
    fn fail(&mut self, ctx: &Context, error: String) -> (bool, u64, Option<String>) {
        self.stream = Stream::Offline;
        self.pixels = None;
        self.texture = None;
        self.error = Some(error.clone());
        if self.retry.input.is_some() {
            self.retry.delay =
                (self.retry.delay * 2).clamp(Duration::from_secs(2), Duration::from_secs(5));
            self.retry.next = Some(Instant::now() + self.retry.delay);
            ctx.request_repaint_after(self.retry.delay);
        }
        (false, 0, Some(error))
    }
    fn set_pixels(&mut self, ctx: &Context, pixels: Pixels) {
        if let Some(texture) = &mut self.texture {
            texture.set(
                ColorImage::from_rgba_unmultiplied(
                    [pixels.width as usize, pixels.height as usize],
                    &pixels.rgba,
                ),
                TextureOptions::LINEAR,
            );
        } else {
            self.texture = Some(load_texture(
                ctx,
                "source",
                pixels.width,
                pixels.height,
                &pixels.rgba,
            ));
        }
        self.pixels = Some(pixels);
        self.error = None;
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;

    #[test]
    fn missing_source_is_reopened_automatically_when_it_returns() {
        let path =
            std::env::temp_dir().join(format!("mini-studio-recovery-{}.png", std::process::id()));
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        let ctx = Context::default();
        let mut runtime = RuntimeSource::loading(&ctx, SourceInput::Image(path.clone()));
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime.error.is_none() {
            runtime.poll(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let scheduled = runtime.retry.next.unwrap();
        for _ in 0..10 {
            runtime.poll(&ctx);
        }
        assert!(matches!(runtime.stream, Stream::Offline));
        assert_eq!(runtime.retry.next, Some(scheduled));
        assert_eq!(runtime.retry.delay, Duration::from_secs(2));
        image::RgbaImage::from_pixel(2, 2, image::Rgba([10_u8, 20, 30, 255]))
            .save(&path)
            .unwrap();
        runtime.retry.next = Some(Instant::now()); // Advance the retry deadline without sleeping.
        let deadline = Instant::now() + Duration::from_secs(5);
        while runtime.pixels.is_none() {
            runtime.poll(&ctx);
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(runtime.error.is_none());
        assert_eq!(
            &runtime.pixels.as_ref().unwrap().rgba[..4],
            &[10, 20, 30, 255]
        );
        assert_eq!(runtime.retry.delay, Duration::ZERO);
        assert!(runtime.retry.next.is_none());
        std::fs::remove_file(path).unwrap();
    }
}
