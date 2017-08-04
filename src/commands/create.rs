use std::fs::File;
use std::path::{Path, PathBuf};

use clap::{App, Arg, ArgMatches, SubCommand};

use hpk;

pub fn clap<'a, 'b>() -> App<'a, 'b> {
    fn validate_dir(value: String) -> Result<(), String> {
        let path = Path::new(&value);
        if let Ok(md) = path.metadata() {
            if md.is_dir() {
                return Ok(());
            }
        }
        Err(String::from("Not a valid directory"))
    }

    SubCommand::with_name("create")
        .about("create a new hpk archive")
        .display_order(0)
        .arg(Arg::from_usage("<dir> 'input directory'")
                .validator(validate_dir))
        .arg(Arg::from_usage("<file> 'hpk output file'"))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "dir", String).unwrap();
    let file = value_t!(matches, "file", String).unwrap();

    let mut out = File::create(file).unwrap();
    hpk::write_hpk(PathBuf::from(input), &mut out).unwrap();
}

