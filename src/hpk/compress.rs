use std::io;
use std::io::prelude::*;
use std::io::Cursor;

use zstd::stream::Decoder as ZstdDecoder;

pub trait Decoder {
    fn decode_chunk<W: Write + ?Sized>(chunk: &[u8], w: &mut W) -> io::Result<u64>;
}

pub trait Encoder {
    fn encode_chunk<W: Write>(chunk: &[u8], w: &mut W) -> io::Result<u64>;
}

pub enum Zlib {}
pub enum Zstd {}
pub enum Lz4Block {}
#[allow(dead_code)]
#[cfg(feature = "lz4frame")]
pub enum Lz4Frame {}

impl Decoder for Lz4Block {
    fn decode_chunk<W: Write + ?Sized>(chunk: &[u8], w: &mut W) -> io::Result<u64> {
        match lz4_compress::decompress(chunk) {
            Ok(buf) => io::copy(&mut Cursor::new(&buf), w),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }
}

impl Encoder for Lz4Block {
    fn encode_chunk<W: Write>(chunk: &[u8], w: &mut W) -> io::Result<u64> {
        io::copy(&mut Cursor::new(lz4_compress::compress(chunk)), w)
    }
}

#[cfg(feature = "lz4frame")]
impl Decoder for Lz4Frame {
    fn decode_chunk<W: Write + ?Sized>(chunk: &[u8], w: &mut W) -> io::Result<u64> {
        let mut dec = lz4::Decoder::new(chunk)?;
        io::copy(&mut dec, w)
    }
}

#[cfg(feature = "lz4frame")]
impl Encoder for Lz4Frame {
    fn encode_chunk<W: Write>(mut chunk: &[u8], w: &mut W) -> io::Result<u64> {
        let mut enc = lz4::EncoderBuilder::new().build(vec![])?;
        io::copy(&mut chunk, &mut enc)?;
        match enc.finish() {
            (buf, Ok(_)) => {
                let mut buf = Cursor::new(buf);
                io::copy(&mut buf, w)
            }
            (_, Err(e)) => Err(e),
        }
    }
}

impl Decoder for Zlib {
    fn decode_chunk<W: Write + ?Sized>(chunk: &[u8], w: &mut W) -> io::Result<u64> {
        let mut dec = flate2::read::ZlibDecoder::new(chunk);
        io::copy(&mut dec, w)
    }
}

impl Encoder for Zlib {
    fn encode_chunk<W: Write>(mut chunk: &[u8], w: &mut W) -> io::Result<u64> {
        let mut enc = flate2::write::ZlibEncoder::new(vec![], flate2::Compression::best());
        io::copy(&mut chunk, &mut enc)?;
        match enc.finish() {
            Ok(buf) => {
                let mut buf = Cursor::new(buf);
                io::copy(&mut buf, w)
            }
            Err(e) => Err(e),
        }
    }
}

impl Decoder for Zstd {
    fn decode_chunk<W: Write + ?Sized>(chunk: &[u8], w: &mut W) -> io::Result<u64> {
        let mut dec = ZstdDecoder::new(chunk)?;
        io::copy(&mut dec, w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zlib() {
        let input = "Hello World".as_bytes();
        let mut buf = vec![];
        let mut output = vec![];
        Zlib::encode_chunk(input, &mut buf).unwrap();
        Zlib::decode_chunk(&buf, &mut output).unwrap();
        assert_eq!(input, &output[..]);
    }

    #[test]
    fn lz4_block() {
        let input = "Hello World".as_bytes();
        let mut buf = vec![];
        let mut output = vec![];
        Lz4Block::encode_chunk(input, &mut buf).unwrap();
        Lz4Block::decode_chunk(&buf, &mut output).unwrap();
        assert_eq!(input, &output[..]);
    }

    #[test]
    #[cfg(feature = "lz4frame")]
    fn lz4_frame() {
        let input = "Hello World".as_bytes();
        let mut buf = vec![];
        let mut output = vec![];
        Lz4Frame::encode_chunk(input, &mut buf).unwrap();
        Lz4Frame::decode_chunk(&buf, &mut output).unwrap();
        assert_eq!(input, &output[..]);
    }
}
