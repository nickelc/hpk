use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::Cursor;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::str;

use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use glob::Pattern;

pub mod compress;
mod lua;
mod read;
mod walk;

pub use crate::read::FragmentedReader;
pub use crate::walk::{walk, HpkIter};

const HPK_SIG: [u8; 4] = *b"BPUL";
const HEADER_LENGTH: u8 = 36;

/// The Windows epoch starts 1601-01-01T00:00:00Z. It's `SEC_TO_UNIX_EPOCH` seconds
/// before the Unix epoch 1970-01-01T00:00:00Z.
const SEC_TO_UNIX_EPOCH: i64 = 11_644_473_600;
const WINDOWS_TICKS: i64 = 10_000_000;

type HpkResult<T> = Result<T, HpkError>;

#[derive(Debug)]
pub enum HpkError {
    InvalidHeader,
    InvalidDirEntryName,
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
    fn new(fragmented_filesystem_offset: u64, fragmented_filesystem_length: u64) -> Header {
        Header {
            _identifier: HPK_SIG,
            data_offset: 36,
            fragments_per_file: 1,
            _unknown2: 0xFFFF_FFFF,
            fragments_residual_offset: 0,
            fragments_residual_count: 0,
            _unknown5: 1,
            fragmented_filesystem_offset,
            fragmented_filesystem_length,
        }
    }

    fn read_from<T: Read>(mut r: T) -> HpkResult<Self> {
        let mut sig = [0; 4];
        r.read_exact(&mut sig)?;
        if !sig.eq(&HPK_SIG) {
            return Err(HpkError::InvalidHeader);
        }
        Ok(Header {
            _identifier: sig,
            data_offset: r.read_u32::<LE>()?,
            fragments_per_file: r.read_u32::<LE>()?,
            _unknown2: r.read_u32::<LE>()?,
            fragments_residual_offset: u64::from(r.read_u32::<LE>()?),
            fragments_residual_count: u64::from(r.read_u32::<LE>()?),
            _unknown5: r.read_u32::<LE>()?,
            fragmented_filesystem_offset: u64::from(r.read_u32::<LE>()?),
            fragmented_filesystem_length: u64::from(r.read_u32::<LE>()?),
        })
    }

    fn write(&self, w: &mut dyn Write) -> HpkResult<()> {
        w.write_all(&self._identifier)?;
        w.write_u32::<LE>(self.data_offset)?;
        w.write_u32::<LE>(self.fragments_per_file)?;
        w.write_u32::<LE>(self._unknown2)?;
        w.write_u32::<LE>(self.fragments_residual_offset as u32)?;
        w.write_u32::<LE>(self.fragments_residual_count as u32)?;
        w.write_u32::<LE>(self._unknown5)?;
        w.write_u32::<LE>(self.fragmented_filesystem_offset as u32)?;
        w.write_u32::<LE>(self.fragmented_filesystem_length as u32)?;

        Ok(())
    }

    pub fn filesystem_entries(&self) -> usize {
        // The size, in bytes, of how `Fragment` appears in files.
        const FRAGMENT_SIZE: u32 = 8;
        (self.fragmented_filesystem_length as u32 / (FRAGMENT_SIZE * self.fragments_per_file))
            as usize
    }
}

#[derive(Clone, Debug)]
pub struct Fragment {
    pub offset: u64,
    pub length: u64,
}

impl Fragment {
    fn read_from<T: Read>(mut r: T) -> HpkResult<Fragment> {
        let offset = u64::from(r.read_u32::<LE>()?);
        let length = u64::from(r.read_u32::<LE>()?);
        Ok(Fragment { offset, length })
    }

    fn read_nth_from<T: Read>(n: usize, mut r: T) -> HpkResult<Vec<Fragment>> {
        let mut fragments = Vec::with_capacity(n);
        for _ in 0..n {
            fragments.push(Fragment::read_from(&mut r)?);
        }
        Ok(fragments)
    }

