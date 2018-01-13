use std::fs;
use std::fs::File;

use clap::{App, Arg, ArgMatches, SubCommand};

use hpk;

pub fn clap<'a, 'b>() -> App<'a, 'b> {
    fn validate_dir(value: String) -> Result<(), String> {
        if let Ok(md) = fs::metadata(value) {
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
    hpk::create(&input, &mut out).unwrap();
}
