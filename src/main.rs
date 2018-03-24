#[macro_use]
extern crate clap;

extern crate hpk;

mod commands;

use clap::{App, AppSettings};

fn main() {
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
        ("create", Some(matches)) => commands::create::execute(matches),
        ("extract", Some(matches)) => commands::extract::execute(matches),
        ("list", Some(matches)) => commands::list::execute(matches),
        ("print", Some(matches)) => commands::print::execute(matches),
        ("", None) => unreachable!(),
        _ => unreachable!(),
    };
}
