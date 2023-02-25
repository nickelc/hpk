use std::path::{Path, PathBuf};

use clap::{arg, ArgMatches, Command};

use crate::CliResult;

pub fn cmd() -> Command {
    fn input_parser(value: &str) -> Result<PathBuf, String> {
        let path = Path::new(value);
        match path.metadata() {
            Ok(ref md) if md.is_file() => Ok(path.to_path_buf()),
            Ok(_) | Err(_) => Err(String::from("Not a valid file")),
        }
    }

    Command::new("debug-print")
        .about("Print debug information of a hpk archive")
        .alias("print")
        .display_order(30)
        .arg(arg!(<file> "hpk archive").value_parser(input_parser))
        .arg(arg!(header: --"header-only" "Print only the header information"))
}

pub fn execute(matches: &ArgMatches) -> CliResult {
    let input = matches.get_one::<PathBuf>("file").expect("required arg");
    let mut walk = hpk::walk(input)?;

    println!("reading file: {}", walk.path().display());
    if walk.is_compressed() {
        println!("file is compressed");
    }
    println!("header:");
    println!("  data_offset: 0x{:X}", walk.header().data_offset);
    println!(
        "  fragments_residual_offset: 0x{:X}",
        walk.header().fragments_residual_offset
    );
    println!(
        "  fragments_residual_count: {}",
        walk.header().fragments_residual_count
    );
    println!("  fragments_per_file: {}", walk.header().fragments_per_file);
    println!(
        "  fragments_filesystem_offset: 0x{:X}",
        walk.header().fragmented_filesystem_offset
    );
    println!(
        "  fragments_filesystem_length: {}",
        walk.header().fragmented_filesystem_length
    );
    println!("filesystem entries: {}", walk.header().filesystem_entries());

    if matches.get_flag("header") {
        return Ok(());
    }

    println!("filesystem fragments:");
    for chunk in &walk.fragments {
        let mut start = if walk.header().fragments_per_file == 1 {
            None
        } else {
            Some(true)
        };
        for fragment in chunk {
            print!("{}", if start.take().is_some() { "- " } else { "  " });
            println!("0x{:<6X} len: {}", fragment.offset, fragment.length);
        }
    }
    if !walk.residuals.is_empty() {
        println!("residual fragments:");
        for f in &walk.residuals {
            println!("  0x{:<6X} len: {}", f.offset, f.length);
        }
    }

    while let Some(Ok(dent)) = walk.next() {
        println!(
            "{} index={} depth={} {:?}",
            if dent.is_dir() { "dir: " } else { "file:" },
            dent.index() + 1,
            dent.depth(),
            dent.path().display(),
        );
        let fragment = &walk.fragments[dent.index()][0];
        println!(
            " fragment: 0x{:X} len: {}",
            fragment.offset, fragment.length
        );
        walk.read_file(&dent, |mut r| {
            if r.is_empty() {
                println!(" empty file");
            } else if hpk::get_compression(&mut r)?.is_compressed() {
                let hdr = hpk::CompressionHeader::read_from(r.len(), &mut r)?;
                println!(
                    " compressed: {} inflated_length={} chunk_size={} chunks={}",
                    hdr.compressor,
                    hdr.inflated_length,
                    hdr.chunk_size,
                    hdr.chunks.len()
                );
                let mut first = Some(true);
                for chunk in &hdr.chunks {
                    if first.take().is_some() {
                        println!("  chunks: 0x{:<6X} len: {}", chunk.offset, chunk.length);
                    } else {
                        println!("          0x{:<6X} len: {}", chunk.offset, chunk.length);
                    }
                }
            } else {
                println!(" compressed: no");
            }
            Ok(())
        })?;
    }
    Ok(())
}
