use std::cmp;
use std::fs::File;
use std::io::prelude::*;
use std::io;
use std::io::Cursor;
use std::io::SeekFrom;
use std::str;
use std::path::{Path, PathBuf};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use flate2::Compression;
use flate2::write::ZlibEncoder;

static HEADER_IDENTIFIER: [u8; 4] = ['B' as u8, 'P' as u8, 'U' as u8, 'L' as u8];
pub static HEADER_LENGTH: u8 = 36;

pub struct Header {
    _identifier: [u8; 4],
    pub data_offset: u32,
    pub fragments_per_file: u32,
    _unknown2: u32,
    pub fragments_residual_offset: u64,
    pub fragments_residual_count: u64,
    _unknown5: u32,
    pub fragmented_filesystem_offset: u64,
    pub fragmented_filesystem_count: u64,
}

impl Header {

    fn new(fragment_filesystem_offset: u64, fragment_filesystem_count: u64) -> Header {
        Header {
            _identifier: HEADER_IDENTIFIER,
            data_offset: 36,
            fragments_per_file: 1,
            _unknown2: 0xFF,
            fragments_residual_offset: 0,
            fragments_residual_count: 0,
            _unknown5: 1,
            fragmented_filesystem_offset: fragment_filesystem_offset,
            fragmented_filesystem_count: fragment_filesystem_count,
        }
    }

    pub fn from_read(r: &mut Read) -> Result<Header, ()> {
        let _identifier = read_identifier(r);
        if _identifier.eq(&HEADER_IDENTIFIER) {
            Ok(Header {
                _identifier,
                data_offset: r.read_u32::<LittleEndian>().unwrap(),
                fragments_per_file: r.read_u32::<LittleEndian>().unwrap(),
                _unknown2: r.read_u32::<LittleEndian>().unwrap(),
                fragments_residual_offset: r.read_u32::<LittleEndian>().unwrap() as u64,
                fragments_residual_count: r.read_u32::<LittleEndian>().unwrap() as u64,
                _unknown5: r.read_u32::<LittleEndian>().unwrap(),
                fragmented_filesystem_offset: r.read_u32::<LittleEndian>().unwrap() as u64,
                fragmented_filesystem_count: r.read_u32::<LittleEndian>().unwrap() as u64,
            })
        } else {
            Err(())
        }
    }

    fn write(&self, w: &mut Write) -> io::Result<()> {
        w.write(&self._identifier)?;
        w.write_u32::<LittleEndian>(self.data_offset)?;
        w.write_u32::<LittleEndian>(self.fragments_per_file)?;
        w.write_u32::<LittleEndian>(self._unknown2).unwrap();
        w.write_u32::<LittleEndian>(self.fragments_residual_offset as u32)?;
        w.write_u32::<LittleEndian>(self.fragments_residual_count as u32)?;
        w.write_u32::<LittleEndian>(self._unknown5)?;
        w.write_u32::<LittleEndian>(self.fragmented_filesystem_offset as u32)?;
        w.write_u32::<LittleEndian>(self.fragmented_filesystem_count as u32)?;

        Ok(())
    }

    pub fn filesystem_entries(&self) -> usize {
        const FRAGMENT_SIZE: u32 = 8;
        (self.fragmented_filesystem_count as u32 / (FRAGMENT_SIZE * self.fragments_per_file)) as usize
    }
}

#[derive(Debug)]
pub struct Fragment {
    pub offset: u64,
    pub length: u64,
}

impl Fragment {

    pub fn from_read(r: &mut Read) -> io::Result<Fragment> {
        let offset = u64::from(r.read_u32::<LittleEndian>()?);
        let length = u64::from(r.read_u32::<LittleEndian>()?);
        Ok(Fragment { offset, length })
    }

    pub fn new(offset: u64, length: u64) -> Fragment {
        Fragment { offset, length }
    }

    fn write(&self, w: &mut Write) -> io::Result<()> {
        w.write_u32::<LittleEndian>(self.offset as u32)?;
        w.write_u32::<LittleEndian>(self.length as u32)?;

        Ok(())
    }
}

struct FragmentState {
    offset: u64,
    length: u64,
    limit: u64,
}

pub struct FragmentedFile<T> {
    inner: T,
    length: u64,
    current: usize,
    fragments: Vec<FragmentState>,
}

