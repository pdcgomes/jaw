//! Opus in Ogg as WhatsApp's phone apps record voice notes: 16 kHz mono in VoIP mode at about
//! 24 kbit/s, variable, in 20 ms frames, so recipients' apps take it as a voice note. Pages close
//! every second, as ffmpeg's do.

use std::io::Write;

use ogg::{PacketWriteEndInfo, PacketWriter};
use opus::{Application, Bitrate, Channels, Encoder, Signal};

use crate::Error;
use crate::oggopus::GRANULE_RATE;

pub(crate) const RATE: u32 = 16_000;
const FRAME: usize = 320;
const BITRATE: i32 = 24_000;
const PACKETS_A_PAGE: u64 = 50;
const SCALE: u64 = (GRANULE_RATE / RATE) as u64;

pub(crate) struct OggOpusWriter<W: Write> {
    opus: Encoder,
    ogg: PacketWriter<'static, W>,
    serial: u32,
    lookahead: usize,
    pre_skip: u64,
    frame: Vec<f32>,
    /// Each packet waits for the next, so the last can carry the stream's end.
    held: Option<Vec<u8>>,
    packets: u64,
    samples: u64,
    out: Vec<u8>,
}

impl<W: Write> OggOpusWriter<W> {
    pub(crate) fn new(writer: W, serial: u32) -> Result<OggOpusWriter<W>, Error> {
        let codec = |what| move |e| Error::codec(what, e);
        let mut opus = Encoder::new(RATE, Channels::Mono, Application::Voip).map_err(codec("start an encoder"))?;
        opus.set_bitrate(Bitrate::Bits(BITRATE)).map_err(codec("set the bitrate"))?;
        opus.set_vbr(true).map_err(codec("set a variable bitrate"))?;
        opus.set_signal(Signal::Voice).map_err(codec("set the signal to voice"))?;
        opus.set_complexity(10).map_err(codec("set the complexity"))?;
        let lookahead = opus.get_lookahead().map_err(codec("read its lookahead"))? as usize;
        let pre_skip = lookahead as u64 * SCALE;

        let mut head = Vec::with_capacity(19);
        head.extend_from_slice(b"OpusHead");
        head.extend_from_slice(&[1, 1]);
        head.extend_from_slice(&(pre_skip as u16).to_le_bytes());
        head.extend_from_slice(&RATE.to_le_bytes());
        head.extend_from_slice(&[0, 0, 0]);
        let vendor = format!("jaw-voice with {}", opus::version());
        let mut tags = Vec::new();
        tags.extend_from_slice(b"OpusTags");
        tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
        tags.extend_from_slice(vendor.as_bytes());
        tags.extend_from_slice(&0u32.to_le_bytes());

        let mut ogg = PacketWriter::new(writer);
        let io = |e: std::io::Error| Error::Codec(format!("couldn't write the recording: {e}"));
        ogg.write_packet(head, serial, PacketWriteEndInfo::EndPage, 0).map_err(io)?;
        ogg.write_packet(tags, serial, PacketWriteEndInfo::EndPage, 0).map_err(io)?;
        Ok(OggOpusWriter {
            opus,
            ogg,
            serial,
            lookahead,
            pre_skip,
            frame: Vec::with_capacity(FRAME),
            held: None,
            packets: 0,
            samples: 0,
            out: vec![0; 1_500],
        })
    }

    /// Samples given so far, at 16 kHz.
    pub(crate) fn samples(&self) -> u64 {
        self.samples
    }

    pub(crate) fn push(&mut self, pcm: &[f32]) -> Result<(), Error> {
        self.samples += pcm.len() as u64;
        self.feed(pcm)
    }

    fn feed(&mut self, mut pcm: &[f32]) -> Result<(), Error> {
        while !pcm.is_empty() {
            let n = (FRAME - self.frame.len()).min(pcm.len());
            self.frame.extend_from_slice(&pcm[..n]);
            pcm = &pcm[n..];
            if self.frame.len() == FRAME {
                self.encode()?;
            }
        }
        Ok(())
    }

    fn encode(&mut self) -> Result<(), Error> {
        let n = self.opus.encode_float(&self.frame, &mut self.out).map_err(|e| Error::codec("encode", e))?;
        self.frame.clear();
        let packet = self.out[..n].to_vec();
        if let Some(held) = self.held.replace(packet) {
            let end = if self.packets % PACKETS_A_PAGE == PACKETS_A_PAGE - 1 {
                PacketWriteEndInfo::EndPage
            } else {
                PacketWriteEndInfo::NormalPacket
            };
            self.write(held, end, (self.packets + 1) * FRAME as u64 * SCALE)?;
            self.packets += 1;
        }
        Ok(())
    }

    fn write(&mut self, packet: Vec<u8>, end: PacketWriteEndInfo, granule: u64) -> Result<(), Error> {
        self.ogg
            .write_packet(packet, self.serial, end, granule)
            .map_err(|e| Error::Codec(format!("couldn't write the recording: {e}")))
    }

    /// Encodes what the encoder still holds and ends the stream, its last granule trimming the
    /// padding so it plays exactly the samples given.
    pub(crate) fn finish(mut self) -> Result<W, Error> {
        // The encoder holds its lookahead back; zeros push it out, then fill the last frame.
        self.feed(&vec![0.0; self.lookahead])?;
        if !self.frame.is_empty() {
            self.feed(&vec![0.0; FRAME - self.frame.len()])?;
        }
        let last =
            self.held.take().ok_or_else(|| Error::Codec("libopus gave no audio to end the recording with".into()))?;
        self.write(last, PacketWriteEndInfo::EndStream, self.pre_skip + self.samples * SCALE)?;
        Ok(self.ogg.into_inner())
    }
}
