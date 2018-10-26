use std::cmp;
use std::io;
use std::io::prelude::*;
use std::io::SeekFrom;

use super::Fragment;

struct FragmentState {
    offset: u64,
    length: u64,
    end_pos: u64,
    limit: u64,
}

pub struct FragmentedReader<T> {
    inner: T,
    length: u64,
    pos: u64,
    fragments: Vec<FragmentState>,
}

impl<T: Read + Seek> FragmentedReader<T> {
    pub fn new(inner: T, fragments: &[Fragment]) -> Self {
        let states: Vec<_> = fragments
            .iter()
            .map(|f| FragmentState {
                offset: f.offset,
                length: f.length,
                end_pos: 0,
                limit: f.length,
            }).scan(0, |state, mut f| {
                *state += f.length;
                f.end_pos = *state;
                Some(f)
            }).collect();

        let length = fragments.iter().map(|f| f.length).sum();

        Self {
            inner,
            length,
            pos: 0,
            fragments: states,
        }
    }

    fn set_position(&mut self, pos: u64) -> io::Result<()> {
        if self.pos == pos {
            return Ok(());
        }

        let mut limit = pos;
        for f in &mut self.fragments {
            let n = cmp::min(f.length, limit);
            f.limit = f.length - n;
            limit -= n;

            // read will seek when limit == length
            if f.limit > 0 && f.limit != f.length {
                self.inner.seek(SeekFrom::Start(f.offset + n))?;
            }
        }
        self.pos = pos;
        Ok(())
    }

    pub fn len(&self) -> u64 {
        self.length
    }

    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: Read + Seek> Read for FragmentedReader<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let current = self
            .fragments
            .iter()
            .rposition(|f| f.end_pos <= self.pos)
            .map_or(0, |i| i + 1);

        if let Some(f) = self.fragments.get_mut(current) {
            // Nothing has been read yet? seek to fragment start
            if f.limit == f.length {
                self.inner.seek(SeekFrom::Start(f.offset))?;
            }

            let max = cmp::min(buf.len() as u64, f.limit) as usize;
            let n = self.inner.read(&mut buf[..max])?;
            self.pos += n as u64;
            f.limit -= n as u64;
            return Ok(n);
        }
        Ok(0)
    }
}

impl<T: Read + Seek> Seek for FragmentedReader<T> {
    fn seek(&mut self, style: SeekFrom) -> io::Result<u64> {
        let (base_pos, offset) = match style {
            SeekFrom::Start(n) => {
                self.set_position(n)?;
                return Ok(n);
            }
            SeekFrom::End(n) => (self.length, n),
            SeekFrom::Current(n) => (self.pos, n),
        };

        let new_pos = if offset >= 0 {
            base_pos.checked_add(offset as u64)
        } else {
            base_pos.checked_sub((offset.wrapping_neg()) as u64)
        };
        match new_pos {
            Some(n) => {
                self.set_position(n)?;
                Ok(n)
            }
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative or overflowing position",
            )),
        }
    }
}

// Tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;
    use std::ptr;

    // macro: create_buffer {{{
    macro_rules! create_buffer {
        ($size:expr, $init:expr, $data: expr) => {{
            let mut buf: Vec<u8> = vec![$init; $size];

            let mut iter = $data.iter();
            while let Some(&(start, end, val)) = iter.next() {
                let slice = &mut buf[start as usize..(start + end) as usize];
                unsafe {
                    ptr::write_bytes(slice.as_mut_ptr(), val, slice.len());
                }
            }
            buf
        }};
    }
    // }}}

    // macro: create_fragments {{{
    macro_rules! create_fragments {
        ($x:expr) => {
            $x.iter()
                .map(|x| Fragment::new(x.0, x.1))
                .collect::<Vec<_>>()
        };
    }
    // }}}

    // macro: create_fragmented_reader {{{
    macro_rules! create_fragmented_reader {
        ($buffer_size:expr, $initial_value:expr, $offsets:expr) => {{
            let data = create_buffer!($buffer_size, $initial_value, $offsets);
            let fragments = create_fragments!($offsets);

            let cur = Cursor::new(data);
            FragmentedReader::new(cur, &fragments)
        }};
    }
    // }}}

    // macro: print_buf {{{
    macro_rules! print_buf {
        ($indent:expr, $buf:expr) => {
            for row in $buf.chunks(16) {
                print!($indent);
                for col in row.chunks(2) {
                    match col.len() {
                        2 => print!("{:02X}{:02X} ", col[0], col[1]),
                        1 => print!("{:02X}", col[0]),
                        _ => unreachable!(),
                    };
                }
                println!();
            }
        };
    }
    // }}}

    // trait PrintState {{{
    trait PrintState {
        fn print_state(&mut self);
    }

    impl<T: Read + Seek> PrintState for FragmentedReader<T> {
        fn print_state(&mut self) {
            println!("pos: {}", self.pos);
            println!("inner pos: {:?}", self.inner.seek(SeekFrom::Current(0)));
            print!("positions: ");
            for pos in &self.fragments {
                print!("{} ", pos.end_pos);
            }
            println!();
            println!("fragment states:");
            for (i, s) in self.fragments.iter().enumerate() {
                println!(
                    "{}: off: {} len: {} limit: {}",
                    i, s.offset, s.length, s.limit
                );
            }
        }
    }
    // }}}

    #[test]
    fn fragmented_reader_read() {
        let sample = vec![
            (10, 12, 0x11),
            (32, 20, 0x22),
            (60, 35, 0x33),
            (100, 22, 0x44),
        ];
        let mut r = create_fragmented_reader!(128, 0xFF, sample);

        assert_eq!(r.len(), 89);

        let mut buf = vec![0; r.len() as usize];

        let n = r.read(&mut buf).unwrap();
        assert_eq!(n, 12);
        let mut start = n;
        let n = r.read(&mut buf[start..]).unwrap();
        assert_eq!(n, 20);
        start += n;
        let n = r.read(&mut buf[start..]).unwrap();
        assert_eq!(n, 35);
        start += n;
        let n = r.read(&mut buf[start..]).unwrap();
        assert_eq!(n, 22);

        // EOF of fragmented file reached
        let n = r.read(&mut buf).unwrap();
        assert_eq!(n, 0);

        let data = r.into_inner().into_inner();
        println!("Original data: len={}", data.len());
        print_buf!("  ", data);
        println!("fragmented data: len={}", buf.len());
        print_buf!("  ", buf);

        // check output buffer
        assert_eq!(&buf[0..12], [0x11; 12]);
        assert_eq!(&buf[12..32], [0x22; 20]);
        assert_eq!(&buf[32..64], [0x33; 32]);
        assert_eq!(&buf[64..67], [0x33; 3]);
        assert_eq!(&buf[67..89], [0x44; 22]);
    }

    #[test]
    fn fragmented_reader_read_exact() {
        let sample = vec![
            (10, 12, 0x11),
            (32, 20, 0x22),
            (60, 35, 0x33),
            (100, 22, 0x44),
        ];
        let mut r = create_fragmented_reader!(128, 0xFF, sample);

        assert_eq!(r.len(), 89);

        let mut buf = vec![0; r.len() as usize];

        r.read_exact(&mut buf).unwrap();

        // EOF of fragmented file reached
        let n = r.read(&mut buf).unwrap();
        assert_eq!(n, 0);

        let data = r.into_inner().into_inner();
        println!("Original data: len={}", data.len());
        print_buf!("  ", data);
        println!("fragmented data: len={}", buf.len());
        print_buf!("  ", buf);

        // check output buffer
        assert_eq!(&buf[0..12], [0x11; 12]);
        assert_eq!(&buf[12..32], [0x22; 20]);
        assert_eq!(&buf[32..64], [0x33; 32]);
        assert_eq!(&buf[64..67], [0x33; 3]);
        assert_eq!(&buf[67..89], [0x44; 22]);
    }

    #[test]
    fn fragmented_reader_seek() {
        let sample = vec![
            (10, 12, 0x11),
            (32, 20, 0x22),
            (60, 35, 0x33),
            (100, 22, 0x44),
        ];
        let mut r = create_fragmented_reader!(128, 0xFF, sample);

        assert_eq!(r.len(), 89);

        let mut buf = [0; 2];
        let ret = r.seek(SeekFrom::Start(11)).unwrap();
        assert_eq!(ret, 11);
        r.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [0x11, 0x22]);

        let ret = r.seek(SeekFrom::Current(18)).unwrap();
        assert_eq!(ret, 31);
        r.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [0x22, 0x33]);

        let ret = r.seek(SeekFrom::End(-23)).unwrap();
        assert_eq!(ret, 66);
        r.read_exact(&mut buf).unwrap();
        assert_eq!(buf, [0x33, 0x44]);

        let ret = r.seek(SeekFrom::End(0)).unwrap();
        assert_eq!(ret, 89);

        assert_eq!(r.read(&mut buf).unwrap(), 0);
        let ret = r.seek(SeekFrom::Start(12)).unwrap();
        assert_eq!(ret, 12);

        let mut buf = [0; 20];
        let n = r.read(&mut buf).unwrap();
        assert_eq!(n, 20);
        assert_eq!(buf, [0x22; 20]);
    }
}
// }}}

// vim: fdm=marker
