extern crate hpk;
extern crate tempdir;

use std::env;
use std::io::prelude::*;
use std::io;
use std::fs;
use std::path::Path;

macro_rules! assert_path_exists {
    ($p:expr) => {
        assert!(Path::new($p).exists(), format!("{} does not exist", $p));
    }
}

#[test]
fn create_extract_and_compress() {
    fn create_dir(path: &str) {
        fs::create_dir(path).unwrap();
    }

    fn create_file(path: &str, content: Option<&str>) {
        let mut file = fs::File::create(path).unwrap();
        if let Some(content) = content {
            file.write(content.as_bytes()).unwrap();
        }
    }

    let root = tempdir::TempDir::new("hpk-tests");
    let root = root.expect("Should have created a temp director");
    assert!(env::set_current_dir(root.path()).is_ok());

    create_dir("test1");
    create_file("test1/compressed.lst", Some("Hello World, Hello World"));
    create_file("test1/empty_compressed.lst", None);
    create_file("test1/empty_file", None);
    create_dir("test1/empty_folder");
    create_dir("test1/folder");
    create_file("test1/folder/six_bytes", Some("ABCDEF"));
    create_file("test1/two_bytes", Some("AB"));

    {
        let options = Default::default();
        hpk::create(options, "test1", "test1.hpk").unwrap();
    }

    {
        let options = Default::default();
        hpk::extract(options, "test1.hpk", "test1-extracted").expect("could not extract test1.hpk");
    }

    assert_path_exists!("test1-extracted");
    assert_path_exists!("test1-extracted/compressed.lst");
    assert_path_exists!("test1-extracted/empty_compressed.lst");
    assert_path_exists!("test1-extracted/empty_file");
    assert_path_exists!("test1-extracted/empty_folder");
    assert_path_exists!("test1-extracted/folder/six_bytes");
    assert_path_exists!("test1-extracted/two_bytes");

    let mut walk = hpk::walk("test1.hpk").unwrap();
    assert!(!walk.is_compressed());

    while let Some(Ok(dent)) = walk.next() {
        if !dent.is_dir() {
            walk.read_file(&dent, |mut r| {
                io::copy(&mut r, &mut io::sink()).unwrap();
                Ok(())
            }).unwrap();
        }
    }

    {
        let mut file = fs::File::open("test1.hpk").unwrap();
        let mut out = fs::File::create("test1-compressed.hpk").unwrap();
        let options = Default::default();
        hpk::compress(&options, &mut file, &mut out).unwrap();
    }

    let mut walk = hpk::walk("test1-compressed.hpk").unwrap();
    assert!(walk.is_compressed());

    while let Some(Ok(dent)) = walk.next() {
        if !dent.is_dir() {
            walk.read_file(&dent, |mut r| {
                io::copy(&mut r, &mut io::sink()).unwrap();
                Ok(())
            }).unwrap();
        }
    }
}
