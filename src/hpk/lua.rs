use std::io;
use std::io::prelude::*;

#[cfg_attr(rustfmt, rustfmt_skip)]
static LUA_VALID_HEADER_32: [u8; 29] = [
    0x1B, 0x4C, 0x75, 0x61, 0x53, 0x00,
    0x19, 0x93, 0x0D, 0x0A, 0x1A, 0x0A,
    0x04, 0x04, 0x04, 0x04, 0x08,
    0x78, 0x56, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x28, 0x77, 0x40,
];
#[cfg_attr(rustfmt, rustfmt_skip)]
static LUA_VALID_HEADER_64: [u8; 33] = [
    0x1B, 0x4C, 0x75, 0x61, 0x53, 0x00,
    0x19, 0x93, 0x0D, 0x0A, 0x1A, 0x0A,
    0x04, 0x04, 0x04, 0x08, 0x08,
    0x78, 0x56, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x28, 0x77, 0x40,
];
#[cfg_attr(rustfmt, rustfmt_skip)]
static LUA_INVALID_HEADER_32: [u8; 27] = [
    0x1B, 0x4C, 0x75, 0x61, 0x53, 0x00,
    0x19, 0x93, 0x0D, 0x0A, 0x1A, 0x0A,
    0x04, 0x04, 0x08,
    0x78, 0x56, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x28, 0x77, 0x40,
];
#[cfg_attr(rustfmt, rustfmt_skip)]
static LUA_INVALID_HEADER_64: [u8; 31] = [
    0x1B, 0x4C, 0x75, 0x61, 0x53, 0x00,
    0x19, 0x93, 0x0D, 0x0A, 0x1A, 0x0A,
    0x04, 0x04, 0x08,
    0x78, 0x56, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x28, 0x77, 0x40,
];
const LUA_SIG: &[u8] = b"\x1BLua";
const LUA_VERSION53_FMT: &[u8] = b"\x53\x00";
const LUAC_DATA: &[u8] = b"\x19\x93\r\n\x1A\n";
const LUAC_INT32: u32 = 0x5678;
const LUAC_INT64: u64 = 0x5678;
const LUAC_NUM: f64 = 370.5;

mod parser {
    use super::*;
    use nom::*;

    #[derive(Debug, PartialEq)]
    pub enum Bits {
        Int32,
        Int64,
    }

    named!(lua_sig, tag!(LUA_SIG));
    named!(lua_version53_fmt, tag!(LUA_VERSION53_FMT));
    named!(luac_data, tag!(LUAC_DATA));
    named!(luac_valid_sizeof, take!(5));
    named!(luac_invalid_sizeof, take!(3));
    named!(
        luac_int32<Bits>,
        value!(Bits::Int32, verify!(le_u32, |val| val == LUAC_INT32))
    );
    named!(
        luac_int64<Bits>,
        value!(Bits::Int64, verify!(le_u64, |val| val == LUAC_INT64))
    );
    named!(luac_num<f64>, verify!(le_f64, |val| val == LUAC_NUM));
    named!(pub check_invalid_header<Bits>,
        do_parse!(
            lua_sig
        >>  lua_version53_fmt
        >>  luac_data
        >>  luac_invalid_sizeof
        >>  bits: alt_complete!(
                do_parse!(
                    bits: luac_int32    >>
                    luac_num            >>
                    (bits)
                ) |
                do_parse!(
                    bits: luac_int64    >>
                    luac_num            >>
                    (bits)
                )
            )
        >>  (bits)
        )
    );
    named!(pub check_valid_header<Bits>,
        do_parse!(
            lua_sig
        >>  lua_version53_fmt
        >>  luac_data
        >>  luac_valid_sizeof
        >>  bits: alt_complete!(
                do_parse!(
                    bits: luac_int32    >>
                    luac_num            >>
                    (bits)
                ) |
                do_parse!(
                    bits: luac_int64    >>
                    luac_num            >>
                    (bits)
                )
            )
        >>  (bits)
        )
    );
}

pub type LuaHeaderRewriteReader<R> =
    LuaHeaderRewriter<R, fn(&mut R, &mut [u8]) -> io::Result<usize>>;
pub type LuaHeaderRewriteWriter<W> = LuaHeaderRewriter<W, fn(&mut W, &[u8]) -> io::Result<usize>>;

pub struct LuaHeaderRewriter<T, F> {
    inner: T,
    done: bool,
    func: F,
}

impl<T, F> LuaHeaderRewriter<T, F> {
    fn new(inner: T, func: F) -> Self {
        Self {
            inner,
            done: false,
            func,
        }
    }
}

