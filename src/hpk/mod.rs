extern crate byteorder;
extern crate flate2;
#[cfg(feature = "lz4frame")]
extern crate lz4;
extern crate lz4_compress;
extern crate tempdir;
extern crate walkdir;

use std::fs::File;
use std::io::prelude::*;
use std::io;
use std::io::Cursor;
use std::io::SeekFrom;
use std::str;
use std::path::{Path, PathBuf};
use std::ffi::OsStr;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

pub mod compress;
mod read;
mod walk;

use read::FragmentedReader;
pub use self::walk::walk;

const HPK_SIG: [u8; 4] = *b"BPUL";
const HEADER_LENGTH: u8 = 36;

type HpkResult<T> = Result<T, HpkError>;

#[derive(Debug)]
pub enum HpkError {
    InvalidHeader,
    InvalidDirEntryName(str::Utf8Error),
    InvalidFragmentIndex,
    Io(io::Error),
    WalkDir(walkdir::Error),
}

impl From<io::Error> for HpkError {
    fn from(err: io::Error) -> HpkError {
        HpkError::Io(err)
    }
}

impl From<walkdir::Error> for HpkError {
    fn from(err: walkdir::Error) -> HpkError {
        HpkError::WalkDir(err)
    }
}

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

    pub fn read_from<T: Read>(mut r: T) -> HpkResult<Self> {
        let mut sig = [0; 4];
        r.read_exact(&mut sig)?;
        if !sig.eq(&HPK_SIG) {
            return Err(HpkError::InvalidHeader);
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

    pub fn write(&self, w: &mut Write) -> HpkResult<()> {
        w.write(&self._identifier)?;
        w.write_u32::<LittleEndian>(self.data_offset)?;
        w.write_u32::<LittleEndian>(self.fragments_per_file)?;
        w.write_u32::<LittleEndian>(self._unknown2)?;
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

    pub fn read_from<T: Read>(mut r: T) -> HpkResult<Fragment> {
        let offset = u64::from(r.read_u32::<LittleEndian>()?);
        let length = u64::from(r.read_u32::<LittleEndian>()?);
        Ok(Fragment { offset, length })
    }

    pub fn read_nth_from<T: Read>(n: usize, mut r: T) -> HpkResult<Vec<Fragment>> {
        let mut fragments = Vec::with_capacity(n);
        for _ in 0..n {
            fragments.push(Fragment::read_from(&mut r)?);
        }
        Ok(fragments)
    }

    pub fn new(offset: u64, length: u64) -> Fragment {
        Fragment { offset, length }
    }

    pub fn write(&self, w: &mut Write) -> HpkResult<()> {
        w.write_u32::<LittleEndian>(self.offset as u32)?;
        w.write_u32::<LittleEndian>(self.length as u32)?;

        Ok(())
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

    fn read_from<T: Read>(parent: &Path, depth: usize, mut r: T) -> HpkResult<DirEntry> {
        let fragment_index = r.read_u32::<LittleEndian>()?.checked_sub(1).ok_or(
            HpkError::InvalidFragmentIndex,
        )?;

        let ft = r.read_u32::<LittleEndian>().map(|t| if t == 0 {
            FileType::File(fragment_index as usize)
        } else {
            FileType::Dir(fragment_index as usize)
        })?;

        let name_length = r.read_u16::<LittleEndian>()?;
        let mut buf = vec![0; name_length as usize];
        r.read_exact(&mut buf)?;
        let name = str::from_utf8(&buf).map_err(
            |e| HpkError::InvalidDirEntryName(e),
        )?;

        Ok(DirEntry {
            path: parent.join(name),
            ft,
            depth,
        })
    }

    pub fn write(&self, w: &mut Write) -> HpkResult<()> {
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

pub fn get_compression<T: Read + Seek>(r: &mut T) -> Compression {
    let pos = r.seek(SeekFrom::Current(0)).expect("failed to get current position");
    let compression = match Compression::read_from(r) {
        Ok(c) => c,
        Err(_) => Compression::None,
    };
    r.seek(SeekFrom::Start(pos)).expect("failed to seek to previous position");

    compression
}

/// Compresses the data using the encoder used
///
/// if no data is written at all the hpk compression header is written without any chunks
/// it's the same behaviour as in a DLC file for Tropico 4
///
pub fn compress<T: compress::Encoder>(r: &mut Read, w: &mut Write) -> HpkResult<u64> {
    const CHUNK_SIZE: u64 = 32768;
    let mut inflated_length = 0;
    let mut output_buffer = vec![];
    let mut offsets = vec![];

    loop {
        let mut chunk = vec![];
        let mut t = r.take(CHUNK_SIZE);

        inflated_length += match io::copy(&mut t, &mut chunk) {
            Ok(0) => {
                // no data left.
                break;
            }
            Ok(n) => n as u32,
            Err(e) => return Err(HpkError::Io(e)),
        };

        let position = output_buffer.len() as u32;
        offsets.push(position);

        let mut chunk = Cursor::new(chunk);
        T::encode_chunk(&mut chunk, &mut output_buffer)?;
    }

    let header_size = CompressionHeader::write(inflated_length, offsets, w)?;

    Ok(header_size + io::copy(&mut Cursor::new(output_buffer), w)?)
}

fn decompress<T: compress::Decoder>(length: u64, r: &mut Read, w: &mut Write) -> HpkResult<u64> {
    let hdr = CompressionHeader::read_from(length, r)?;
    let mut written = 0;
    for chunk in &hdr.chunks {
        let mut buf = vec![0; chunk.length as usize];
        r.read_exact(&mut buf)?;
        written += match T::decode_chunk(&mut Cursor::new(&buf), w) {
            Ok(n) => n,
            Err(_) => {
                // chunk seems to be not compressed
                io::copy(&mut Cursor::new(buf), w)?
            }
        };
    }
    Ok(written)
}

#[derive(PartialEq)]
pub enum Compression {
    Zlib,
    Lz4,
    None,
}

impl std::fmt::Display for Compression {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match *self {
            Compression::Zlib => write!(f, "ZLIB"),
            Compression::Lz4 => write!(f, "LZ4"),
            Compression::None => write!(f, "None"),
        }
    }
}

impl Compression {
    pub fn is_compressed(&self) -> bool {
        match *self {
            Compression::None => false,
            _ => true,
        }
    }

    fn read_from<T: Read + ?Sized>(r: &mut T) -> HpkResult<Self> {
        let mut buf = [0; 4];
        match r.read_exact(&mut buf) {
            Ok(_) => {
                match (buf.eq(b"ZLIB"), buf.eq(b"LZ4 ")) {
                    (true, _) => Ok(Compression::Zlib),
                    (_, true) => Ok(Compression::Lz4),
                    (_, _) => Ok(Compression::None),
                }
            }
            Err(e) => return Err(HpkError::Io(e)),
        }
    }

    fn write_identifier(&self, w: &mut Write) -> HpkResult<u64> {
        match *self {
            Compression::Zlib => Ok(w.write(b"ZLIB")? as u64),
            Compression::Lz4 => Ok(w.write(b"LZ4 ")? as u64),
            Compression::None => Ok(0),
        }
    }
}

pub struct CompressionHeader {
    pub compressor: Compression,
    pub inflated_length: u32,
    pub chunk_size: u32,
    pub chunks: Vec<Chunk>,
}

#[derive(Copy, Clone)]
pub struct Chunk {
    pub offset: u64,
    pub length: u64,
}

impl CompressionHeader {

    pub fn read_from<T: Read + ?Sized>(length: u64, r: &mut T) -> HpkResult<CompressionHeader> {
        let compressor = Compression::read_from(r)?;

        let inflated_length = r.read_u32::<LittleEndian>()?;
        let chunk_size = r.read_u32::<LittleEndian>()?;
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
            Err(e) => return Err(HpkError::Io(e)),
        };

        Ok(CompressionHeader {
            compressor,
            inflated_length,
            chunk_size,
            chunks: chunks,
        })
    }

    pub fn write(inflated_length: u32, offsets: Vec<u32>, out: &mut Write) -> HpkResult<u64> {
        const CHUNK_SIZE: u32 = 32768;
        const HDR_SIZE: u32 = 12;

        Compression::Zlib.write_identifier(out)?;
        out.write_u32::<LittleEndian>(inflated_length)?;
        out.write_u32::<LittleEndian>(CHUNK_SIZE)?;

        let offsets_size = offsets.len() as u32 * 4;
        let offsets = offsets.iter().map(|x| HDR_SIZE + offsets_size + x);
        for offset in offsets {
            out.write_u32::<LittleEndian>(offset)?;
        }

        Ok((HDR_SIZE + offsets_size) as u64)
    }
}

pub fn extract<P>(file: P, dest: P) -> HpkResult<()>
where
    P: AsRef<Path>,
{
    let file = file.as_ref();
    let dest = dest.as_ref();
    let mut walk = walk(file)?;

    while let Some(entry) = walk.next() {
        if let Ok(entry) = entry {
            let path = dest.join(entry.path());
            if entry.is_dir() {
                if !path.exists() {
                    ::std::fs::create_dir(path)?;
                }
            } else {
                walk.read_file(&entry, |mut r| {
                    let mut out = File::create(path)?;
                    copy(&mut r, &mut out)?;
                    Ok(())
                })?;
            }
        }
    }
    Ok(())
}

pub fn copy<W>(r: &mut FragmentedReader<&File>, w: &mut W) -> HpkResult<u64>
where
    W: Write,
{
    match get_compression(r) {
        Compression::Lz4 => decompress::<compress::Lz4Block>(r.len(), r, w),
        Compression::Zlib => decompress::<compress::Zlib>(r.len(), r, w),
        Compression::None => io::copy(r, w).map_err(|e| HpkError::Io(e)),
    }
}

// struct CreateOptions {{{
pub struct CreateOptions {
}

impl CreateOptions {
    pub fn new() -> Self {
        Self { }
    }

    pub fn with_default_filedates_format(&mut self) {
    }

    pub fn with_short_filedates_format(&mut self) {
    }
}
// }}}

pub fn create<P, W>(options: CreateOptions, dir: P, w: &mut W) -> HpkResult<()>
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
    fn write_file<W>(file: &Path, w: &mut W) -> HpkResult<Fragment>
    where
        W: Write + Seek,
    {
        let extensions = vec!["lst", "lua", "xml", "tga", "dds", "xtex", "bin", "csv"];

        let _compress = file.extension()
            .map(|e| extensions.contains(&e.to_str().unwrap()))
            .unwrap_or(false);

        if _compress {
            let mut file = File::open(file)?;
            let position = w.seek(SeekFrom::Current(0))?;

            let n = compress::<compress::Zlib>(&mut file, w)?;

            Ok(Fragment::new(position, n))

        } else {
            let position = w.seek(SeekFrom::Current(0))?;
            let mut input = File::open(file)?;
            let n = io::copy(&mut input, w)?;

            Ok(Fragment::new(position, n))
        }
    }
    // }}}
}

// vim: fdm=marker