#[allow(dead_code)]
impl<T> FragmentedFile<T> {

    pub fn new(inner: T, fragments: Vec<Fragment>) -> Self {
        let states: Vec<_> = fragments
            .iter()
            .map(|f| {
                FragmentState {
                    offset: f.offset,
                    length: f.length,
                    limit: f.length,
                }
            })
            .collect();

        let length = fragments.iter().map(|f| f.length).sum();

        Self {
            inner,
            length,
            current: 0,
            fragments: states,
        }
    }

    pub fn len(&self) -> u64 {
        self.length
    }
}

impl<T: Read + Seek> Read for FragmentedFile<T> {

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(f) = self.fragments.get_mut(self.current) {
            // Nothing has been read yet?
            if f.length == f.limit {
                self.inner.seek(SeekFrom::Start(f.offset))?;
            }

            let max = cmp::min(buf.len() as u64, f.limit) as usize;
            let n = self.inner.read(&mut buf[..max])?;
            f.limit -= n as u64;

            // if fragment is consumed then goto next fragment
            if f.limit == 0 {
                self.current += 1;
            }
            return Ok(n);
        }
        Ok(0)
    }
}

#[derive(Debug)]
pub struct FileEntry {
    pub fragment_index: i32,
    pub fragment_type: u32,
    pub name: String,
}

impl FileEntry {

    fn new_dir(fragment_index: i32, name: String) -> FileEntry {
        FileEntry {
            fragment_index,
            fragment_type: 1,
            name,
        }
    }

    fn new_file(fragment_index: i32, name: String) -> FileEntry {
        FileEntry {
            fragment_index,
            fragment_type: 0,
            name,
        }
    }

    pub fn from_read(r: &mut Read) -> io::Result<FileEntry> {
        let fragment_index = r.read_i32::<LittleEndian>()?;
        let fragment_type = r.read_u32::<LittleEndian>()?;
        let name_length = r.read_u16::<LittleEndian>()?;
        let mut buf = vec![0; name_length as usize];
        r.read_exact(&mut buf).unwrap();
        let name = str::from_utf8(&buf).unwrap().to_owned();

        Ok(FileEntry { fragment_index, fragment_type, name })
    }

    fn write(&self, w: &mut Write) -> io::Result<()> {
        w.write_i32::<LittleEndian>(self.fragment_index)?;
        w.write_u32::<LittleEndian>(self.fragment_type)?;
        w.write_u16::<LittleEndian>(self.name.len() as u16)?;
        w.write(self.name.as_bytes())?;

        Ok(())
    }

    pub fn get_index(&self) -> usize {
        (self.fragment_index - 1) as usize
    }

    pub fn get_size(&self) -> u64 {
        10 + self.name.len() as u64
    }

    pub fn is_dir(&self) -> bool {
        self.fragment_type == 1
    }

    #[allow(dead_code)]
    pub fn is_file(&self) -> bool {
        self.fragment_type == 0
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
        r.read_exact(&mut buf).expect("failed to read compression identifier");
        r.seek(SeekFrom::Current(-4)).expect("failed seek to previous position");

        buf.eq("ZLIB".as_bytes())
    }

    pub fn from_read<T: Read>(fragment: &Fragment, r: &mut T) -> io::Result<CompressionHeader> {
        let mut _identifier = [0; 4];
        r.read_exact(&mut _identifier)?;

        let inflated_length = r.read_u32::<LittleEndian>()?;
        let chunk_size = r.read_i32::<LittleEndian>()?;
        let mut offsets = vec![r.read_u32::<LittleEndian>()? as u64];
        if offsets[0] != 16 {
            for _ in 0..((offsets[0] - 16) / 4) {
                offsets.push(r.read_u32::<LittleEndian>()? as u64);
            }
        }
        let mut chunks = vec![Chunk{offset: 0, length: 0}; offsets.len()];
        let mut len = fragment.length;
        for (i, offset) in offsets.iter().enumerate().rev() {
            chunks[i] = Chunk {
                offset: fragment.offset + offset,
                length: len - offset,
            };
            len -= chunks[i].length;
        }

        Ok(CompressionHeader {
            _identifier,
            inflated_length,
            chunk_size,
            chunks: chunks,
        })
    }

