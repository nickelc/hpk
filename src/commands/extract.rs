use std::path::{Path, PathBuf};
use std::process;

use clap::{arg, ArgMatches, Command};

use crate::CliResult;

pub fn clap() -> Command {
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

    Command::new("extract")
        .about("Extract files from a hpk archive")
        .display_order(10)
        .arg(arg!(<file> "hpk archive").value_parser(input_parser))
        .arg(arg!(<dest> "destination folder").value_parser(dest_parser))
        .arg(arg!([paths]... "An optional list of archive members to be processed, separated by spaces."))
        .arg(arg!(filedates: --"ignore-filedates" "Skip processing of a _filedates file and just extract it"))
        .arg(arg!(fix_lua: --"fix-lua-files" "Fix the bytecode header of Victor Vran's or Surviving Mars' Lua files"))
        .arg(arg!(--force "Force extraction if destination folder is not empty"))
        .arg(arg!(verbose: -v "Verbosely list files processed"))
}

pub fn execute(matches: &ArgMatches) -> CliResult {
    let input = matches.get_one::<PathBuf>("file").expect("required arg");
    let dest = matches.get_one::<PathBuf>("dest").expect("required arg");
    let force = matches.get_flag("force");
    let verbose = matches.get_flag("verbose");

    if let Ok(dir) = dest.read_dir() {
        if !force && dir.count() > 0 {
            eprintln!("error: Directory is not empty");
            process::exit(1);
        }
    }

    let paths = matches
        .get_many("paths")
        .map(|v| v.cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let mut options = hpk::ExtractOptions::new();
    options.set_paths(&paths);
    options.set_verbose(verbose);
    if matches.get_flag("filedates") {
        options.skip_filedates();
    }
    if matches.get_flag("fix_lua") {
        options.fix_lua_files();
    }

    hpk::extract(&options, input, dest)?;
    Ok(())
}