    fn new(offset: u64, length: u64) -> Fragment {
        Fragment { offset, length }
    }

    fn write(&self, w: &mut dyn Write) -> HpkResult<()> {
        w.write_u32::<LE>(self.offset as u32)?;
        w.write_u32::<LE>(self.length as u32)?;

        Ok(())
    }
}

#[derive(Copy, Clone)]
enum EntryType {
    Dir,
    File,
}

impl EntryType {
    fn to_value(self) -> u32 {
        match self {
            Self::Dir => 1,
            Self::File => 0,
        }
    }

    fn from_value(value: u32) -> Self {
        if value & 1 == 1 {
            Self::Dir
        } else {
            Self::File
        }
    }
}

pub struct DirEntry {
    path: PathBuf,
    kind: EntryType,
    index: usize,
    depth: usize,
}

impl DirEntry {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file_name(&self) -> &OsStr {
        self.path.file_name().unwrap_or(self.path.as_os_str())
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn depth(&self) -> usize {
        self.depth
    }

    pub fn is_dir(&self) -> bool {
        std::matches!(self.kind, EntryType::Dir)
    }

    fn new_root() -> Self {
        DirEntry {
            path: PathBuf::new(),
            kind: EntryType::Dir,
            index: 0,
            depth: 0,
        }
    }

    fn new_dir<P: AsRef<Path>>(path: P, index: usize, depth: usize) -> Self {
        DirEntry {
            path: path.as_ref().to_path_buf(),
            kind: EntryType::Dir,
            index,
            depth,
        }
    }

    fn new_file<P: AsRef<Path>>(path: P, index: usize, depth: usize) -> Self {
        DirEntry {
            path: path.as_ref().to_path_buf(),
            kind: EntryType::File,
            index,
            depth,
        }
    }

    fn read_from<T: Read>(parent: &Path, depth: usize, mut r: T) -> HpkResult<DirEntry> {
        let fragment_index = r
            .read_u32::<LE>()?
            .checked_sub(1)
            .ok_or(HpkError::InvalidFragmentIndex)?;

        let kind = r.read_u32::<LE>().map(EntryType::from_value)?;

        let name_length = r.read_u16::<LE>()?;
        let mut buf = vec![0; name_length as usize];
        r.read_exact(&mut buf)?;
        let name = str::from_utf8(&buf).map_err(|_| HpkError::InvalidDirEntryName)?;

        Ok(DirEntry {
            path: parent.join(name),
            kind,
            index: fragment_index as usize,
            depth,
        })
    }