    fn write(inflated_length: u32, offsets: Vec<i32>, out: &mut Write) -> io::Result<()> {
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

fn read_identifier(r: &mut Read) -> [u8; 4]  {
    let mut buf = [0; 4];
    r.read_exact(&mut buf).unwrap();
    buf
}

#[allow(unused_variables)]
pub trait ReadVisitor {
    fn visit_header(&mut self, header: &Header) {}
    fn visit_fragments(&mut self, fragments: &Vec<Fragment>) {}
    fn visit_file_entry(&mut self, file_entry: &FileEntry) {}
    fn visit_directory(&mut self, dir: &Path, fragment: &Fragment) {}
    fn visit_file(&mut self, file: &Path, fragment: &Fragment, r: &mut File) {}
}

pub fn read_hpk(file: &mut File, visitor: &mut ReadVisitor) {
    if let Ok(hdr) = Header::from_read(file) {
        visitor.visit_header(&hdr);

        let mut fragments_data = Cursor::new(vec![0; hdr.fragmented_filesystem_count as usize]);

        file.seek(SeekFrom::Start(hdr.fragmented_filesystem_offset)).unwrap();
        file.read_exact(fragments_data.get_mut().as_mut_slice()).unwrap();

        let entries = hdr.filesystem_entries() * hdr.fragments_per_file as usize;
        let mut fragments = Vec::with_capacity(entries);
        for _ in 0..entries {
            let fragment = Fragment::from_read(&mut fragments_data).unwrap();
            fragments.push(fragment);
        }
        visitor.visit_fragments(&fragments);

        fn read_directory(v: &mut ReadVisitor, fragments: &Vec<Fragment>, fragment_index: usize, wd: PathBuf, r: &mut File) {
            let fragment = fragments.get(fragment_index).unwrap();
            let mut file_entries = Cursor::new(vec![0; fragment.length as usize]);

            v.visit_directory(wd.as_path(), fragment);

            r.seek(SeekFrom::Start(fragment.offset)).unwrap();
            r.read_exact(file_entries.get_mut().as_mut_slice()).unwrap();

            let mut pos = 0;
            while pos < fragment.length {
                let entry = FileEntry::from_read(&mut file_entries).unwrap();
                v.visit_file_entry(&entry);
                pos += entry.get_size();

                let path = wd.join(entry.name.clone());

                if entry.is_dir() {
                    read_directory(v, fragments, entry.get_index(), path, r);
                } else {
                    let file_fragment = fragments.get(entry.get_index()).unwrap();

                    v.visit_file(path.as_path(), file_fragment, r);
                }
            }
        }

        read_directory(visitor, &fragments, 0, PathBuf::from(""), file);
    }
}

pub fn write_hpk(path: PathBuf, out: &mut File) -> io::Result<()> {
    // skip header
    out.seek(SeekFrom::Start(HEADER_LENGTH as u64))?;

    let mut fragments = vec![];
    let fragment = walk_dir(path, &mut fragments, out)?;
    fragments.insert(0, fragment);

    let fragment_position = out.seek(SeekFrom::Current(0))?;

    for fragment in fragments {
        fragment.write(out)?;
    }

    let current_pos = out.seek(SeekFrom::Current(0))?;
    out.seek(SeekFrom::Start(0))?;

    let header = Header::new(fragment_position, current_pos - fragment_position);
    header.write(out)?;

    return Ok(());

    fn write_file(file: PathBuf, out: &mut File) -> io::Result<Fragment> {
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
                let position = output_buffer.len() as i32;
                offsets.push(position);

                let mut chunk = vec![];
                let mut t = file.take(CHUNK_SIZE);
                io::copy(&mut t, &mut chunk)?;
                file = t.into_inner();

                let mut encoder = ZlibEncoder::new(vec![], Compression::Best);
                let mut chunk = Cursor::new(chunk);
                io::copy(&mut chunk, &mut encoder)?;

                match encoder.finish() {
                    Ok(ref buf) if buf.len() as u64 == CHUNK_SIZE => {
                        io::copy(&mut chunk, &mut output_buffer)?;
                    },
                    Ok(buf) => {
                        let mut buf = Cursor::new(buf);
                        io::copy(&mut buf, &mut output_buffer)?;
                    },
                    Err(_) => {},
                };

                if file.seek(SeekFrom::Current(0))? == length {
                    break;
                }
            }

            let position = out.seek(SeekFrom::Current(0))?;

            CompressionHeader::write(length as u32, offsets, out)?;
            io::copy(&mut Cursor::new(output_buffer), out)?;

            let current_pos = out.seek(SeekFrom::Current(0))?;

            Ok(Fragment::new(position, current_pos - position))

        } else {
            let position = out.seek(SeekFrom::Current(0))?;
            let mut input = File::open(file)?;
            io::copy(&mut input, out)?;
            let current_pos = out.seek(SeekFrom::Current(0))?;

            Ok(Fragment::new(position, current_pos - position))
        }
    }