impl<R, F> Read for LuaHeaderRewriter<R, F>
where
    R: Read,
    F: Fn(&mut R, &mut [u8]) -> io::Result<usize>,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.done {
            let res = (self.func)(&mut self.inner, buf);
            self.done = true;
            res
        } else {
            self.inner.read(buf)
        }
    }
}

impl<W, F> Write for LuaHeaderRewriter<W, F>
where
    W: Write,
    F: Fn(&mut W, &[u8]) -> io::Result<usize>,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.done {
            let res = (self.func)(&mut self.inner, buf);
            self.done = true;
            res
        } else {
            self.inner.write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub fn cripple_header<R>(r: R) -> LuaHeaderRewriteReader<R>
where
    R: Read,
{
    LuaHeaderRewriter::new(r, read_with_invalid_header)
}

pub fn fix_header<W>(w: W) -> LuaHeaderRewriteWriter<W>
where
    W: Write,
{
    LuaHeaderRewriter::new(w, write_with_valid_header)
}

fn read_with_invalid_header<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut tmp = vec![0; buf.len()];
    match r.read(&mut tmp) {
        Ok(0) => Ok(0),
        Ok(n) => {
            let mut tmp = &tmp[0..n];
            match parser::check_valid_header(&tmp) {
                Ok((remaining, bits)) => {
                    let mut w = io::Cursor::new(buf);
                    let mut n = 0;
                    let hdr = match bits {
                        parser::Bits::Int32 => &LUA_INVALID_HEADER_32[..],
                        parser::Bits::Int64 => &LUA_INVALID_HEADER_64[..],
                    };
                    n += w.write(hdr)?;
                    n += w.write(remaining)?;
                    Ok(n)
                }
                Err(_) => tmp.read(buf),
            }
        }
        Err(e) => Err(e),
    }
}

fn write_with_valid_header<W: Write>(w: &mut W, buf: &[u8]) -> io::Result<usize> {
    match parser::check_invalid_header(buf) {
        Ok((remaining, bits)) => {
            let mut n = 0;
            let hdr = match bits {
                parser::Bits::Int32 => &LUA_VALID_HEADER_32[..],
                parser::Bits::Int64 => &LUA_VALID_HEADER_64[..],
            };
            n += w.write(hdr)? - 2; // ignore the two additional bytes
            n += w.write(remaining)?;
            Ok(n)
        }
        Err(_) => w.write(buf),
    }
}

// Tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_valid_header_parser() {
        assert_eq!(
            parser::check_valid_header(&LUA_VALID_HEADER_32),
            Ok((&b""[..], parser::Bits::Int32))
        );
        assert_eq!(
            parser::check_valid_header(&LUA_VALID_HEADER_64),
            Ok((&b""[..], parser::Bits::Int64))
        );
    }

    #[test]
    fn check_invalid_header_parser() {
        assert_eq!(
            parser::check_invalid_header(&LUA_INVALID_HEADER_32),
            Ok((&b""[..], parser::Bits::Int32))
        );
        assert_eq!(
            parser::check_invalid_header(&LUA_INVALID_HEADER_64),
            Ok((&b""[..], parser::Bits::Int64))
        );
    }

    #[test]
    fn header_rewrite() {
        let mut input = io::Cursor::new(vec![]);
        let mut buf = io::Cursor::new(vec![]);
        input.write(&LUA_VALID_HEADER_64).unwrap();
        input.write(&[0xCA, 0xFE, 0xCA, 0xFE]).unwrap();
        input.set_position(0);

        {
            let mut wrapper = cripple_header(&mut input);
            let n = io::copy(&mut wrapper, &mut buf).unwrap();
            assert_eq!(n, LUA_INVALID_HEADER_64.len() as u64 + 4);
        }
        assert_eq!(buf.get_ref().len(), LUA_INVALID_HEADER_64.len() + 4);
        assert_eq!(buf.get_ref()[0..31], LUA_INVALID_HEADER_64);

        let mut output = io::Cursor::new(vec![]);
        buf.set_position(0);

        {
            let mut wrapper = fix_header(&mut output);
            let n = io::copy(&mut buf, &mut wrapper).unwrap();

            // LuaHeaderRewriter has to lie here about the written bytes
            assert_eq!(n, LUA_INVALID_HEADER_64.len() as u64 + 4);
        }
        assert_eq!(output.position(), LUA_VALID_HEADER_64.len() as u64 + 4);

        assert_eq!(input.into_inner(), output.into_inner());
    }
}
// }}}

// vim: fdm=marker
