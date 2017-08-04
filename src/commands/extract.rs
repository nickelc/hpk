use std::io;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use libflate::zlib;

use clap::{App, Arg, ArgMatches, SubCommand};

use hpk;

pub fn clap<'a, 'b>() -> App<'a, 'b> {
    fn validate_input(value: String) -> Result<(), String> {
        let path = Path::new(&value);
        if let Ok(md) = path.metadata() {
            if md.is_file() {
                return Ok(())
            }
        }
        Err(String::from("Not a valid file"))
    }
    fn validate_dest(value: String) -> Result<(), String> {
        match fs::read_dir(value) {
            Ok(dir) => {
                if dir.count() > 0 {
                    Err(String::from("Directory is not empty"))
                } else {
                    Ok(())
                }
            }
            Err(_) => Err(String::from("Not a valid directory")),
        }
    }

    SubCommand::with_name("extract")
        .about("extract files from a hpk archive")
        .display_order(10)
        .arg(Arg::from_usage("<file> 'hpk archive'")
                .validator(validate_input))
        .arg(Arg::from_usage("<dest> 'destination folder'")
                .validator(validate_dest))
        .arg(Arg::from_usage("[verbose] -v 'verbosely list files processed'"))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "file", String).unwrap();
    let dest = value_t!(matches, "dest", String).map(|d| PathBuf::from(d)).unwrap();

    let mut f = File::open(input).unwrap();
    let mut visitor = ExtractVisitor {
        base_path: dest,
        verbose: matches.is_present("verbose"),
    };

    hpk::read_hpk(&mut f, &mut visitor);
}


struct ExtractVisitor {
    base_path: PathBuf,
    verbose: bool,
}

impl hpk::Visitor for ExtractVisitor {

    fn visit_directory(&mut self, dir: &Path, fragment: &hpk::Fragment) {
        let path = self.base_path.join(dir);
        if !path.exists() {
            fs::create_dir(path).unwrap();
        }
    }

    fn visit_file(&mut self, file: &Path, fragment: &hpk::Fragment, r: &mut File) {
        println!("{}", file.display());
        if self.verbose {
            println!("fragment: {:X} len: {}", fragment.offset, fragment.length);
        }
        r.seek(SeekFrom::Start(fragment.offset)).unwrap();

        if let Ok(hdr) = hpk::CompressionHeader::from_read(fragment, r) {
            if self.verbose {
                println!("compressed: inflated_length={} chunk_size={} chunks={}", hdr.inflated_length, hdr.chunk_size, hdr.chunks.len());
            }
            let mut w = File::create(self.base_path.join(file)).unwrap();
            for chunk in &hdr.chunks {
                if self.verbose {
                    println!("write chunk: {:X} len: {}", chunk.offset, chunk.length);
                }
                r.seek(SeekFrom::Start(chunk.offset)).unwrap();
                {
                    let take = r.take(chunk.length);
                    if let Ok(mut dec) = zlib::Decoder::new(take) {
                        if self.verbose {
                            println!("{:?}", dec.header());
                        }
                        io::copy(&mut dec, &mut w).unwrap();
                        continue;
                    }
                }
                // chunk is not compressed
                r.seek(SeekFrom::Start(chunk.offset)).unwrap();
                io::copy(&mut r.take(chunk.length), &mut w).unwrap();
            }
        } else {
            if self.verbose {
                println!("compressed: no");
            }
            let mut w = File::create(self.base_path.join(file)).unwrap();
            r.seek(SeekFrom::Start(fragment.offset)).unwrap();
            io::copy(&mut r.take(fragment.length), &mut w).unwrap();
        }
    }
}
