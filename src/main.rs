mod commands;

use clap::Command;

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
    let matches = Command::new("hpk")
        .version(clap::crate_version!())
        .about(clap::crate_description!())
        .after_help("https://github.com/nickelc/hpk")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(commands::create::cmd())
        .subcommand(commands::extract::cmd())
        .subcommand(commands::list::cmd())
        .subcommand(commands::print::cmd())
        .get_matches();

    match matches.subcommand() {
        Some(("create", matches)) => commands::create::execute(matches)?,
        Some(("extract", matches)) => commands::extract::execute(matches)?,
        Some(("list", matches)) => commands::list::execute(matches)?,
        Some(("debug-print", matches)) => commands::print::execute(matches)?,
        _ => unreachable!(),
    };
    Ok(())
}
