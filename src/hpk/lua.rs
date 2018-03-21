use std::io::prelude::*;
use std::io;
use byteorder::{WriteBytesExt, LE};

const LUA_SIG: &'static [u8] = b"\x1BLua";
const LUA_VERSION53_FMT: &'static [u8] = b"\x53\x00";
const LUAC_SIZEOF: &'static [u8] = b"\x04\x04\x04\x08\x08";
const LUAC_DATA: &'static [u8] = b"\x19\x93\r\n\x1A\n";
const LUAC_INT: u64 = 0x5678;
const LUAC_NUM: f64 = 370.5;

mod parser {
    use nom::*;
    use super::*;

    named!(lua_sig, tag!(LUA_SIG));
    named!(lua_version53_fmt, tag!(LUA_VERSION53_FMT));
    named!(luac_data, tag!(LUAC_DATA));
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
}

pub struct LuaHeaderRewriter<W: Write, F> {
    inner: Option<W>,
    done: bool,
    func: F,
}

impl<W, F> LuaHeaderRewriter<W, F>
where
    W: Write,
    F: Fn(&mut W, &[u8]) -> io::Result<usize>,
{
    fn new(inner: W, func: F) -> Self {
        Self {
            inner: Some(inner),
            done: false,
            func: func,
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

pub fn fix_header<W>(w: W) -> LuaHeaderRewriter<W, fn(&mut W, &[u8]) -> io::Result<usize>>
where
    W: Write,
{
    LuaHeaderRewriter::new(w, write_with_valid_header)
}

fn write_with_valid_header<W: Write>(w: &mut W, buf: &[u8]) -> io::Result<usize> {
    match parser::check_invalid_header(buf) {
        Ok((remaining, ())) => {
            let mut n = 0;
            n += w.write(LUA_SIG)?;
            n += w.write(LUA_VERSION53_FMT)?;
            n += w.write(LUAC_DATA)?;
            n += w.write(LUAC_SIZEOF)? - 2; // ignore the two added bytes
            w.write_u64::<LE>(LUAC_INT)?;
            w.write_f64::<LE>(LUAC_NUM)?;
            n += ::std::mem::size_of::<u64>();
            n += ::std::mem::size_of::<f64>();
            n += w.write(remaining)?;
            Ok(n)
        }
        Err(_) => w.write(buf),
    }
}
