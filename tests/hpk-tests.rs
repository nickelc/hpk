extern crate hpk;
extern crate tempdir;

use std::env;
use std::io::prelude::*;
use std::io;
use std::fs;

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
    create_file("test1/six_bytes", Some("ABCDEF"));
    create_file("test1/two_bytes", Some("AB"));

    {
        let mut out = fs::File::create("test1.hpk").unwrap();
        let options = hpk::CreateOptions::new();
        hpk::create(options, "test1", &mut out).unwrap();
    }

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
        hpk::compress::<hpk::compress::Zlib>(&mut file, &mut out).unwrap();
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
