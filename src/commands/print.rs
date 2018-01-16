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
        .about("Print information of a hpk archive")
        .display_order(30)
        .arg(Arg::from_usage("<file> 'hpk archive'")
                .validator(validate_input))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "file", String).unwrap();
    let mut walk = hpk::walk(input).unwrap();

    println!("reading file: {}", walk.path().display());
    println!("header:");
    println!("  data_offset: 0x{:X}", walk.header().data_offset);
    println!("  fragments_residual_offset: 0x{:X}", walk.header().fragments_residual_offset);
    println!("  fragments_residual_count: {}", walk.header().fragments_residual_count);
    println!("  fragments_per_file: {}", walk.header().fragments_per_file);
    println!("  fragments_filesystem_offset: 0x{:X}", walk.header().fragmented_filesystem_offset);
    println!("  fragments_filesystem_length: {}", walk.header().fragmented_filesystem_length);
    println!("filesystem entries: {}", walk.header().filesystem_entries());
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
        println!("{} index={} depth={} {:?}",
            if dent.is_dir() { "dir: " } else { "file:" },
            dent.index() + 1,
            dent.depth(),
            dent.path().display(),
        );
        let fragment = &walk.fragments[dent.index()][0];
        println!(" fragment: 0x{:X} len: {}", fragment.offset, fragment.length);
        walk.read_file(&dent, |mut r| {
            if r.len() == 0 {
                println!(" empty file");
            } else if hpk::is_compressed(&mut r) {
                let hdr = hpk::CompressionHeader::read_from(r.len(), &mut r).unwrap();
                println!(" compressed: inflated_length={} chunk_size={} chunks={}",
                    hdr.inflated_length,
                    hdr.chunk_size,
                    hdr.chunks.len()
                );
                let mut first = Some(true);
                for chunk in &hdr.chunks {
                    if let Some(_) = first.take() {
                        println!("  chunks: 0x{:<6X} len: {}", chunk.offset, chunk.length);
                    } else {
                        println!("          0x{:<6X} len: {}", chunk.offset, chunk.length);
                    }
                }
            } else {
                println!(" compressed: no");
            }
        });
    }
}
