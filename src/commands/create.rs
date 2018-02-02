use std::fs;

use clap::{App, Arg, ArgMatches, SubCommand};

use hpk;

arg_enum!{
    #[allow(non_camel_case_types)]
    #[derive(PartialEq, Debug)]
    enum FileDateFormat {
        default,
        short
    }
}

const FILETIME_FMT_HELP: &str = "Specifies the format of the stored filedates.

default: 'Windows file time' used by Tropico 3 and Grand Ages: Rome
short: 'Windows file time / 2000' used by Tropico 4 and Omerta";

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
        .about("Create a new hpk archive")
        .display_order(0)
        .arg(Arg::from_usage("[compress] --compress 'Compress the whole hpk file'"))
        .arg(Arg::from_usage(
            "[filedates] --with-filedates 'Stores the last modification times in a _filedates file'",
        ))
        .arg(
            Arg::from_usage("[filedate-fmt] --filedate-fmt <FORMAT>")
                .default_value_if("filedates", None, "default")
                .possible_values(&FileDateFormat::variants())
                .hide_possible_values(true)
                .next_line_help(true)
                .long_help(FILETIME_FMT_HELP),
        )
        .arg(Arg::from_usage("<dir> 'input directory'")
                .validator(validate_dir))
        .arg(Arg::from_usage("<file> 'hpk output file'"))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "dir", String).unwrap();
    let file = value_t!(matches, "file", String).unwrap();

    let mut options = hpk::CreateOptions::new();
    if matches.is_present("compress") {
        options.compress();
    }
    if let Ok(fmt) = value_t!(matches, "filedate-fmt", FileDateFormat) {
        match fmt {
            FileDateFormat::default => options.with_default_filedates_format(),
            FileDateFormat::short => options.with_short_filedates_format(),
        }
    }

    hpk::create(options, input, file).unwrap();
}
