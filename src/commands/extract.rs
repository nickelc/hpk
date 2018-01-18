use std::fs;
use std::fs::File;
use std::path::PathBuf;
use std::process;

use clap::{App, Arg, ArgMatches, SubCommand};

use hpk;

pub fn clap<'a, 'b>() -> App<'a, 'b> {
    fn validate_input(value: String) -> Result<(), String> {
        if let Ok(md) = fs::metadata(value) {
            if md.is_file() {
                return Ok(());
            }
        }
        Err(String::from("Not a valid file"))
    }
    fn validate_dest(value: String) -> Result<(), String> {
        match fs::read_dir(value) {
            Ok(_) => Ok(()),
            Err(_) => Err(String::from("Not a valid directory")),
        }
    }

    SubCommand::with_name("extract")
        .about("Extract files from a hpk archive")
        .display_order(10)
        .arg(Arg::from_usage("<file> 'hpk archive'")
                .validator(validate_input))
        .arg(Arg::from_usage("<dest> 'destination folder'")
                .validator(validate_dest))
        .arg(Arg::from_usage("[force] --force 'Force extraction if destination folder is not empty'"))
        .arg(Arg::from_usage("[verbose] -v 'Verbosely list files processed'"))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "file", String).unwrap();
    let dest = value_t!(matches, "dest", String).map(|d| PathBuf::from(d)).unwrap();
    let force = matches.is_present("force");
    let verbose = matches.is_present("verbose");

    if let Ok(dir) = dest.read_dir() {
        if !force && dir.count() > 0 {
            eprintln!("error: Directory is not empty");
            process::exit(1);
        }
    }

    let mut walk = hpk::walk(input).unwrap();

    while let Some(entry) = walk.next() {
        if let Ok(entry) = entry {
            if entry.is_dir() {
                let path = dest.join(entry.path());
                if !path.exists() {
                    fs::create_dir(path).unwrap();
                }
            } else {
                walk.read_file(&entry, |mut r| {
                    let out = dest.join(entry.path());
                    if verbose {
                        println!("{}", out.display());
                    }
                    let mut out = File::create(out).unwrap();
                    hpk::copy(&mut r, &mut out).unwrap();
                });
            }
        }
    }
}
