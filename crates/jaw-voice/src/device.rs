//! The audio devices behind small traits: cpal's in `system`, and fakes in `fake`, so tests never
//! need a microphone or speakers. A device is opened for each play or recording and closed when it
//! ends, so an idle jawd holds no device and takes no wake-ups from one.

use crate::Error;

/// Fills a buffer with mono samples at the output's rate; the device copies them to every channel.
/// It runs on the device's own thread and must never block.
pub type Render = Box<dyn FnMut(&mut [f32]) + Send>;

/// Takes mono samples at the input's rate, on the device's own thread; it must never block.
pub type Capture = Box<dyn FnMut(&[f32]) + Send>;

pub trait Speaker: Send {
    /// The output as it is now, ready to start; a changed default device is followed at each play.
    fn open(&mut self) -> Result<Box<dyn Output>, Error>;
}

pub trait Output {
    fn rate(&self) -> u32;
    /// Starts calling `render` until the output is dropped.
    fn start(&mut self, render: Render) -> Result<(), Error>;
    /// What stopped the device after it started, if something did.
    fn fault(&self) -> Option<String>;
}

pub trait Microphone: Send {
    fn open(&mut self) -> Result<Box<dyn Input>, Error>;
}

pub trait Input {
    fn rate(&self) -> u32;
    /// Starts calling `capture` until the input is dropped.
    fn start(&mut self, capture: Capture) -> Result<(), Error>;
    fn fault(&self) -> Option<String>;
}
