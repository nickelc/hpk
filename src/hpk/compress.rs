use std::io;
use std::io::prelude::*;
use std::io::Cursor;

use zstd::stream::Decoder as ZstdDecoder;

pub trait Decoder {
    fn decode_chunk<R: Read + ?Sized, W: Write + ?Sized>(r: &mut R, w: &mut W) -> io::Result<u64>;
}

pub trait Encoder {
    fn encode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64>;
}

pub enum Zlib {}
pub enum Zstd {}
pub enum Lz4Block {}
#[allow(dead_code)]
#[cfg(feature = "lz4frame")]
pub enum Lz4Frame {}

impl Decoder for Lz4Block {
    fn decode_chunk<R: Read + ?Sized, W: Write + ?Sized>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut buf = vec![];
        r.read_to_end(&mut buf)?;
        match lz4_compress::decompress(&buf) {
            Ok(buf) => io::copy(&mut Cursor::new(&buf), w),
            Err(e) => Err(io::Error::new(io::ErrorKind::Other, e)),
        }
    }
}

impl Encoder for Lz4Block {
    fn encode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut buf = vec![];
        r.read_to_end(&mut buf)?;
        io::copy(&mut Cursor::new(lz4_compress::compress(&buf)), w)
    }
}

#[cfg(feature = "lz4frame")]
impl Decoder for Lz4Frame {
    fn decode_chunk<R: Read + ?Sized, W: Write + ?Sized>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut dec = lz4::Decoder::new(r)?;
        io::copy(&mut dec, w)
    }
}

#[cfg(feature = "lz4frame")]
impl Encoder for Lz4Frame {
    fn encode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut enc = lz4::EncoderBuilder::new().build(vec![])?;
        io::copy(r, &mut enc)?;
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
    fn decode_chunk<R: Read + ?Sized, W: Write + ?Sized>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut dec = flate2::read::ZlibDecoder::new(r);
        io::copy(&mut dec, w)
    }
}

impl Encoder for Zlib {
    fn encode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut enc = flate2::write::ZlibEncoder::new(vec![], flate2::Compression::best());
        io::copy(r, &mut enc)?;
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
    fn decode_chunk<R: Read + ?Sized, W: Write + ?Sized>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut dec = ZstdDecoder::new(r)?;
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
        Zlib::encode_chunk(&mut Cursor::new(input), &mut buf).unwrap();
        Zlib::decode_chunk(&mut Cursor::new(buf), &mut output).unwrap();
        assert_eq!(input, &output[..]);
    }

    #[test]
    fn lz4_block() {
        let input = "Hello World".as_bytes();
        let mut buf = vec![];
        let mut output = vec![];
        Lz4Block::encode_chunk(&mut Cursor::new(input), &mut buf).unwrap();
        Lz4Block::decode_chunk(&mut Cursor::new(buf), &mut output).unwrap();
        assert_eq!(input, &output[..]);
    }

    #[test]
    #[cfg(feature = "lz4frame")]
    fn lz4_frame() {
        let input = "Hello World".as_bytes();
        let mut buf = vec![];
        let mut output = vec![];
        Lz4Frame::encode_chunk(&mut Cursor::new(input), &mut buf).unwrap();
        Lz4Frame::decode_chunk(&mut Cursor::new(buf), &mut output).unwrap();
        assert_eq!(input, &output[..]);
    }
}