    fn write(&self, w: &mut dyn Write) -> HpkResult<()> {
        w.write_u32::<LE>(self.index as u32)?;
        w.write_u32::<LE>(self.kind.to_value())?;
        let name = self
            .path
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or(HpkError::InvalidDirEntryName)?;
        w.write_u16::<LE>(name.len() as u16)?;
        w.write_all(name.as_bytes())?;
        Ok(())
    }
}

pub fn get_compression<T: Read + Seek>(r: &mut T) -> HpkResult<Compression> {
    let pos = r.stream_position()?;
    let compression = match Compression::read_from(r) {
        Ok(c) => c,
        Err(_) => Compression::None,
    };
    r.seek(SeekFrom::Start(pos))?;

    Ok(compression)
}

/// Compresses the data with the encoder used
///
/// if no data is written at all the hpk compression header is written without any chunks
/// it's the same behaviour as in a DLC file for Tropico 4
///
pub fn compress(options: &CompressOptions, r: &mut dyn Read, w: &mut dyn Write) -> HpkResult<u64> {
    use crate::compress::Encoder;

    let mut inflated_length = 0;
    let mut output_buffer = vec![];
    let mut offsets = vec![];

    loop {
        let mut chunk = vec![];
        let mut t = r.take(u64::from(options.chunk_size));

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

        match options.compressor {
            Compression::Zlib => compress::Zlib::encode_chunk(&chunk, &mut output_buffer)?,
            Compression::Lz4 => compress::Lz4Block::encode_chunk(&chunk, &mut output_buffer)?,
            _ => unreachable!(),
        };
    }

    let header_size = CompressionHeader::write(options, inflated_length, &offsets, w)?;

    Ok(header_size + io::copy(&mut Cursor::new(output_buffer), w)?)
}

fn decompress<T: compress::Decoder>(
    length: u64,
    r: &mut dyn Read,
    w: &mut dyn Write,
) -> HpkResult<u64> {
    let hdr = CompressionHeader::read_from(length, r)?;
    let mut written = 0;
    for chunk in &hdr.chunks {
        let mut buf = vec![0; chunk.length as usize];
        r.read_exact(&mut buf)?;
        written += match T::decode_chunk(&buf, w) {
            Ok(n) => n,
            Err(_) => {
                // chunk seems to be not compressed
                io::copy(&mut Cursor::new(buf), w)?
            }
        };
    }
    Ok(written)
}

pub struct CompressOptions {
    chunk_size: u32,
    compressor: Compression,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            chunk_size: 32768,
            compressor: Compression::Zlib,
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum Compression {
    Zlib,
    Lz4,
    Zstd,
    None,
}

impl std::fmt::Display for Compression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match *self {
            Compression::Zlib => write!(f, "ZLIB"),
            Compression::Lz4 => write!(f, "LZ4"),
            Compression::Zstd => write!(f, "ZSTD"),
            Compression::None => write!(f, "None"),
        }
    }
}

impl Compression {
    pub fn is_compressed(&self) -> bool {
        !std::matches!(*self, Compression::None)
    }

    fn read_from<T: Read + ?Sized>(r: &mut T) -> HpkResult<Self> {
        let mut buf = [0; 4];
        match r.read_exact(&mut buf) {
            Ok(_) => match (buf.eq(b"ZLIB"), buf.eq(b"LZ4 "), buf.eq(b"ZSTD")) {
                (true, _, _) => Ok(Compression::Zlib),
                (_, true, _) => Ok(Compression::Lz4),
                (_, _, true) => Ok(Compression::Zstd),
                (_, _, _) => Ok(Compression::None),
            },
            Err(e) => Err(HpkError::Io(e)),
        }
    }

