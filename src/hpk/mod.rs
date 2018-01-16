extern crate byteorder;
extern crate flate2;
extern crate lz4;
extern crate walkdir;

use std::cmp;
use std::fs::File;
use std::io::prelude::*;
use std::io;
use std::io::Cursor;
use std::io::SeekFrom;
use std::str;
use std::path::{Path, PathBuf};
use std::ffi::OsStr;

use byteorder::{LittleEndian, BigEndian, ReadBytesExt, WriteBytesExt};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

mod compression;
mod walk;

pub use self::walk::walk;

const HPK_SIG: [u8; 4] = *b"BPUL";
const HEADER_LENGTH: u8 = 36;

pub struct Header {
    _identifier: [u8; 4],
    pub data_offset: u32,
    pub fragments_per_file: u32,
    _unknown2: u32,
    pub fragments_residual_offset: u64,
    pub fragments_residual_count: u64,
    _unknown5: u32,
    pub fragmented_filesystem_offset: u64,
    pub fragmented_filesystem_length: u64,
}

impl Header {

    pub fn new(fragmented_filesystem_offset: u64, fragmented_filesystem_length: u64) -> Header {
        Header {
            _identifier: HPK_SIG,
            data_offset: 36,
            fragments_per_file: 1,
            _unknown2: 0xFF,
            fragments_residual_offset: 0,
            fragments_residual_count: 0,
            _unknown5: 1,
            fragmented_filesystem_offset,
            fragmented_filesystem_length,
        }
    }

    pub fn read_from<T: Read>(mut r: T) -> io::Result<Self> {
        let mut sig = [0; 4];
        r.read_exact(&mut sig)?;
        if !sig.eq(&HPK_SIG) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid hpk header"));
        }
        Ok(Header {
            _identifier: sig,
            data_offset: r.read_u32::<LittleEndian>()?,
            fragments_per_file: r.read_u32::<LittleEndian>()?,
            _unknown2: r.read_u32::<LittleEndian>()?,
            fragments_residual_offset: r.read_u32::<LittleEndian>()? as u64,
            fragments_residual_count: r.read_u32::<LittleEndian>()? as u64,
            _unknown5: r.read_u32::<LittleEndian>()?,
            fragmented_filesystem_offset: r.read_u32::<LittleEndian>()? as u64,
            fragmented_filesystem_length: r.read_u32::<LittleEndian>()? as u64,
        })
    }

    pub fn write(&self, w: &mut Write) -> io::Result<()> {
        w.write(&self._identifier)?;
        w.write_u32::<LittleEndian>(self.data_offset)?;
        w.write_u32::<LittleEndian>(self.fragments_per_file)?;
        w.write_u32::<LittleEndian>(self._unknown2).unwrap();
        w.write_u32::<LittleEndian>(self.fragments_residual_offset as u32)?;
        w.write_u32::<LittleEndian>(self.fragments_residual_count as u32)?;
        w.write_u32::<LittleEndian>(self._unknown5)?;
        w.write_u32::<LittleEndian>(self.fragmented_filesystem_offset as u32)?;
        w.write_u32::<LittleEndian>(self.fragmented_filesystem_length as u32)?;

        Ok(())
    }

    pub fn filesystem_entries(&self) -> usize {
        const FRAGMENT_SIZE: u32 = 8;
        (self.fragmented_filesystem_length as u32 / (FRAGMENT_SIZE * self.fragments_per_file)) as usize
    }
}

#[derive(Clone, Debug)]
pub struct Fragment {
    pub offset: u64,
    pub length: u64,
}

impl Fragment {

    pub fn read_from<T: Read>(mut r: T) -> io::Result<Fragment> {
        let offset = u64::from(r.read_u32::<LittleEndian>()?);
        let length = u64::from(r.read_u32::<LittleEndian>()?);
        Ok(Fragment { offset, length })
    }

    pub fn read_nth_from<T: Read>(n: usize, mut r: T) -> io::Result<Vec<Fragment>> {
        let mut fragments = Vec::with_capacity(n);
        for _ in 0..n {
            fragments.push(Fragment::read_from(&mut r)?);
        }
        Ok(fragments)
    }

    pub fn new(offset: u64, length: u64) -> Fragment {
        Fragment { offset, length }
    }

