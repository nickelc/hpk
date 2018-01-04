use std::fs::File;
use std::path::Path;

use clap::{App, Arg, ArgMatches, SubCommand};

use hpk;


pub fn clap<'a, 'b>() -> App<'a, 'b> {
    fn validate_input(value: String) -> Result<(), String> {
        let path = Path::new(&value);
        match path.metadata() {
            Ok(ref md) if md.is_file() => Ok(()),
            Ok(_) => Err(String::from("Not a valid file")),
            Err(_) => Err(String::from("Not a valid file")),
        }
    }

    SubCommand::with_name("list")
        .about("list the content of a hpk archive")
        .display_order(20)
        .arg(Arg::from_usage("<file> 'hpk archive'")
                .validator(validate_input))
}

pub fn execute(matches: &ArgMatches) {
    let input = value_t!(matches, "file", String).unwrap();
    let mut f = File::open(input).unwrap();

    let mut visitor = ListVisitor{};
    hpk::read_hpk(&mut f, &mut visitor);
}


struct ListVisitor;

#[allow(unused_variables)]
impl hpk::ReadVisitor for ListVisitor {

    fn visit_file(&mut self, file: &Path, fragment: &hpk::Fragment, r: &mut File) {
        println!("{}", file.display());
    }
}