    fn write_identifier(&self, w: &mut dyn Write) -> HpkResult<u64> {
        match *self {
            Compression::Zlib => Ok(w.write(b"ZLIB")? as u64),
            Compression::Lz4 => Ok(w.write(b"LZ4 ")? as u64),
            Compression::Zstd => Ok(w.write(b"ZSTD")? as u64),
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

        let inflated_length = r.read_u32::<LE>()?;
        let chunk_size = r.read_u32::<LE>()?;
        let chunks = match r.read_u32::<LE>() {
            Ok(val) => {
                let mut offsets = vec![u64::from(val)];
                if offsets[0] != 16 {
                    for _ in 0..((offsets[0] - 16) / 4) {
                        offsets.push(u64::from(r.read_u32::<LE>()?));
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
            chunks,
        })
    }

    fn write(
        options: &CompressOptions,
        inflated_length: u32,
        offsets: &[u32],
        out: &mut dyn Write,
    ) -> HpkResult<u64> {
        const HDR_SIZE: u32 = 12;

        options.compressor.write_identifier(out)?;
        out.write_u32::<LE>(inflated_length)?;
        out.write_u32::<LE>(options.chunk_size)?;

        let offsets_size = offsets.len() as u32 * 4;
        let offsets = offsets.iter().map(|x| HDR_SIZE + offsets_size + x);
        for offset in offsets {
            out.write_u32::<LE>(offset)?;
        }

        Ok(u64::from(HDR_SIZE + offsets_size))
    }
}

// struct ExtractOptions {{{
#[derive(Default)]
pub struct ExtractOptions {
    paths: Vec<Pattern>,
    skip_filedates: bool,
    fix_lua_files: bool,
    verbose: bool,
}

impl ExtractOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn skip_filedates(&mut self) {
        self.skip_filedates = true;
    }

    pub fn fix_lua_files(&mut self) {
        self.fix_lua_files = true;
    }

    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    pub fn set_paths(&mut self, paths: &[Pattern]) {
        self.paths = paths.to_vec();
    }

    fn matches(&self, path: &Path) -> bool {
        if self.paths.is_empty() {
            return true;
        }
        for pat in &self.paths {
            if pat.matches_path(path) {
                return true;
            }
        }
        false
    }
}
// }}}

pub fn extract<P>(options: &ExtractOptions, file: P, dest: P) -> HpkResult<()>
where
    P: AsRef<Path>,
{
    let file = file.as_ref();
    let dest = dest.as_ref();
    let mut walk = walk(file)?;
    let _filedates = Path::new("_filedates");

    while let Some(entry) = walk.next() {
        if let Ok(entry) = entry {
            let path = dest.join(entry.path());
            if !options.matches(&entry.path) {
                continue;
            }
            if entry.is_dir() {
                if !path.exists() {
                    ::std::fs::create_dir_all(&path)?;
                }
            } else {
                if let Some(parent) = path.parent() {
                    if !parent.exists() {
                        ::std::fs::create_dir_all(parent)?;
                    }
                }
                walk.read_file(&entry, |mut r| {
                    if options.verbose {
                        println!("{}", path.display());
                    }
                    if !options.skip_filedates && entry.depth() == 1 && entry.path().eq(_filedates)
                    {
                        process_filedates(dest, &mut r)
                    } else {
                        let ext = path
                            .extension()
                            .and_then(OsStr::to_str)
                            .map_or(String::new(), str::to_ascii_lowercase);

                        if options.fix_lua_files && &ext[..] == "lua" {
                            let out = File::create(path)?;
                            copy(&mut r, &mut lua::fix_header(out))?;
                        } else {
                            let mut out = File::create(path)?;
                            copy(&mut r, &mut out)?;
                        }
                        Ok(())
                    }
                })?;
            }
        }
    }
    Ok(())
}

fn process_filedates<P: AsRef<Path>>(dest: P, r: &mut FragmentedReader<&File>) -> HpkResult<()> {
    // macro: is_valid {{{
    macro_rules! is_valid {
        ($e:expr) => {{
            #[cfg(unix)]
            let valid = $e.exists();
            #[cfg(windows)]
            let valid = $e.exists() && $e.metadata()?.is_file();
            valid
        }};
    }
    // }}}

    let mut buf = Cursor::new(Vec::new());
    copy(r, &mut buf)?;
    buf.set_position(0);

    let br = io::BufReader::new(buf);
    for line in br.lines() {
        let line = line?;
        let entry: Vec<_> = line.rsplitn(2, '=').collect();
        if let Ok(val) = entry[0].parse::<i64>() {
            // This catches the different file time formats.
            // Multiplication overflows for the Windows file time
            let val = match val.checked_mul(2000) {
                Some(val) => val,
                None => val,
            };
            let unix_secs = (val / WINDOWS_TICKS) - SEC_TO_UNIX_EPOCH;
            let ft = filetime::FileTime::from_unix_time(unix_secs, 0);

            let path = dest.as_ref().join(entry[1]);
            if is_valid!(path) {
                filetime::set_file_times(path, ft, ft)?;
            } else {
                // Remove the first component of the path and try again because
                // Grand Ages: Rome adds the basename of the original hpk file to the path
                let mut comps = Path::new(entry[1]).components();
                comps.next();

                let path = dest.as_ref().join(comps.as_path());
                if is_valid!(path) {
                    filetime::set_file_times(path, ft, ft)?;
                }
            }
        }
    }
    Ok(())
}

pub fn copy<W>(r: &mut FragmentedReader<&File>, w: &mut W) -> HpkResult<u64>
where
    W: Write,
{
    match get_compression(r)? {
        Compression::Lz4 => decompress::<compress::Lz4Block>(r.len(), r, w),
        Compression::Zlib => decompress::<compress::Zlib>(r.len(), r, w),
        Compression::Zstd => decompress::<compress::Zstd>(r.len(), r, w),
        Compression::None => io::copy(r, w).map_err(HpkError::Io),
    }
}

// struct CreateOptions {{{
enum FileDateFormat {
    Default,
    Short,
}

pub struct CreateOptions {
    compress: bool,
    compress_options: CompressOptions,
    cripple_lua_files: bool,
    extensions: Vec<String>,
    filedates_fmt: Option<FileDateFormat>,
}

impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            compress: false,
            compress_options: CompressOptions::default(),
            cripple_lua_files: false,
            extensions: vec![
                "lst".into(),
                "lua".into(),
                "xml".into(),
                "tga".into(),
                "dds".into(),
                "xtex".into(),
                "bin".into(),
                "csv".into(),
            ],
            filedates_fmt: None,
        }
    }
}

