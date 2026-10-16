//! Ogg Opus as voice messages come (RFC 7845): one logical stream, mono or stereo, read whole into
//! memory, since a five-minute note is under a megabyte, and decoded from any position.

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ogg::PacketReader;
use opus::{Channels, Decoder};

use crate::Error;
use crate::resample::Resampler;

/// Granule positions in Ogg Opus count samples at 48 kHz, whatever the input's rate.
pub(crate) const GRANULE_RATE: u32 = 48_000;

/// The rates libopus decodes to directly.
pub(crate) const OPUS_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];

/// Opus asks a decoder to run 80 ms ahead of a seek target, so its state has settled by then.
const PRE_ROLL: u64 = 3_840;

/// The most a file may be. A voice message at the recording cap is about 3 MB.
const MAX_FILE: u64 = 64 << 20;

/// A voice message's packets with where each starts, ready to decode from anywhere.
pub struct VoiceFile {
    data: Vec<u8>,
    packets: Vec<Packet>,
    channels: Channels,
    gain: i32,
    pre_skip: u64,
    end: u64,
}

/// 16 bytes a packet; a packet ends where the next begins.
#[derive(Clone, Copy)]
struct Packet {
    start: u64,
    offset: u32,
    samples: u32,
}

impl VoiceFile {
    pub fn open(path: &Path) -> Result<Arc<VoiceFile>, Error> {
        let size = std::fs::metadata(path).map_err(|e| Error::file(path, "read", e))?.len();
        if size > MAX_FILE {
            return Err(Error::not_voice(path, format!("it's {} MB, more than a voice message can be", size >> 20)));
        }
        let bytes = std::fs::read(path).map_err(|e| Error::file(path, "read", e))?;
        VoiceFile::parse(&bytes).map(Arc::new).map_err(|why| Error::not_voice(path, why))
    }

