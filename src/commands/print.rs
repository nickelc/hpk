use std::io::prelude::*;
use std::io::SeekFrom;
use std::fs::File;
use std::path::Path;

use clap::{App, Arg, ArgMatches, SubCommand};

use hpk;

pub fn clap<'a, 'b>() -> App<'a, 'b> {
    fn validate_input(value: String) -> Result<(), String> {
        let path = Path::new(&value);
        match path.metadata() {
            Ok(ref md) if md.is_file() => Ok(()),
            Ok(_) => Err(String::from("Not a valid file")),
            Err(_) => Err(String::from("Not a valid file")),
        }
    }

    SubCommand::with_name("print")
        .about("print information of a hpk archive")
        .display_order(30)
        .arg(Arg::from_usage("<file> 'hpk archive'")
                .validator(validate_input))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "file", String).unwrap();
    println!("reading file: {}", input);
    let mut f = File::open(input).unwrap();

    let mut visitor = PrintVisitor{};
    hpk::read_hpk(&mut f, &mut visitor);
}

struct PrintVisitor;

impl hpk::Visitor for PrintVisitor {

    fn visit_header(&mut self, header: &hpk::Header) {
        println!("header:");
        println!("  data_offset: 0x{:X}", header.data_offset);
        println!("  fragments_per_file: {}", header.fragments_per_file);
        println!("  fragments_residual_offset: 0x{:X}", header.fragments_residual_offset);
        println!("  fragments_residual_count: {}", header.fragments_residual_count);
        println!("  fragments_filesystem_offset: 0x{:X}", header.fragmented_filesystem_offset);
        println!("  fragments_filesystem_count: {}", header.fragmented_filesystem_count);
        println!("  entries: {}", header.filesystem_entries());
    }

    fn visit_fragments(&mut self, fragments: &Vec<hpk::Fragment>) {
        println!("fragments:");
        for fragment in fragments {
            println!("  0x{:<6X} len: {}", fragment.offset, fragment.length);
        }
    }

    fn visit_file_entry(&mut self, file_entry: &hpk::FileEntry) {
        println!("file entry: index={} type={} name={:?}",
            file_entry.fragment_index, file_entry.fragment_type, file_entry.name);
    }

    fn visit_directory(&mut self, dir: &Path, fragment: &hpk::Fragment) {
        println!("dir: {:?} fragment: 0x{:X} len: {}", dir.display(), fragment.offset, fragment.length);
    }

    fn visit_file(&mut self, file: &Path, fragment: &hpk::Fragment, r: &mut File) {
        println!("file: {:?} fragment: 0x{:X} len: {}", file.display(), fragment.offset, fragment.length);
        r.seek(SeekFrom::Start(fragment.offset)).unwrap();

        if let Ok(hdr) = hpk::CompressionHeader::from_read(fragment, r) {
            println!("compressed: inflated_length={} chunk_size={} chunks={}", hdr.inflated_length, hdr.chunk_size, hdr.chunks.len());
            for chunks in &hdr.chunks {
                println!("chunk: 0x{:<6X} len: {}", chunks.offset, chunks.length);
            }
        } else {
            println!("compressed: no");
        }
    }
}