    pub fn write(&self, w: &mut Write) -> io::Result<()> {
        w.write_u32::<LittleEndian>(self.offset as u32)?;
        w.write_u32::<LittleEndian>(self.length as u32)?;

        Ok(())
    }
}

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

    pub fn new(inner: T, fragments: Vec<Fragment>) -> Self {
        let states: Vec<_> = fragments
            .iter()
            .map(|f| {
                FragmentState {
                    offset: f.offset,
                    length: f.length,
                    end_pos: 0,
                    limit: f.length,
                }
            })
            .scan(0, |state, mut f| {
                *state += f.length;
                f.end_pos = *state;
                Some(f)
            })
            .collect();

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
        let current = self.fragments
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

enum FileType {
    Dir(usize),
    File(usize),
}

pub struct DirEntry {
    path: PathBuf,
    ft: FileType,
    depth: usize,
}

impl DirEntry {

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file_name(&self) -> &OsStr {
        self.path.file_name().unwrap_or_else(
            || self.path.as_os_str(),
        )
    }

    pub fn index(&self) -> usize {
        match self.ft {
            FileType::Dir(idx) => idx,
            FileType::File(idx) => idx,
        }
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn is_dir(&self) -> bool {
        if let FileType::Dir(_) = self.ft {
            true
        } else {
            false
        }
    }

    fn new_root() -> Self {
        DirEntry {
            path: PathBuf::new(),
            ft: FileType::Dir(0),
            depth: 0,
        }
    }

    pub fn new_dir<P: AsRef<Path>>(path: P, index: usize, depth: usize) -> Self {
        DirEntry {
            path: path.as_ref().to_path_buf(),
            ft: FileType::Dir(index),
            depth,
        }
    }

    pub fn new_file<P: AsRef<Path>>(path: P, index: usize, depth: usize) -> Self {
        DirEntry {
            path: path.as_ref().to_path_buf(),
            ft: FileType::File(index),
            depth,
        }
    }

    fn read_from<T: Read>(parent: &Path, depth: usize, mut r: T) -> io::Result<DirEntry> {
        let fragment_index = r.read_u32::<LittleEndian>()?.checked_sub(1).ok_or_else(
            || {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "invalid data for fragment index",
                )
            },
        )?;

        let ft = r.read_u32::<LittleEndian>().map(|t| if t == 0 {
            FileType::File(fragment_index as usize)
        } else {
            FileType::Dir(fragment_index as usize)
        })?;

        let name_length = r.read_u16::<LittleEndian>()?;
        let mut buf = vec![0; name_length as usize];
        r.read_exact(&mut buf)?;
        let name = str::from_utf8(&buf).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid name for entry")
        })?;

        Ok(DirEntry {
            path: parent.join(name),
            ft,
            depth,
        })
    }

    pub fn write(&self, w: &mut Write) -> io::Result<()> {
        let (index, _type) = match self.ft {
            FileType::Dir(index) => (index, 1),
            FileType::File(index) => (index, 0),
        };
        w.write_u32::<LittleEndian>(index as u32)?;
        w.write_u32::<LittleEndian>(_type)?;
        let name = self.path.file_name().unwrap().to_str().unwrap();
        w.write_u16::<LittleEndian>(name.len() as u16)?;
        w.write(name.as_bytes())?;
        Ok(())
    }
}

pub struct CompressionHeader {
    _identifier: [u8; 4],
    pub inflated_length: u32,
    pub chunk_size: i32,
    pub chunks: Vec<Chunk>,
}

#[derive(Copy, Clone)]
pub struct Chunk {
    pub offset: u64,
    pub length: u64,
}

impl CompressionHeader {

    pub fn is_compressed<T: Read + Seek>(r: &mut T) -> bool {
        let mut buf = [0; 4];
        let pos = r.seek(SeekFrom::Current(0)).expect("failed to get current position");
        let compressed = match r.read_exact(&mut buf) {
            Ok(_) => buf.eq(b"ZLIB"),
            Err(_) => false,
        };
        r.seek(SeekFrom::Start(pos)).expect("failed seek to previous position");

        compressed
    }

    pub fn read_from<T: Read>(length: u64, r: &mut T) -> io::Result<CompressionHeader> {
        let mut _identifier = [0; 4];
        r.read_exact(&mut _identifier)?;

        let inflated_length = r.read_u32::<LittleEndian>()?;
        let chunk_size = r.read_i32::<LittleEndian>()?;
        let chunks = match r.read_u32::<LittleEndian>() {
            Ok(val) => {
                let mut offsets = vec![val as u64];
                if offsets[0] != 16 {
                    for _ in 0..((offsets[0] - 16) / 4) {
                        offsets.push(r.read_u32::<LittleEndian>()? as u64);
                    }
                }
                let mut chunks = vec![
                    Chunk {
                        offset: 0,
                        length: 0,
                    };
                    offsets.len()
                ];
                let mut len = length;
                for (i, offset) in offsets.iter().enumerate().rev() {
                    chunks[i] = Chunk {
                        offset: *offset,
                        length: len - offset,
                    };
                    len -= chunks[i].length;
                }
                chunks
            }
            Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => vec![],
            Err(e) => return Err(e),
        };

        Ok(CompressionHeader {
            _identifier,
            inflated_length,
            chunk_size,
            chunks: chunks,
        })
    }