    fn parse(bytes: &[u8]) -> Result<VoiceFile, String> {
        let mut reader = PacketReader::new(Cursor::new(bytes));
        let mut next = || reader.read_packet().map_err(|e| format!("its Ogg pages are damaged ({e})"));

        let head = next()?.ok_or("it's empty")?;
        let serial = head.stream_serial();
        let h = &head.data;
        if h.len() < 19 || &h[..8] != b"OpusHead" {
            return Err("it isn't Opus in an Ogg container".into());
        }
        if h[8] >> 4 != 0 {
            return Err(format!("it's Ogg Opus version {}, which jaw doesn't know", h[8]));
        }
        let channels = match (h[9], h[18]) {
            (1, 0) => Channels::Mono,
            (2, 0) => Channels::Stereo,
            (n, _) => return Err(format!("it has {n} channels, and jaw plays mono or stereo")),
        };
        let pre_skip = u64::from(u16::from_le_bytes([h[10], h[11]]));
        let gain = i32::from(i16::from_le_bytes([h[16], h[17]]));

        let mut data = Vec::with_capacity(bytes.len());
        let mut packets = Vec::new();
        // Each packet's duration, and the granule of each page a packet ended, to place them.
        let mut first_end: Option<(usize, u64, bool)> = None;
        let mut last_granule = None;
        let mut tags = false;
        while let Some(p) = next()? {
            if p.stream_serial() != serial {
                continue;
            }
            if !tags {
                if !p.data.starts_with(b"OpusTags") {
                    return Err("its second packet isn't OpusTags".into());
                }
                tags = true;
                continue;
            }
            let samples = opus::packet::get_nb_samples(&p.data, GRANULE_RATE).unwrap_or(0) as u32;
            packets.push(Packet { start: 0, offset: data.len() as u32, samples });
            data.extend_from_slice(&p.data);
            if p.last_in_page() && p.absgp_page() != u64::MAX {
                if first_end.is_none() {
                    first_end = Some((packets.len(), p.absgp_page(), p.last_in_stream()));
                }
                last_granule = Some(p.absgp_page());
            }
        }
        let Some((n, granule, eos)) = first_end else {
            return Err("it has no audio".into());
        };
        // A stream may start past zero; the first page's granule says where. When that page also
        // ends the stream, its granule is trimmed instead, and the stream starts at zero.
        let first: u64 = packets[..n].iter().map(|p| u64::from(p.samples)).sum();
        let mut at = if eos { 0 } else { granule.saturating_sub(first) };
        for p in &mut packets {
            p.start = at;
            at += u64::from(p.samples);
        }
        let end = last_granule.unwrap_or(at).min(at);
        if end <= pre_skip {
            return Err("it has no audio".into());
        }
        data.shrink_to_fit();
        Ok(VoiceFile { data, packets, channels, gain, pre_skip, end })
    }

    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.samples() as f64 / f64::from(GRANULE_RATE))
    }

    /// How many samples it plays, at 48 kHz.
    fn samples(&self) -> u64 {
        self.end - self.pre_skip
    }

    /// Bytes held in memory.
    pub fn size(&self) -> usize {
        self.data.capacity() + self.packets.capacity() * size_of::<Packet>()
    }

    /// The whole message as mono samples at `rate`.
    pub fn pcm(self: &Arc<Self>, rate: u32) -> Result<Vec<f32>, Error> {
        let decode_rate = if OPUS_RATES.contains(&rate) { rate } else { GRANULE_RATE };
        let mut decode = self.decode(decode_rate, Duration::ZERO)?;
        let mut pcm = Vec::with_capacity((self.samples() * u64::from(decode_rate) / u64::from(GRANULE_RATE)) as usize);
        while decode.next(&mut pcm) {}
        if decode_rate == rate {
            return Ok(pcm);
        }
        let mut out = Vec::new();
        let mut resample = Resampler::new(decode_rate, rate);
        resample.process(&pcm, &mut out);
        resample.finish(&mut out);
        Ok(out)
    }

    /// A decoder at `rate` (one of libopus's rates) whose first sample is the one at `at`.
    pub(crate) fn decode(self: &Arc<Self>, rate: u32, at: Duration) -> Result<Decode, Error> {
        debug_assert!(OPUS_RATES.contains(&rate));
        let mut decoder = Decoder::new(rate, self.channels).map_err(|e| Error::codec("start a decoder", e))?;
        if self.gain != 0 {
            decoder.set_gain(self.gain).map_err(|e| Error::codec("apply the file's gain", e))?;
        }
        let target = (self.pre_skip + (at.as_secs_f64() * f64::from(GRANULE_RATE)) as u64).min(self.end);
        let from = target.saturating_sub(PRE_ROLL);
        let next = self.packets.partition_point(|p| p.start + u64::from(p.samples) <= from);
        let channels = if self.channels == Channels::Stereo { 2 } else { 1 };
        Ok(Decode {
            file: Arc::clone(self),
            decoder,
            step: u64::from(GRANULE_RATE / rate),
            channels,
            next,
            from: target,
            pcm: vec![0.0; 5_760 * channels],
            concealed: 0,
        })
    }
}

/// Decodes a file packet by packet into mono samples at one rate.
pub(crate) struct Decode {
    file: Arc<VoiceFile>,
    decoder: Decoder,
    step: u64,
    channels: usize,
    next: usize,
    from: u64,
    pcm: Vec<f32>,
    concealed: u64,
}

