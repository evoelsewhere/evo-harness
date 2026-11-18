//! Native messaging frames: a 4-byte little-endian length, then that many
//! bytes of UTF-8 JSON. The host relays frames without reading them; the app
//! uses the same framing on its pipe
//! (docs/features/browser-bridge/plan.md, 3.3).

use std::io::{self, ErrorKind};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The largest frame accepted in either direction. The browser sends at most
/// 64 MiB to a host; the app splits anything over 1 MB it sends to the
/// browser into `chunk` frames.
pub const MAX_FRAME: usize = 64 * 1024 * 1024;

/// Read one frame's payload; `Ok(None)` at a clean end of stream.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len = [0u8; 4];
    match reader.read_exact(&mut len).await {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("frame of {len} bytes is over the limit"),
        ));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;
    Ok(Some(payload))
}

/// Write one frame and flush it.
pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "frame too large"))?;
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&len.to_le_bytes());
    buf.extend_from_slice(payload);
    writer.write_all(&buf).await?;
    writer.flush().await
}

/// Copy frames from `reader` to `writer` until the reader ends or either side fails.
pub async fn relay<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    reader: &mut R,
    writer: &mut W,
) -> io::Result<()> {
    while let Some(frame) = read_frame(reader).await? {
        write_frame(writer, &frame).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn framed(payloads: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for payload in payloads {
            write_frame(&mut out, payload).await.unwrap();
        }
        out
    }

    #[tokio::test]
    async fn frames_pass_through_unchanged() {
        let input = framed(&[br#"{"t":"hello"}"#, b"", br#"{"t":"cdp","id":7}"#]).await;
        let mut output = Vec::new();
        relay(&mut input.as_slice(), &mut output).await.unwrap();
        assert_eq!(output, input);
    }

    #[tokio::test]
    async fn a_clean_end_is_not_an_error() {
        assert!(read_frame(&mut [].as_slice()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_frame_cut_short_is_an_error() {
        let mut input = framed(&[b"abcdef"]).await;
        input.truncate(7);
        assert!(read_frame(&mut input.as_slice()).await.is_err());
    }

    #[tokio::test]
    async fn an_oversized_length_is_refused_before_reading() {
        let len = (MAX_FRAME as u32 + 1).to_le_bytes();
        let error = read_frame(&mut len.as_slice()).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
    }
}
