//! The system's default output and input through cpal: Core Audio on macOS, ALSA on Linux.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, ErrorKind, FromSample, SampleFormat, SizedSample, Stream, SupportedStreamConfig};

use crate::Error;
use crate::device::{Capture, Input, Microphone, Output, Render, Speaker};

/// The default output device, opened afresh at each play.
pub struct SystemSpeaker;

/// The default input device, opened afresh at each recording.
pub struct SystemMicrophone;

struct Opened {
    device: Device,
    config: SupportedStreamConfig,
    stream: Option<Stream>,
    fault: Arc<Mutex<Option<String>>>,
}

impl Opened {
    fn on_error(&self) -> impl FnMut(cpal::Error) + Send + 'static {
        let fault = Arc::clone(&self.fault);
        move |err: cpal::Error| {
            // A rerouted stream and a missed deadline carry on; anything else has stopped it.
            if !matches!(err.kind(), ErrorKind::DeviceChanged | ErrorKind::RealtimeDenied | ErrorKind::Xrun) {
                *fault.lock().unwrap_or_else(|e| e.into_inner()) = Some(err.to_string());
            }
        }
    }

    fn play(&mut self, stream: Result<Stream, cpal::Error>, what: &str) -> Result<(), Error> {
        let stream = stream.map_err(|e| device_error(what, &e))?;
        stream.play().map_err(|e| device_error(what, &e))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn fault(&self) -> Option<String> {
        self.fault.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

fn device_error(what: &str, err: &cpal::Error) -> Error {
    let hint = match err.kind() {
        ErrorKind::PermissionDenied if what == "microphone" => {
            "; allow jawd in System Settings › Privacy & Security › Microphone"
        }
        ErrorKind::DeviceBusy => "; another app may be holding it",
        _ => "",
    };
    Error::Device(format!("the {what} can't be used: {err}{hint}"))
}

impl Speaker for SystemSpeaker {
    fn open(&mut self) -> Result<Box<dyn Output>, Error> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| Error::Device("there's no audio output; connect speakers or headphones".into()))?;
        let config = device.default_output_config().map_err(|e| device_error("audio output", &e))?;
        Ok(Box::new(Opened { device, config, stream: None, fault: Arc::default() }))
    }
}

impl Microphone for SystemMicrophone {
    fn open(&mut self) -> Result<Box<dyn Input>, Error> {
        let device = cpal::default_host()
            .default_input_device()
            .ok_or_else(|| Error::Device("there's no microphone; connect one to record".into()))?;
        let config = device.default_input_config().map_err(|e| device_error("microphone", &e))?;
        Ok(Box::new(Opened { device, config, stream: None, fault: Arc::default() }))
    }
}

impl Output for Opened {
    fn rate(&self) -> u32 {
        self.config.sample_rate()
    }

    fn start(&mut self, render: Render) -> Result<(), Error> {
        match self.config.sample_format() {
            SampleFormat::F32 => self.output::<f32>(render),
            SampleFormat::I16 => self.output::<i16>(render),
            SampleFormat::I32 => self.output::<i32>(render),
            SampleFormat::U16 => self.output::<u16>(render),
            f => Err(Error::Device(format!("the audio output wants {f} samples, which jaw doesn't write"))),
        }
    }

    fn fault(&self) -> Option<String> {
        Opened::fault(self)
    }
}

impl Opened {
    fn output<T: SizedSample + FromSample<f32>>(&mut self, mut render: Render) -> Result<(), Error> {
        let channels = usize::from(self.config.channels());
        // Sized past any usual callback, so the device's thread doesn't allocate.
        let mut mono = vec![0.0; 8_192];
        let stream = self.device.build_output_stream(
            self.config.config(),
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels;
                if mono.len() < frames {
                    mono.resize(frames, 0.0);
                }
                render(&mut mono[..frames]);
                for (frame, &x) in data.chunks_mut(channels).zip(&mono) {
                    frame.fill(T::from_sample(x));
                }
            },
            self.on_error(),
            None,
        );
        self.play(stream, "audio output")
    }

    fn input<T: SizedSample>(&mut self, mut capture: Capture) -> Result<(), Error>
    where
        f32: FromSample<T>,
    {
        let channels = usize::from(self.config.channels());
        let mut mono = Vec::with_capacity(8_192);
        let stream = self.device.build_input_stream(
            self.config.config(),
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                mono.clear();
                mono.extend(
                    data.chunks(channels)
                        .map(|frame| frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / channels as f32),
                );
                capture(&mono);
            },
            self.on_error(),
            None,
        );
        self.play(stream, "microphone")
    }
}

impl Input for Opened {
    fn rate(&self) -> u32 {
        self.config.sample_rate()
    }

    fn start(&mut self, capture: Capture) -> Result<(), Error> {
        match self.config.sample_format() {
            SampleFormat::F32 => self.input::<f32>(capture),
            SampleFormat::I16 => self.input::<i16>(capture),
            SampleFormat::I32 => self.input::<i32>(capture),
            SampleFormat::U16 => self.input::<u16>(capture),
            f => Err(Error::Device(format!("the microphone gives {f} samples, which jaw doesn't read"))),
        }
    }

    fn fault(&self) -> Option<String> {
        Opened::fault(self)
    }
}
