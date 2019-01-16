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
        .version(crate_version!())
        .about(crate_description!())
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .subcommand(commands::create::clap())
        .subcommand(commands::extract::clap())
        .subcommand(commands::list::clap())
        .subcommand(commands::print::clap())
        .get_matches();

    match matches.subcommand() {
        ("create", Some(matches)) => commands::create::execute(matches)?,
        ("extract", Some(matches)) => commands::extract::execute(matches)?,
        ("list", Some(matches)) => commands::list::execute(matches)?,
        ("print", Some(matches)) => commands::print::execute(matches)?,
        _ => unreachable!(),
    };
    Ok(())
}
