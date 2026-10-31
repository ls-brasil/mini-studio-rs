use crate::{
    camera::CameraCapture,
    composition::Pixels,
    image_source, load_texture,
    native_capture::{self, CaptureSession, SourceKind},
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
    Loading(Pending),
    Offline,
}
struct Ready {
    pixels: Pixels,
    stream: Stream,
}

pub struct RuntimeSource {
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
        }
    }
    pub fn loading(ctx: &Context, input: SourceInput) -> Self {
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = cancelled.clone();
        let context = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| match input {
                SourceInput::Image(path) => Ok(Ready {
                    pixels: image_source::load(&path)?.pixels,
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
                                "‘{}’ indisponível ou ambígua. Escolha a fonte novamente.",
                                saved.name
                            )
                        })?;
                    let capture = CaptureSession::start(kind, source.id)?;
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while !cancellation.load(Ordering::Acquire) && Instant::now() < deadline {
                        if let Some(frame) = capture.read(0)? {
                            let wake = context.clone();
                            capture.set_waker(Arc::new(move || wake.request_repaint()));
                            return Ok(Ready {
                                pixels: Pixels {
                                    width: frame.width,
                                    height: frame.height,
                                    rgba: frame.rgba,
                                },
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
                                pixels: Pixels {
                                    width: frame.width,
                                    height: frame.height,
                                    rgba: frame.rgba,
                                },
                                stream: Stream::Camera(capture, frame.sequence),
                            });
                        }
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err("A câmera não respondeu. Confira o dispositivo e tente novamente.".into())
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
        }
    }
    pub fn loading_now(&self) -> bool {
        matches!(self.stream, Stream::Loading(_))
    }
    pub fn live(&self) -> bool {
        matches!(self.stream, Stream::Desktop(..) | Stream::Camera(..))
    }
    pub fn poll(&mut self, ctx: &Context) -> (bool, u64, Option<String>) {
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
                    self.stream = ready.stream;
                    self.set_pixels(ctx, ready.pixels);
                    return (true, 0, None);
                }
                Err(error) => return self.fail(error),
            }
        }
        let mut dropped = 0;
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
            _ => Ok(None),
        };
        match frame {
            Ok(Some(pixels)) => {
                self.set_pixels(ctx, pixels);
                (true, dropped, None)
            }
            Ok(None) => (false, 0, None),
            Err(error) => self.fail(error),
        }
    }
    fn fail(&mut self, error: String) -> (bool, u64, Option<String>) {
        self.stream = Stream::Offline;
        self.pixels = None;
        self.texture = None;
        self.error = Some(error.clone());
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