impl Decode {
    /// Appends the next packet's samples to `out`, or returns false at the end.
    pub(crate) fn next(&mut self, out: &mut Vec<f32>) -> bool {
        let file = &self.file;
        let Some(p) = file.packets.get(self.next) else {
            return false;
        };
        if p.start >= file.end {
            return false;
        }
        self.next += 1;
        let end = file.packets.get(self.next).map_or(file.data.len(), |q| q.offset as usize);
        let packet = &file.data[p.offset as usize..end];
        let frames = (u64::from(p.samples) / self.step) as usize;
        let pcm = &mut self.pcm[..frames * self.channels];
        // A damaged packet is concealed rather than ending the message.
        let got = match self.decoder.decode_float(packet, pcm, false) {
            Ok(n) => n,
            Err(_) => {
                self.concealed += 1;
                self.decoder.decode_float(&[], pcm, false).unwrap_or_else(|_| {
                    pcm.fill(0.0);
                    frames
                })
            }
        };
        let first = file.pre_skip.max(self.from);
        for i in 0..got.min(frames) {
            let granule = p.start + i as u64 * self.step;
            if granule < first || granule >= file.end {
                continue;
            }
            out.push(match self.channels {
                1 => pcm[i],
                _ => 0.5 * (pcm[2 * i] + pcm[2 * i + 1]),
            });
        }
        true
    }

    /// Packets that couldn't be decoded and were concealed.
    pub(crate) fn concealed(&self) -> u64 {
        self.concealed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture, frequency, rms};

    fn decode_all(file: &Arc<VoiceFile>, rate: u32, at: f64) -> Vec<f32> {
        let mut d = file.decode(rate, Duration::from_secs_f64(at)).unwrap();
        let mut out = Vec::new();
        while d.next(&mut out) {}
        assert_eq!(d.concealed(), 0);
        out
    }

    #[test]
    fn a_tone_decodes_to_its_exact_length_pitch_and_loudness() {
        for name in ["tone-16k.ogg", "tone-48k.ogg", "tone-stereo.ogg"] {
            let file = VoiceFile::open(&fixture(name)).unwrap();
            assert_eq!(file.duration(), Duration::from_secs(2), "{name}");
            for rate in OPUS_RATES {
                let pcm = decode_all(&file, rate, 0.0);
                assert_eq!(pcm.len(), 2 * rate as usize, "{name} at {rate}");
                let body = &pcm[rate as usize / 10..pcm.len() - rate as usize / 10];
                let hz = frequency(body, rate);
                assert!((hz - 440.0).abs() < 2.0, "{name} at {rate} sounds at {hz} Hz");
                // The tone was 0.5 at its peak, so its RMS is 0.354.
                let level = rms(body);
                assert!((level - 0.354).abs() < 0.03, "{name} at {rate} has an RMS of {level}");
            }
        }
    }

    #[test]
    fn decoding_from_a_position_gives_the_same_samples_as_decoding_through_it() {
        let file = VoiceFile::open(&fixture("speech-16k.ogg")).unwrap();
        let all = decode_all(&file, 48_000, 0.0);
        let from = decode_all(&file, 48_000, 1.5);
        assert_eq!(from.len(), all.len() - 72_000);
        let tail = &all[72_000..];
        let error: f32 = tail.iter().zip(&from).map(|(a, b)| (a - b).powi(2)).sum::<f32>() / tail.len() as f32;
        // The pre-roll settles the decoder; what differs is far below the signal.
        assert!(error.sqrt() < rms(tail) / 100.0, "error {} against {}", error.sqrt(), rms(tail));
    }

    #[test]
    fn files_that_arent_ogg_opus_are_refused_saying_why() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.ogg");
        std::fs::write(&path, b"not a voice message at all").unwrap();
        let err = VoiceFile::open(&path).err().unwrap().to_string();
        assert!(err.contains("isn't a voice message jaw can play"), "{err}");
        let err = VoiceFile::open(&dir.path().join("missing.ogg")).err().unwrap().to_string();
        assert!(err.starts_with("couldn't read"), "{err}");
    }

    #[test]
    fn a_five_minute_note_is_held_in_a_megabyte_and_a_half() {
        let file = VoiceFile::open(&fixture("speech-16k.ogg")).unwrap();
        let per_second = file.size() as f64 / file.duration().as_secs_f64();
        assert!(per_second * 300.0 < 1.5 * 1_048_576.0, "{per_second} bytes a second");
    }
}
