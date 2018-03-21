use std::fs;
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
        .arg(Arg::from_usage("[filedates] --ignore-filedates")
                .help("Skip processing of a _filedates file and just extract it"))
        .arg(Arg::from_usage("[fix_lua] --fix-lua-files")
                .help("Fix the bytecode header of Surviving Mars' Lua files"))
        .arg(Arg::from_usage("[force] --force 'Force extraction if destination folder is not empty'"))
        .arg(Arg::from_usage("[verbose] -v 'Verbosely list files processed'"))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "file", String).map(PathBuf::from).unwrap();
    let dest = value_t!(matches, "dest", String).map(PathBuf::from).unwrap();
    let force = matches.is_present("force");
    let verbose = matches.is_present("verbose");

    if let Ok(dir) = dest.read_dir() {
        if !force && dir.count() > 0 {
            eprintln!("error: Directory is not empty");
            process::exit(1);
        }
    }

    let mut options = hpk::ExtractOptions::new();
    options.set_verbose(verbose);
    if matches.is_present("filedates") {
        options.skip_filedates();
    }
    if matches.is_present("fix_lua") {
        options.fix_lua_files();
    }
    hpk::extract(options, input, dest).unwrap();
}
