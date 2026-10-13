use super::*;
use std::io::{self, Write};

fn assert_frame<'a>(bytes: &'a [u8], filename: &str, uncompressed_size: usize) -> &'a [u8] {
    assert_eq!(&bytes[..6], b"HAXX\x00\x05");
    assert_eq!(
        u16::from_be_bytes(bytes[6..8].try_into().unwrap()) as usize,
        filename.len()
    );
    let size = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
    assert_eq!(
        u32::from_be_bytes(bytes[12..16].try_into().unwrap()) as usize,
        uncompressed_size
    );
    assert_eq!(bytes.len(), 16 + size + filename.len() + 1);
    assert_eq!(&bytes[16 + size..bytes.len() - 1], filename.as_bytes());
    assert_eq!(bytes.last(), Some(&0));
    &bytes[16..16 + size]
}

#[test]
fn send_flushes_the_complete_frame() {
    for size in [0, 1, 4096, CHUNK_SIZE + 7] {
        let payload = vec![0x42; size];
        let mut output = Vec::new();
        send(&mut output, "boot.dol", &mut payload.as_slice(), size).unwrap();
        assert_eq!(assert_frame(&output, "boot.dol", 0), payload);
    }
}

struct FailAfter {
    remaining: usize,
}

impl Write for FailAfter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        let written = bytes.len().min(self.remaining);
        self.remaining -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn send_propagates_failure_while_flushing_the_filename() {
    let payload = b"body";
    let mut output = FailAfter {
        remaining: 16 + payload.len(),
    };
    let result = send(
        &mut output,
        "boot.dol",
        &mut payload.as_slice(),
        payload.len(),
    );
    assert!(matches!(
        result,
        Err(WiiloadError::Io(io::ErrorKind::BrokenPipe))
    ));
}

#[cfg(feature = "async")]
impl futures::AsyncWrite for FailAfter {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::task::Poll::Ready(self.write(bytes))
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(self.flush())
    }

    fn poll_close(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

#[cfg(feature = "async")]
#[test]
fn send_async_flushes_the_complete_frame() {
    futures::executor::block_on(async {
        for size in [0, 1, 4096, CHUNK_SIZE + 7] {
            let payload = vec![0x42; size];
            let mut output = Vec::new();
            send_async(&mut output, "boot.dol", &mut payload.as_slice(), size)
                .await
                .unwrap();
            assert_eq!(assert_frame(&output, "boot.dol", 0), payload);
        }
    });
}

#[cfg(feature = "async")]
#[test]
fn send_async_propagates_failure_while_flushing_the_filename() {
    futures::executor::block_on(async {
        let payload = b"body";
        let mut output = FailAfter {
            remaining: 16 + payload.len(),
        };
        let result = send_async(
            &mut output,
            "boot.dol",
            &mut payload.as_slice(),
            payload.len(),
        )
        .await;
        assert!(matches!(
            result,
            Err(WiiloadError::Io(io::ErrorKind::BrokenPipe))
        ));
    });
}

#[cfg(feature = "compression")]
fn assert_compressed(bytes: &[u8], filename: &str, payload: &[u8]) {
    use std::io::Read;
    let compressed = assert_frame(bytes, filename, payload.len());
    let mut decoded = Vec::new();
    flate2::read::ZlibDecoder::new(compressed)
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, payload);
}

#[cfg(feature = "compression")]
#[test]
fn compressed_send_flushes_the_complete_frame() {
    let payload = vec![0x42; 4096];
    let mut output = Vec::new();
    compress_then_send(&mut output, "boot.elf", &mut payload.as_slice()).unwrap();
    assert_compressed(&output, "boot.elf", &payload);
}

#[cfg(all(
    feature = "compression",
    feature = "compression-async",
    feature = "async"
))]
#[test]
fn compressed_async_send_flushes_the_complete_frame() {
    futures::executor::block_on(async {
        for size in [0, 1, 4096, CHUNK_SIZE + 7] {
            let payload: Vec<_> = (0..size).map(|n| n as u8).collect();
            let mut output = Vec::new();
            compress_then_send_async(
                &mut output,
                "boot.elf",
                &mut payload.as_slice(),
                &mut futures::io::Cursor::new(Vec::new()),
            )
            .await
            .unwrap();
            assert_compressed(&output, "boot.elf", &payload);
        }
    });
}
