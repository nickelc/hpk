#[macro_use]
extern crate clap;

mod commands;

use clap::{App, AppSettings};

#[derive(Debug)]
pub enum Error {
    Hpk(hpk::HpkError),
    Clap(clap::Error),
}

impl From<hpk::HpkError> for Error {
    fn from(e: hpk::HpkError) -> Error {
        Error::Hpk(e)
    }
}

impl From<clap::Error> for Error {
    fn from(e: clap::Error) -> Error {
        Error::Clap(e)
    }
}

type CliResult = Result<(), Error>;

fn main() -> CliResult {
    let matches = App::new("hpk")
        .version(clap::crate_version!())
        .about(clap::crate_description!())
        .after_help("https://github.com/nickelc/hpk")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(commands::create::clap())
        .subcommand(commands::extract::clap())
        .subcommand(commands::list::clap())
        .subcommand(commands::print::clap())
        .get_matches();

    match matches.subcommand() {
        Some(("create", matches)) => commands::create::execute(matches)?,
        Some(("extract", matches)) => commands::extract::execute(matches)?,
        Some(("list", matches)) => commands::list::execute(matches)?,
        Some(("print", matches)) => commands::print::execute(matches)?,
        _ => unreachable!(),
    };
    Ok(())
}