    fn walk_dir(dir: PathBuf, fragments: &mut Vec<Fragment>, out: &mut File) -> io::Result<Fragment> {
        let entries = dir.read_dir()?;
        let mut paths = entries.map(|e| e.unwrap().path()).collect::<Vec<_>>();
        paths.sort_by(|a, b| {
            let a = a.to_str().unwrap().to_owned().to_lowercase();
            let b = b.to_str().unwrap().to_owned().to_lowercase();
            a.cmp(&b)
        });

        let mut dir_buffer = vec![];

        for entry in paths {
            let entry_name = entry.file_name().unwrap()
                                    .to_str().unwrap().to_owned();
            if entry.is_dir() {
                let fragment = walk_dir(entry, fragments, out)?;
                fragments.push(fragment);
                let file_entry = FileEntry::new_dir(
                    fragments.len() as i32 + 1,
                    entry_name,
                );
                file_entry.write(&mut dir_buffer)?;

            } else {
                let fragment = write_file(entry, out)?;
                fragments.push(fragment);

                let file_entry = FileEntry::new_file(
                    fragments.len() as i32 + 1,
                    entry_name,
                );
                file_entry.write(&mut dir_buffer)?;
            }
        }

        let position = out.seek(SeekFrom::Current(0))?;
        let mut buffer = Cursor::new(dir_buffer);
        io::copy(&mut buffer, out)?;
        let current_pos = out.seek(SeekFrom::Current(0))?;

        Ok(Fragment::new(position, current_pos - position))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;
    use std::ptr;

    macro_rules! create_buffer {
        ($size:expr, $init:expr, $ranges: expr, $vals: expr) => (
            {
                let mut buf: Vec<u8> = vec![$init; $size];

                let mut zip = $ranges.iter().zip($vals.iter());
                while let Some((r, val)) = zip.next() {
                    let slice = &mut buf[r.clone()];
                    unsafe {
                        ptr::write_bytes(slice.as_mut_ptr(), *val, slice.len());
                    }
                }

                buf
            }
        )
    }

    macro_rules! create_fragments {
        ($($x:expr),*) => (
            vec![$($x),*].iter()
                .map(|x| Fragment::new(x.0, x.1))
                .collect::<Vec<_>>()
        );
        ($($x:expr,)*) => (create_fragments![$($x),*])
    }

    fn create_fragmented_file() -> FragmentedFile<Cursor<Vec<u8>>> {
        let fragments = create_fragments!((10, 12), (32, 20), (60, 35), (100, 22));

        let ranges = vec![10..22, 32..52, 60..95, 100..122];
        let vals: Vec<u8> = vec![0x11, 0x22, 0x33, 0x44];

        let r = Cursor::new(create_buffer!(128, 0xFF, ranges, vals));

        FragmentedFile::new(r, fragments)
    }

    #[test]
    fn test_fragmented_file_read() {
        let mut ff = create_fragmented_file();

        assert_eq!(ff.len(), 89);

        let mut buf = vec![0; ff.len() as usize];

        let n = ff.read(&mut buf).unwrap();
        assert_eq!(n, 12);
        let n = ff.read(&mut buf).unwrap();
        assert_eq!(n, 20);
        let n = ff.read(&mut buf).unwrap();
        assert_eq!(n, 35);
        let n = ff.read(&mut buf).unwrap();
        assert_eq!(n, 22);

        // EOF of fragmented file reached
        let n = ff.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_fragmented_file_read_exact() {
        let mut ff = create_fragmented_file();

        assert_eq!(ff.len(), 89);

        let mut buf = vec![0; ff.len() as usize];

        ff.read_exact(&mut buf).unwrap();

        // EOF of fragmented file reached
        let n = ff.read(&mut buf).unwrap();
        assert_eq!(n, 0);
    }
}
