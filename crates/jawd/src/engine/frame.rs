//! Frames on the engine link: each is its length, four bytes big-endian, then the protobuf.

use anyhow::{Result, bail};
use prost::Message as _;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::pb;

/// The largest frame either side sends; big payloads, such as history, travel as files.
pub const MAX_FRAME: usize = 16 << 20;

/// The next frame, or `None` if the other side closed the link between frames.
pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Option<pb::Frame>> {
    let mut size = [0u8; 4];
    match reader.read_exact(&mut size).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let n = u32::from_be_bytes(size) as usize;
    if n > MAX_FRAME {
        bail!("a frame of {n} bytes is over the link's {MAX_FRAME}");
    }
    let mut data = vec![0u8; n];
    reader.read_exact(&mut data).await.map_err(|e| anyhow::anyhow!("the link closed in the middle of a frame: {e}"))?;
    Ok(Some(pb::Frame::decode(data.as_slice())?))
}

/// Writes one frame, without flushing, so a burst can go out in one write.
pub async fn write<W: AsyncWrite + Unpin>(writer: &mut W, frame: &pb::Frame) -> Result<()> {
    let data = frame.encode_to_vec();
    if data.len() > MAX_FRAME {
        bail!("a frame of {} bytes is over the link's {MAX_FRAME}", data.len());
    }
    writer.write_all(&(data.len() as u32).to_be_bytes()).await?;
    writer.write_all(&data).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(program: &str) -> pb::Frame {
        pb::Frame {
            id: 0,
            body: Some(pb::frame::Body::Hello(pb::Hello {
                protocol: 1,
                program: program.into(),
                whatsmeow: String::new(),
            })),
        }
    }

    #[tokio::test]
    async fn frames_round_trip_and_end_cleanly() {
        let (mut ours, mut theirs) = tokio::io::duplex(1 << 20);
        let big = pb::Frame {
            id: 0,
            body: Some(pb::frame::Body::Event(pb::Event {
                seq: 3,
                kind: Some(pb::event::Kind::Bench(pb::Bench { payload: vec![7; 200_000] })),
            })),
        };
        write(&mut ours, &hello("a")).await.unwrap();
        write(&mut ours, &big).await.unwrap();
        ours.flush().await.unwrap();
        drop(ours);
        assert_eq!(read(&mut theirs).await.unwrap(), Some(hello("a")));
        assert_eq!(read(&mut theirs).await.unwrap(), Some(big));
        assert_eq!(read(&mut theirs).await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_frame_cut_short_is_an_error() {
        let (mut ours, mut theirs) = tokio::io::duplex(64);
        ours.write_all(&10u32.to_be_bytes()).await.unwrap();
        ours.write_all(&[1, 2, 3]).await.unwrap();
        drop(ours);
        assert!(read(&mut theirs).await.is_err());
    }

    #[tokio::test]
    async fn an_oversized_frame_is_refused() {
        let (mut ours, mut theirs) = tokio::io::duplex(64);
        ours.write_all(&((MAX_FRAME + 1) as u32).to_be_bytes()).await.unwrap();
        assert!(read(&mut theirs).await.unwrap_err().to_string().contains("over the link's"));
    }
}
