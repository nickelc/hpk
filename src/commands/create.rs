use std::path::{Path, PathBuf};

use clap::builder::{EnumValueParser, PathBufValueParser, PossibleValue};
use clap::{arg, ArgMatches, Command};

use crate::CliResult;

#[derive(Clone, Debug, PartialEq)]
enum FileDateFormat {
    Default,
    Short,
}

impl clap::ValueEnum for FileDateFormat {
    fn value_variants<'a>() -> &'a [Self] {
        &[Self::Default, Self::Short]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue<'a>> {
        match self {
            Self::Default => Some(PossibleValue::new("default")),
            Self::Short => Some(PossibleValue::new("short")),
        }
    }
}

const FILETIME_FMT_HELP: &str = "Specifies the format of the stored filedates.

default: 'Windows file time' used by Tropico 3 and Grand Ages: Rome
short: 'Windows file time / 2000' used by Tropico 4 and Omerta";

const EXTENSIONS_HELP: &str = "Specifies the file extensions to be compressed. \
                               default: [lst,lua,xml,tga,dds,xtex,bin,csv]";

pub fn clap<'a>() -> Command<'a> {
    fn input_parser(value: &str) -> Result<PathBuf, String> {
        let dir = Path::new(value);
        if let Ok(md) = dir.metadata() {
            if md.is_dir() {
                return Ok(dir.to_path_buf());
            }
        }
        Err(String::from("Not a valid directory"))
    }

    Command::new("create")
        .about("Create a new hpk archive")
        .display_order(0)
        .arg(arg!(--compress "Compress the whole hpk file").display_order(0))
        .arg(arg!(--lz4 "Sets LZ4 as encoder").display_order(10))
        .arg(arg!(chunk_size: --"chunk-size" <SIZE> "Default chunk size: 32768")
                .required(false)
                .value_parser(clap::value_parser!(u32))
                .next_line_help(true))
        .arg(arg!(cripple_lua: --"cripple-lua-files" "Cripple bytecode header for Victor Vran or Surviving Mars"))
        .arg(arg!(--"with-filedates" "Stores the last modification times in a _filedates file"))
        .arg(
            arg!(--"filedate-fmt" <FORMAT>)
                .required(false)
                .default_value_if("with-filedates", None, Some("default"))
                .value_parser(EnumValueParser::<FileDateFormat>::new())
                .hide_possible_values(true)
                .next_line_help(true)
                .long_help(FILETIME_FMT_HELP)
        )
        .arg(arg!(no_compress: --"dont-compress-files" "No files are compressed. Overrides `--extensions`"))
        .arg(arg!(--extensions <EXT>...)
                .required(false)
                .multiple_values(true)
                .use_value_delimiter(true)
                .next_line_help(true)
                .long_help(EXTENSIONS_HELP))
        .arg(arg!(<dir> "input directory").value_parser(input_parser))
        .arg(arg!(<file> "hpk output file").value_parser(PathBufValueParser::new()))
}

pub fn execute(matches: &ArgMatches) -> CliResult {
    let input = matches.get_one::<PathBuf>("dir").expect("required arg");
    let file = matches.get_one::<PathBuf>("file").expect("required arg");

    let mut options = hpk::CreateOptions::new();
    if matches.contains_id("compress") {
        options.compress();
    }
    if matches.contains_id("lz4") {
        options.use_lz4();
    }
    if matches.contains_id("cripple_lua") {
        options.cripple_lua_files();
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
    if matches.contains_id("no_compress") {
        options.with_extensions(Vec::new());
    }

    hpk::create(&options, input, file)?;
    Ok(())
}
