extern crate glob;

use std::fs;
use std::path::Path;

use self::glob::Pattern;
use clap::{App, Arg, ArgMatches, SubCommand};

use hpk;

pub fn clap<'a, 'b>() -> App<'a, 'b> {
    #[allow(clippy::needless_pass_by_value)]
    fn validate_input(value: String) -> Result<(), String> {
        match fs::metadata(value) {
            Ok(ref md) if md.is_file() => Ok(()),
            Ok(_) => Err(String::from("Not a valid file")),
            Err(_) => Err(String::from("Not a valid file")),
        }
    }

    SubCommand::with_name("list")
        .about("List the content of a hpk archive")
        .display_order(20)
        .arg(Arg::from_usage("<file> 'hpk archive'").validator(validate_input))
        .arg(Arg::from_usage("[paths]..."))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "file", String).unwrap();
    let paths = values_t!(matches, "paths", String).unwrap_or_default();
    let paths = paths
        .iter()
        .filter_map(|s| Pattern::new(s).ok())
        .collect::<Vec<_>>();

    let walk = hpk::walk(input).unwrap();

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

    for dent in walk {
        if let Ok(dent) = dent {
            if !matches_path(dent.path(), &paths) {
                continue;
            }
            if !dent.is_dir() {
                println!("{}", dent.path().display());
            }
        }
    }
}
