use std::path::{Path, PathBuf};

use clap::{arg, ArgMatches, Command};
use glob::Pattern;

use crate::CliResult;

pub fn cmd() -> Command {
    fn input_parser(value: &str) -> Result<PathBuf, String> {
        let file = Path::new(value);
        match file.metadata() {
            Ok(ref md) if md.is_file() => Ok(file.to_path_buf()),
            Ok(_) | Err(_) => Err(String::from("Not a valid file")),
        }
    }

    Command::new("list")
        .about("List the content of a hpk archive")
        .display_order(20)
        .arg(arg!(<file> "hpk archive").value_parser(input_parser))
        .arg(arg!([paths]...).value_parser(Pattern::new))
}

pub fn execute(matches: &ArgMatches) -> CliResult {
    let input = matches.get_one::<PathBuf>("file").expect("required arg");
    let paths = matches
        .get_many::<Pattern>("paths")
        .map(Iterator::collect::<Vec<_>>)
        .unwrap_or_default();

    let walk = hpk::walk(input)?;

    fn matches_path(path: &Path, paths: &[&Pattern]) -> bool {
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
