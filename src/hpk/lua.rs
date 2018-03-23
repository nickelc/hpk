use std::io::prelude::*;
use std::io;

#[cfg_attr(rustfmt, rustfmt_skip)]
const LUA_VALID_HEADER: [u8; 33] = [
    0x1B, 0x4C, 0x75, 0x61, 0x53, 0x00,
    0x19, 0x93, 0x0D, 0x0A, 0x1A, 0x0A,
    0x04, 0x04, 0x04, 0x08, 0x08,
    0x78, 0x56, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x28, 0x77, 0x40,
];
#[cfg_attr(rustfmt, rustfmt_skip)]
const LUA_INVALID_HEADER: [u8; 31] = [
    0x1B, 0x4C, 0x75, 0x61, 0x53, 0x00,
    0x19, 0x93, 0x0D, 0x0A, 0x1A, 0x0A,
    0x04, 0x04, 0x08,
    0x78, 0x56, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x28, 0x77, 0x40,
];
const LUA_SIG: &'static [u8] = b"\x1BLua";
const LUA_VERSION53_FMT: &'static [u8] = b"\x53\x00";
const LUAC_DATA: &'static [u8] = b"\x19\x93\r\n\x1A\n";
const LUAC_INT: u64 = 0x5678;
const LUAC_NUM: f64 = 370.5;

mod parser {
    use nom::*;
    use super::*;

    named!(lua_sig, tag!(LUA_SIG));
    named!(lua_version53_fmt, tag!(LUA_VERSION53_FMT));
    named!(luac_data, tag!(LUAC_DATA));
    named!(luac_valid_sizeof, take!(5));
    named!(luac_invalid_sizeof, take!(3));
    named!(luac_int<u64>, verify!(le_u64, |val| val == LUAC_INT));
    named!(luac_num<f64>, verify!(le_f64, |val| val == LUAC_NUM));
    named!(pub check_invalid_header<()>,
        do_parse!(
            lua_sig
        >>  lua_version53_fmt
        >>  luac_data
        >>  luac_invalid_sizeof
        >>  luac_int
        >>  luac_num
        >>  (())
        )
    );
    named!(pub check_valid_header<()>,
        do_parse!(
            lua_sig
        >>  lua_version53_fmt
        >>  luac_data
        >>  luac_valid_sizeof
        >>  luac_int
        >>  luac_num
        >>  (())
        )
    );
}

pub struct LuaHeaderRewriter<T, F> {
    inner: Option<T>,
    done: bool,
    func: F,
}

impl<T, F> LuaHeaderRewriter<T, F> {
    fn new(inner: T, func: F) -> Self {
        Self {
            inner: Some(inner),
            done: false,
            func: func,
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
            let res = (self.func)(self.inner.as_mut().unwrap(), buf);
            self.done = true;
            res
        } else {
            self.inner.as_mut().unwrap().read(buf)
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
            let res = (self.func)(self.inner.as_mut().unwrap(), buf);
            self.done = true;
            res
        } else {
            self.inner.as_mut().unwrap().write(buf)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.as_mut().unwrap().flush()
    }
}

pub fn cripple_header<R>(r: R) -> LuaHeaderRewriter<R, fn(&mut R, &mut [u8]) -> io::Result<usize>>
where
    R: Read,
{
    LuaHeaderRewriter::new(r, read_with_invalid_header)
}

pub fn fix_header<W>(w: W) -> LuaHeaderRewriter<W, fn(&mut W, &[u8]) -> io::Result<usize>>
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
                Ok((remaining, ())) => {
                    let mut w = io::Cursor::new(buf);
                    let mut n = 0;
                    n += w.write(&LUA_INVALID_HEADER)?;
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
        Ok((remaining, ())) => {
            let mut n = 0;
            n += w.write(&LUA_VALID_HEADER)? - 2; // ignore the two additional bytes
            n += w.write(remaining)?;
            Ok(n)
        }
        Err(_) => w.write(buf),
    }
}
