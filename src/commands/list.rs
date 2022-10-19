use std::path::{Path, PathBuf};

use clap::{App, Arg, ArgMatches, SubCommand};
use glob::Pattern;

use crate::CliResult;

pub fn clap<'a>() -> App<'a> {
    fn input_parser(value: &str) -> Result<PathBuf, String> {
        let file = Path::new(value);
        match file.metadata() {
            Ok(ref md) if md.is_file() => Ok(file.to_path_buf()),
            Ok(_) => Err(String::from("Not a valid file")),
            Err(_) => Err(String::from("Not a valid file")),
        }
    }

    SubCommand::with_name("list")
        .about("List the content of a hpk archive")
        .display_order(20)
        .arg(Arg::from_usage("<file> 'hpk archive'").value_parser(input_parser))
        .arg(Arg::from_usage("[paths]..."))
}

pub fn execute(matches: &ArgMatches) -> CliResult {
    let input = matches.get_one::<PathBuf>("file").expect("required arg");
    let paths = values_t!(matches, "paths", String).unwrap_or_default();
    let paths = paths
        .iter()
        .filter_map(|s| Pattern::new(s).ok())
        .collect::<Vec<_>>();

    let walk = hpk::walk(input)?;

    fn matches_path(path: &Path, paths: &[Pattern]) -> bool {
        if paths.is_empty() {
            return true;
        }
        for p in paths {
            if p.matches_path(path) {
                return true;
            }
        }
        false
    }

    for dent in walk.flatten() {
        if !matches_path(dent.path(), &paths) {
            continue;
        }
        if !dent.is_dir() {
            println!("{}", dent.path().display());
        }
    }
    Ok(())
}