impl CreateOptions {
    pub fn new() -> Self {
        CreateOptions::default()
    }

    pub fn compress(&mut self) {
        self.compress = true;
    }

    pub fn use_lz4(&mut self) {
        self.compress_options.compressor = Compression::Lz4;
    }

    pub fn cripple_lua_files(&mut self) {
        self.cripple_lua_files = true;
    }

    pub fn with_chunk_size(&mut self, chunk_size: u32) {
        self.compress_options.chunk_size = chunk_size;
    }

    pub fn with_extensions(&mut self, ext: Vec<String>) {
        self.extensions = ext;
    }

    pub fn with_default_filedates_format(&mut self) {
        self.filedates_fmt = Some(FileDateFormat::Default);
    }

    pub fn with_short_filedates_format(&mut self) {
        self.filedates_fmt = Some(FileDateFormat::Short);
    }

    fn with_filedates(&self) -> bool {
        self.filedates_fmt.is_some()
    }

    /// Calculates the file time for the _filedates file
    ///
    /// The actually values for Tropico 3 and Grand Ages: Rome are stored
    /// as Windows file times (default format) and for Tropico 4 and Omerta
    /// the values are the Windows file times divided by 2000 (short format).
    ///
    /// Tropico 5 and Victor Vran don't seem to use it anymore.
    ///
    fn filedates_value_for_path<P: AsRef<Path>>(&self, path: P) -> HpkResult<i64> {
        let ft = filetime::FileTime::from_last_modification_time(&path.as_ref().metadata()?);
        let filetime = ft.seconds();

        // Convert the platform dependent file time to Windows file time
        #[cfg(unix)]
        let filetime = (filetime + SEC_TO_UNIX_EPOCH) * WINDOWS_TICKS;

        match self.filedates_fmt {
            Some(FileDateFormat::Short) => Ok(filetime / 2000),
            _ => Ok(filetime),
        }
    }
}
// }}}