    pub fn write(inflated_length: u32, offsets: Vec<i32>, out: &mut Write) -> io::Result<()> {
        const CHUNK_SIZE: i32 = 32768;
        const HDR_SIZE: i32 = 12;

        out.write("ZLIB".as_bytes())?;
        out.write_u32::<LittleEndian>(inflated_length)?;
        out.write_i32::<LittleEndian>(CHUNK_SIZE)?;

        let offsets_size = offsets.len() as i32 * 4;
        let offsets = offsets.iter().map(|x| HDR_SIZE + offsets_size + x);
        for offset in offsets {
            out.write_i32::<LittleEndian>(offset)?;
        }

        Ok(())
    }
}

pub fn copy<W>(mut r: FragmentedReader<&File>, mut w: &mut W) -> io::Result<u64>
where
    W: Write,
{
    if CompressionHeader::is_compressed(&mut r) {
        let mut written = 0;
        let hdr = CompressionHeader::read_from(r.len(), &mut r)?;
        for chunk in &hdr.chunks {
            r.seek(SeekFrom::Start(chunk.offset))?;

            // quick check of the zlib header
            let check = r.read_u16::<BigEndian>()?;
            let is_zlib = check % 31 == 0;

            if is_zlib {
                r.seek(SeekFrom::Start(chunk.offset))?;
                let take = r.take(chunk.length);
                let mut dec = ZlibDecoder::new(take);
                if let Ok(n) = io::copy(&mut dec, &mut w) {
                    written += n;
                    r = dec.into_inner().into_inner();
                    continue;
                }
                r = dec.into_inner().into_inner();
            }
            // chunk is not compressed
            r.seek(SeekFrom::Start(chunk.offset))?;
            let mut take = r.take(chunk.length);
            written += io::copy(&mut take, &mut w)?;
            r = take.into_inner();
        }
        Ok(written)
    } else {
        io::copy(&mut r, &mut w)
    }
}

