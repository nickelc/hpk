use std::path::PathBuf;

use clap::builder::{EnumValueParser, PathBufValueParser};
use clap::{arg, ArgMatches, Command};

use crate::commands::create::{FileDateFormat, EXTENSIONS_HELP, FILETIME_FMT_HELP};
use crate::CliResult;

pub fn cmd() -> Command {
    Command::new("merge")
        .about("Merge a new hpk archive")
        .display_order(0)
        .arg(arg!(--compress "Compress the whole hpk file").display_order(0))
        .arg(arg!(--lz4 "Sets LZ4 as encoder").display_order(10))
        .arg(arg!(chunk_size: --"chunk-size" <SIZE> "Default chunk size: 32768")
                .value_parser(clap::value_parser!(u32))
                .next_line_help(true))
        .arg(arg!(--"with-filedates" "Stores the last modification times in a _filedates file"))
        .arg(
            arg!(--"filedate-fmt" <FORMAT>)
                .default_value_if("with-filedates", "true", Some("default"))
                .value_parser(EnumValueParser::<FileDateFormat>::new())
                .hide_possible_values(true)
                .next_line_help(true)
                .long_help(FILETIME_FMT_HELP)
        )
        .arg(arg!(no_compress: --"dont-compress-files" "No files are compressed. Overrides `--extensions`"))
        .arg(arg!(--extensions <EXT>...)
                .num_args(1..)
                .value_delimiter(',')
                .next_line_help(true)
                .long_help(EXTENSIONS_HELP))
        .arg(arg!(<input>... "input files").value_parser(PathBufValueParser::new()))
        .arg(arg!(<output> "hpk output file").value_parser(PathBufValueParser::new()))
}

pub fn execute(matches: &ArgMatches) -> CliResult {
    let input = matches.get_many::<PathBuf>("input").expect("required arg");
    let output = matches.get_one::<PathBuf>("output").expect("required arg");

    let mut options = hpk::CreateOptions::new();
    if matches.get_flag("compress") {
        options.compress();
    }
    if matches.get_flag("lz4") {
        options.use_lz4();
    }
    if let Some(chunk_size) = matches.get_one::<u32>("chunk_size") {
        options.with_chunk_size(*chunk_size);
    }
    if let Some(fmt) = matches.get_one("filedate-fmt") {
        match fmt {
            FileDateFormat::Default => options.with_default_filedates_format(),
            FileDateFormat::Short => options.with_short_filedates_format(),
        }
    }
    if let Some(extensions) = matches.get_many::<String>("extensions") {
        options.with_extensions(extensions.map(ToOwned::to_owned).collect());
    }
    if matches.get_flag("no_compress") {
        options.with_extensions(Vec::new());
    }

    hpk::merge(&options, input.collect(), output)?;
    Ok(())
}