pub fn create<P>(options: &CreateOptions, dir: P, file: P) -> HpkResult<()>
where
    P: AsRef<Path>,
{
    use std::collections::HashMap;
    use walkdir::WalkDir;

    // macro: strip_prefix {{{
    macro_rules! strip_prefix {
        (dir $path: expr) => {{
            let path = $path.strip_prefix(&dir).unwrap();
            let parent = path.parent();
            (path, parent)
        }};
        (file $path: expr) => {{
            let (path, parent) = strip_prefix!(dir $path);
            (path, parent.unwrap())
        }};
    }
    // }}}

    let (mut w, tmpfile, _tmpdir) = {
        if options.compress {
            let tempdir = tempfile::Builder::new().prefix("hpk").tempdir()?;
            let tmpfile = tempdir.path().join(
                file.as_ref()
                    .file_name()
                    .and_then(OsStr::to_str)
                    .unwrap_or("temp.hpk"),
            );
            (File::create(&tmpfile)?, Some(tmpfile), Some(tempdir))
        } else {
            (File::create(&file)?, None, None)
        }
    };

    w.seek(SeekFrom::Start(u64::from(HEADER_LENGTH)))?;

    let mut filedates = vec![];
    let mut fragments: Vec<Fragment> = vec![];
    let mut stack = HashMap::new();

    let walkdir = WalkDir::new(&dir).contents_first(true).sort_by_file_name();

    for entry in walkdir {
        let entry = entry?;

        // write filedate entry
        if options.with_filedates() && entry.depth() > 0 {
            let val = options.filedates_value_for_path(entry.path())?;
            let (path, _) = strip_prefix!(dir entry.path());
            writeln!(filedates, "{}={}", path.display(), val)?;
        }

        if entry.file_type().is_file() {
            let (path, parent) = strip_prefix!(file entry.path());

            fragments.push(write_file(options, entry.path(), &mut w)?);
            let index = fragments.len() + 1;
            let parent_buf = stack.entry(parent.to_path_buf()).or_default();
            let dent = DirEntry::new_file(path, index, entry.depth());
            dent.write(parent_buf)?;
        } else if entry.file_type().is_dir() {
            let (path, parent) = strip_prefix!(dir entry.path());
            let mut dir_buffer = stack.remove(&path.to_path_buf()).unwrap_or_default();

            // write _filedates in the root dir buffer
            if options.with_filedates() && entry.depth() == 0 {
                let mut buf = Cursor::new(&filedates);
                let position = w.stream_position()?;
                let n = io::copy(&mut buf, &mut w)?;

                fragments.push(Fragment::new(position, n));
                let index = fragments.len() + 1;
                let dent = DirEntry::new_file("_filedates", index, 1);
                dent.write(&mut dir_buffer)?;
            }

            let position = w.stream_position()?;
            let mut r = Cursor::new(dir_buffer);
            let n = io::copy(&mut r, &mut w)?;

            let fragment = Fragment::new(position, n);
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

    let fragmented_filesystem_offset = w.stream_position()?;
    let fragmented_filesystem_length = fragments.len() as u64 * 8;
    for fragment in fragments {
        fragment.write(&mut w)?;
    }

    w.rewind()?;
    let header = Header::new(fragmented_filesystem_offset, fragmented_filesystem_length);
    header.write(&mut w)?;

    // Compress the temp file
    if let Some(tmpfile) = tmpfile {
        w.sync_data()?;
        let mut input = File::open(tmpfile)?;
        let mut out = File::create(file)?;
        compress(&options.compress_options, &mut input, &mut out)?;
    }

    return Ok(());

    // write_file {{{
    fn write_file<W>(options: &CreateOptions, file: &Path, w: &mut W) -> HpkResult<Fragment>
    where
        W: Write + Seek,
    {
        let ext = file
            .extension()
            .and_then(OsStr::to_str)
            .map_or(String::new(), str::to_ascii_lowercase);
        let _compress = options.extensions.contains(&ext);

        let mut fin = File::open(file)?;
        let position = w.stream_position()?;
        let n = if options.cripple_lua_files && &ext[..] == "lua" {
            let mut r = lua::cripple_header(&mut fin);
            if _compress {
                compress(&options.compress_options, &mut r, w)?
            } else {
                io::copy(&mut r, w)?
            }
        } else if _compress {
            compress(&options.compress_options, &mut fin, w)?
        } else {
            io::copy(&mut fin, w)?
        };

        Ok(Fragment::new(position, n))
    }
    // }}}
}

// vim: fdm=marker