pub fn create<P, W>(dir: P, w: &mut W) -> io::Result<()>
where
    P: AsRef<Path>,
    W: Write + Seek,
{
    use std::collections::HashMap;
    use walkdir::WalkDir;

    // macro: strip_prefix {{{
    macro_rules! strip_prefix {
        (dir $path: expr) => ({
            let path = $path.strip_prefix(&dir).unwrap();
            let parent = path.parent();
            (path, parent)
        });
        (file $path: expr) => ({
            let (path, parent) = strip_prefix!(dir $path);
            (path, parent.unwrap())
        })
    }
    // }}}

    let walkdir = WalkDir::new(&dir).contents_first(true).sort_by(|a, b| {
        a.file_name().cmp(b.file_name())
    });
    let mut fragments: Vec<Fragment> = vec![];
    let mut stack = HashMap::new();

    w.seek(SeekFrom::Start(HEADER_LENGTH as u64))?;

    for entry in walkdir {
        let entry = entry?;

        if entry.file_type().is_file() {
            let (path, parent) = strip_prefix!(file entry.path());

            fragments.push(write_file(entry.path(), w)?);
            let index = fragments.len() + 1;
            let parent_buf = stack.entry(parent.to_path_buf()).or_insert_with(Vec::new);
            let dent = DirEntry::new_file(path, index, entry.depth());
            dent.write(parent_buf)?;

        } else if entry.file_type().is_dir() {
            let (path, parent) = strip_prefix!(dir entry.path());
            let dir_buffer = stack.remove(&path.to_path_buf()).unwrap_or_else(Vec::new);

            let position = w.seek(SeekFrom::Current(0))?;
            let mut r = Cursor::new(dir_buffer);
            io::copy(&mut r, w)?;
            let current_pos = w.seek(SeekFrom::Current(0))?;

            let fragment = Fragment::new(position, current_pos - position);
            if entry.depth() > 0 {
                fragments.push(fragment);
                let index = fragments.len() + 1;
                let dent = DirEntry::new_dir(path, index, entry.depth());
                let parent_buf = stack
                    .entry(parent.expect("bug?").to_path_buf())
                    .or_insert_with(Vec::new);
                dent.write(parent_buf)?;

            } else {
                // root dir must be the first fragment
                fragments.insert(0, fragment);
            }
        }
    }

    let fragment_pos = w.seek(SeekFrom::Current(0))?;
    for fragment in fragments {
        fragment.write(w)?;
    }

    let current_pos = w.seek(SeekFrom::Current(0))?;
    w.seek(SeekFrom::Start(0))?;
    let header = Header::new(fragment_pos, current_pos - fragment_pos);
    header.write(w)?;

    return Ok(());

    // write_file {{{
    fn write_file<W>(file: &Path, w: &mut W) -> io::Result<Fragment>
    where
        W: Write + Seek,
    {
        const CHUNK_SIZE: u64 = 32768;
        let extensions = vec!["lst", "lua", "xml", "tga", "dds", "xtex", "bin", "csv"];

        let compress = file.extension()
            .map(|e| extensions.contains(&e.to_str().unwrap()))
            .unwrap_or(false);

        if compress {
            let length = file.metadata()?.len();
            let mut file = File::open(file)?;
            let mut output_buffer = vec![];
            let mut offsets = vec![];

            loop {
                if length == 0 {
                    // breaking here writes a hpk zlib header without any chunks
                    // it's the same behaviour as in a DLC file for Tropico 4
                    break;
                }

                let position = output_buffer.len() as i32;
                offsets.push(position);

                let mut chunk = vec![];
                let mut t = file.take(CHUNK_SIZE);
                io::copy(&mut t, &mut chunk)?;
                file = t.into_inner();

                let mut encoder = ZlibEncoder::new(vec![], Compression::best());
                let mut chunk = Cursor::new(chunk);
                io::copy(&mut chunk, &mut encoder)?;

                match encoder.finish() {
                    Ok(ref buf) if buf.len() as u64 == CHUNK_SIZE => {
                        io::copy(&mut chunk, &mut output_buffer)?;
                    }
                    Ok(buf) => {
                        let mut buf = Cursor::new(buf);
                        io::copy(&mut buf, &mut output_buffer)?;
                    }
                    Err(_) => {}
                };

                if file.seek(SeekFrom::Current(0))? == length {
                    break;
                }
            }

            let position = w.seek(SeekFrom::Current(0))?;

            CompressionHeader::write(length as u32, offsets, w)?;
            io::copy(&mut Cursor::new(output_buffer), w)?;

            let current_pos = w.seek(SeekFrom::Current(0))?;

            Ok(Fragment::new(position, current_pos - position))

        } else {
            let position = w.seek(SeekFrom::Current(0))?;
            let mut input = File::open(file)?;
            io::copy(&mut input, w)?;
            let current_pos = w.seek(SeekFrom::Current(0))?;

            Ok(Fragment::new(position, current_pos - position))
        }
    }
    // }}}
}

// Tests {{{
#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;
    use std::ptr;

    // macro: create_buffer {{{
    macro_rules! create_buffer {
        ($size:expr, $init:expr, $data: expr) => (
            {
                let mut buf: Vec<u8> = vec![$init; $size];

                let mut iter = $data.iter();
                while let Some(&(start, end, val)) = iter.next() {
                    let slice = &mut buf[start as usize..(start + end) as usize];
                    unsafe {
                        ptr::write_bytes(slice.as_mut_ptr(), val, slice.len());
                    }
                }
                buf
            }
        );
    }
    // }}}

    // macro: create_fragments {{{
    macro_rules! create_fragments {
        ($x:expr) => (
            $x.iter().map(|x| Fragment::new(x.0, x.1))
                .collect::<Vec<_>>()
        );
    }
    // }}}

    // macro: create_fragmented_reader {{{
    macro_rules! create_fragmented_reader {
        ($buffer_size:expr, $initial_value:expr, $offsets:expr) => (
            {
                let data = create_buffer!($buffer_size, $initial_value, $offsets);
                let fragments = create_fragments!($offsets);

                let cur = Cursor::new(data);
                FragmentedReader::new(cur, fragments)
            }
        )
    }
    // }}}

    // macro: print_buf {{{
    macro_rules! print_buf {
        ($indent:expr, $buf:expr) => (
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
        )
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
                    i,
                    s.offset,
                    s.length,
                    s.limit
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
