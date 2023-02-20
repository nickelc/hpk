use std::fs;
use std::path::{Path, PathBuf};

use clap::{arg, ArgMatches, Command};

use crate::CliResult;

const HPK_PK: &[u8] = include_bytes!("../../hpk.pk");
const HPK_MAP: &[u8] = include_bytes!("../../hpk.map");

pub fn cmd() -> Command {
    fn output_parser(value: &str) -> Result<PathBuf, String> {
        let path = Path::new(value);
        match path.metadata() {
            Ok(md) if md.is_dir() => Ok(path.to_path_buf()),
            Ok(_) | Err(_) => Err(String::from("Not a valid directory")),
        }
    }
    Command::new("pickle")
        .about("Get type definition files for GNU poke")
        .arg(
            arg!([output] "Directory in which the files are saved")
                .value_parser(output_parser)
        )
        .arg(arg!(pickle: --"pickle-only" "Dump only the `hpk.pk` file"))
}

pub fn execute(matches: &ArgMatches) -> CliResult {
    let output = matches.get_one::<PathBuf>("output");
    let pickle_only = matches.get_flag("pickle");

    let mut hpk_pk = PathBuf::from("hpk.pk");
    let mut hpk_map = PathBuf::from("hpk.map");
    if let Some(parent) = output {
        hpk_pk = parent.join(hpk_pk);
        hpk_map = parent.join(hpk_map);
    }

    println!("writing {}", hpk_pk.display());
    fs::write(hpk_pk, HPK_PK)?;
    if !pickle_only {
        println!("writing {}", hpk_map.display());
        fs::write(hpk_map, HPK_MAP)?;
    }
    Ok(())
}
