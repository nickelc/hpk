use std::path::{Path, PathBuf};
use std::process;

use clap::{App, Arg, ArgMatches, SubCommand};

use crate::CliResult;

pub fn clap<'a>() -> App<'a> {
    fn input_parser(value: &str) -> Result<PathBuf, String> {
        let file = Path::new(value);
        if let Ok(md) = file.metadata() {
            if md.is_file() {
                return Ok(file.to_path_buf());
            }
        }
        Err(String::from("Not a valid file"))
    }
    fn dest_parser(value: &str) -> Result<PathBuf, String> {
        let dest = Path::new(value);
        match dest.metadata() {
            Ok(ref md) if md.is_file() => Err(String::from("Not a valid directory")),
            Ok(_) | Err(_) => Ok(dest.to_path_buf()),
        }
    }

    SubCommand::with_name("extract")
        .about("Extract files from a hpk archive")
        .display_order(10)
        .arg(Arg::from_usage("<file> 'hpk archive'").value_parser(input_parser))
        .arg(Arg::from_usage("<dest> 'destination folder'").value_parser(dest_parser))
        .arg(
            Arg::from_usage("[paths]...")
                .help("An optional list of archive members to be processed, separated by spaces."),
        )
        .arg(
            Arg::from_usage("[filedates] --ignore-filedates")
                .help("Skip processing of a _filedates file and just extract it"),
        )
        .arg(
            Arg::from_usage("[fix_lua] --fix-lua-files")
                .help("Fix the bytecode header of Victor Vran's or Surviving Mars' Lua files"),
        )
        .arg(Arg::from_usage(
            "[force] --force 'Force extraction if destination folder is not empty'",
        ))
        .arg(Arg::from_usage(
            "[verbose] -v 'Verbosely list files processed'",
        ))
}

pub fn execute(matches: &ArgMatches) -> CliResult {
    let input = matches.get_one::<PathBuf>("file").expect("required arg");
    let dest = matches.get_one::<PathBuf>("dest").expect("required arg");
    let force = matches.is_present("force");
    let verbose = matches.is_present("verbose");

    if let Ok(dir) = dest.read_dir() {
        if !force && dir.count() > 0 {
            eprintln!("error: Directory is not empty");
            process::exit(1);
        }
    }

    let mut options = hpk::ExtractOptions::new();
    options.set_paths(&values_t!(matches, "paths", String).unwrap_or_default());
    options.set_verbose(verbose);
    if matches.is_present("filedates") {
        options.skip_filedates();
    }
    if matches.is_present("fix_lua") {
        options.fix_lua_files();
    }
    hpk::extract(&options, input, dest)?;
    Ok(())
}
