use std::io::prelude::*;
use std::io;
use std::io::Cursor;

use flate2;
use lz4;

pub trait Decoder {
    fn decode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64>;
}

pub trait Encoder {
    fn encode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64>;
}

pub enum Zlib {}
pub enum Lz4 {}

impl Decoder for Lz4 {
    fn decode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut dec = lz4::Decoder::new(r)?;
        io::copy(&mut dec, w)
    }
}

impl Encoder for Lz4 {
    fn encode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut enc = lz4::EncoderBuilder::new().build(vec![])?;
        println!("copied {} bytes into lz4 encoder", io::copy(r, &mut enc)?);
        match enc.finish() {
            (buf, Ok(_)) => {
                let mut buf = Cursor::new(buf);
                io::copy(&mut buf, w)
            }
            (_, Err(e)) => Err(e)
        }
    }
}

impl Decoder for Zlib {
    fn decode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut dec = flate2::read::ZlibDecoder::new(r);
        io::copy(&mut dec, w)
    }
}


impl Encoder for Zlib {
    fn encode_chunk<R: Read, W: Write>(r: &mut R, w: &mut W) -> io::Result<u64> {
        let mut enc = flate2::write::ZlibEncoder::new(vec![], flate2::Compression::best());
        println!("copied {} bytes into zlib encoder", io::copy(r, &mut enc)?);
        match enc.finish() {
            Ok(buf) => {
                let mut buf = Cursor::new(buf);
                io::copy(&mut buf, w)
            }
            Err(e) => Err(e)
        }
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
    fn lz4() {
        let input = "Hello World".as_bytes();
        let mut buf = vec![];
        let mut output = vec![];
        Lz4::encode_chunk(&mut Cursor::new(input), &mut buf).unwrap();
        Lz4::decode_chunk(&mut Cursor::new(buf), &mut output).unwrap();
        assert_eq!(input, &output[..]);
    }
}
