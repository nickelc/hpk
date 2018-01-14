use std::fs;

use clap::{App, Arg, ArgMatches, SubCommand};

use hpk;

pub fn clap<'a, 'b>() -> App<'a, 'b> {
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
        .arg(Arg::from_usage("<file> 'hpk archive'")
                .validator(validate_input))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "file", String).unwrap();
    let walk = hpk::walk(input).unwrap();

    for dent in walk {
        if let Ok(dent) = dent {
            if !dent.is_dir() {
                println!("{}", dent.path().display());
            }
        }
    }
}
