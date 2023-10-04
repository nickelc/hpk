# HPK Archiver for Haemimont Engine game files

[![Crates.io][crates-badge]][crates-url]
![Rust version][rust-version]
[![Actions build status][actions-badge]][actions-url]
[![Downloads][downloads-badge]][downloads-url]

[crates-badge]: https://img.shields.io/crates/v/hpk.svg
[crates-url]: https://crates.io/crates/hpk
[rust-version]: https://img.shields.io/badge/rust-1.70%2B-blue.svg
[actions-badge]: https://github.com/nickelc/hpk/workflows/ci/badge.svg
[actions-url]: https://github.com/nickelc/hpk/actions
[downloads-badge]: https://img.shields.io/github/downloads/nickelc/hpk/total?color=red
[downloads-url]: https://github.com/nickelc/hpk/releases

[lua-url]: https://www.lua.org
[unluac-url]: https://sourceforge.net/projects/unluac
[luadec-url]: https://github.com/viruscamp/luadec

## Supported games
* Tropico 3-5
* The First Templar
* Omerta: City of Gangsters
* Grand Ages: Rome
* Victor Vran
* Surviving Mars

## Overview
1. [Compatibility Notes](#compatibility-notes)
    1. [Game Scripting](#game-scripting)
2. [Building](#building)
3. [Installation](#installation)
4. [Usage](#usage)
    1. [`hpk help`](#hpk-help)
    2. [`hpk list`](#hpk-list)
    3. [`hpk create`](#hpk-create)
    4. [`hpk extract`](#hpk-extract)
    5. [`hpk debug-print`](#hpk-debug-print)
5. [HPK File Format](#hpk-file-format)

## Compatibility Notes

Surviving Mars hangs when hpk files in the `DLC` folder contain compressed files.
Use the `--dont-compress-files` option when creating files.

### Game Scripting
Haeminont uses [Lua][lua-url] for scripting their games.
The scripts contain only the compiled Lua bytecode and have to be decompiled
with [unluac][unluac-url] or [luadec][luadec-url] to get the source code.
With Victor Vran they switched from Lua `5.1` to `5.3` but the bytecode
headers are missing two bytes to make it possible to decompile the scripts.

|        | Victor Vran's Lua Bytecode Header                       |
|--------|---------------------------------------------------------|
| Broken | `1B4C 7561 5300 1993 0D0A 1A0A 0404 ____ 0878 5600 ...` |
| Valid  | `1B4C 7561 5300 1993 0D0A 1A0A 0404 0404 0878 5600 ...` |

|        | Surviving Mars' Lua Bytecode Header                     |
|--------|---------------------------------------------------------|
| Broken | `1B4C 7561 5300 1993 0D0A 1A0A 0404 ____ 0878 5600 ...` |
| Valid  | `1B4C 7561 5300 1993 0D0A 1A0A 0404 0408 0878 5600 ...` |

Use `hpk extract --fix-lua-files ...` to fix the scripts on extraction.<br>
Use `hpk create --cripple-lua-files ...` to change the bytecode header of the compiled lua scripts on repacking.
The game can also load plain Lua files instead of a compiled version if you run into any problems.

## Building

hpk is written in Rust, so you'll need to grab a
[Rust installation](https://www.rust-lang.org/) in order to compile it.
Building is easy:

```
$ git clone https://github.com/nickelc/hpk
$ cd hpk
$ cargo build --release
$ ./target/release/hpk --version
hpk 0.3.0
```

## Installation

### Cargo
```bash
$ cargo install hpk
```

### Windows

Use the GitHub [Releases](https://github.com/nickelc/hpk/releases) tab to obtain the binary.

## Usage

### hpk help
```
$ hpk help
HPK archiver for Haemimont Engine game files (Tropico 3-5, Omerta, Victor Vran, Surviving Mars etc.)

Usage: hpk <COMMAND>

Commands:
  create       Create a new hpk archive
  extract      Extract files from a hpk archive
  list         List the content of a hpk archive
  debug-print  Print debug information of a hpk archive
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

https://github.com/nickelc/hpk
```

### hpk list
```bash
$ hpk list files/omerta/Packs/TextureLists.hpk
entities.lst
fallback.lst
fx.lst
misc.lst
sky.lst
terrains.lst
water.lst
```

### hpk create
```text
$ hpk create -h
Create a new hpk archive

Usage: hpk create [OPTIONS] <dir> <file>

Arguments:
  <dir>   input directory
  <file>  hpk output file

Options:
      --compress
          Compress the whole hpk file
      --chunk-size <SIZE>
          Default chunk size: 32768
      --cripple-lua-files
          Cripple bytecode header for Victor Vran or Surviving Mars
      --with-filedates
          Stores the last modification times in a _filedates file
      --filedate-fmt <FORMAT>
          Specifies the format of the stored filedates.

          default: 'Windows file time' used by Tropico 3 and Grand Ages: Rome
          short: 'Windows file time / 2000' used by Tropico 4 and Omerta
      --dont-compress-files
          No files are compressed. Overrides `--extensions`
      --extensions <EXT>...
          Specifies the file extensions to be compressed. default: [lst,lua,xml,tga,dds,xtex,bin,csv]
      --lz4
          Sets LZ4 as encoder
  -h, --help
          Print help (see more with '--help')
```

### hpk extract
```text
$ hg extract -h
Extract files from a hpk archive

Usage: hpk extract [OPTIONS] <file> <dest> [paths]...

Arguments:
  <file>      hpk archive
  <dest>      destination folder
  [paths]...  An optional list of archive members to be processed, separated by spaces.

Options:
      --ignore-filedates  Skip processing of a _filedates file and just extract it
      --fix-lua-files     Fix the bytecode header of Victor Vran's or Surviving Mars' Lua files
      --force             Force extraction if destination folder is not empty
  -v                      Verbosely list files processed
  -h, --help              Print help
```

### hpk debug-print
```bash
$ hpk debug-print files/omerta/Packs/TextureLists.hpk
reading file: files/omerta/Packs/TextureLists.hpk
header:
  data_offset: 0x24
  fragments_residual_offset: 0x0
  fragments_residual_count: 0
  fragments_per_file: 1
  fragments_filesystem_offset: 0x1459
  fragments_filesystem_length: 64
filesystem entries: 8
filesystem fragments:
  0x13D1   len: 136
  0x24     len: 3876
  0xF48    len: 278
  0x105E   len: 82
  0x10B0   len: 134
  0x1136   len: 140
  0x11C2   len: 304
  0x12F2   len: 223
dir:  index=1 depth=0 ""
 fragment: 0x13D1 len: 136
file: index=2 depth=1 "entities.lst"
 fragment: 0x24 len: 3876
 compressed: ZLIB inflated_length=30917 chunk_size=32768 chunks=1
  chunks: 0x10     len: 3860
file: index=3 depth=1 "fallback.lst"
 fragment: 0xF48 len: 278
 compressed: ZLIB inflated_length=2956 chunk_size=32768 chunks=1
  chunks: 0x10     len: 262
file: index=4 depth=1 "fx.lst"
 fragment: 0x105E len: 82
 compressed: ZLIB inflated_length=290 chunk_size=32768 chunks=1
  chunks: 0x10     len: 66
file: index=5 depth=1 "misc.lst"
 fragment: 0x10B0 len: 134
 compressed: ZLIB inflated_length=344 chunk_size=32768 chunks=1
  chunks: 0x10     len: 118
file: index=6 depth=1 "sky.lst"
 fragment: 0x1136 len: 140
 compressed: ZLIB inflated_length=536 chunk_size=32768 chunks=1
  chunks: 0x10     len: 124
file: index=7 depth=1 "terrains.lst"
 fragment: 0x11C2 len: 304
 compressed: ZLIB inflated_length=2268 chunk_size=32768 chunks=1
  chunks: 0x10     len: 288
file: index=8 depth=1 "water.lst"
 fragment: 0x12F2 len: 223
 compressed: ZLIB inflated_length=1978 chunk_size=32768 chunks=1
  chunks: 0x10     len: 207
```

## HPK File Format

### Header

| Offset | Size | Value                                         |
|--------|------|-----------------------------------------------|
| 0      | 4    | Magic number; `0x4C555042` (`"BPUL"`)         |
| 4      | 4    | Data offset; `0x24` (`36`)                    |
| 8      | 4    | Number of fragments per file; `1`, `8`        |
| 12     | 4    | Unknown; `0xFFFFFFFF`                         |
| 16     | 4    | Offset of the residual fragments in bytes     |
| 20     | 4    | Number of residual fragments                  |
| 24     | 4    | Unknown; `0x1`                                |
| 28     | 4    | Offset of the filesystem fragments in bytes   |
| 32     | 4    | Size of the filesystem fragments in bytes     |

### Fragment (Filesystem & Residual)

* Offsets of fragments are relative from the start of the file.
* The first filesystem fragment is the root directory.

| Offset | Size | Value                                     |
|--------|------|-------------------------------------------|
| 0      | 4    | Offset of a fragment in bytes             |
| 4      | 4    | Size of a fragment in bytes               |

### Filesystem Entry: Directory

| Offset | Size | Value                                     |
|--------|------|-------------------------------------------|
| 0      | 4    | Fragment Index; Index starts with `1`     |
| 4      | 4    | Entry Type; File=`0x0` Dir=`0x1`          |
| 8      | 2    | Size of the following name in bytes       |
| 10     | ?    | Name data                                 |

### Fragmented File (zlib/lz4/zstd compressed)

Offsets of compressed chunks are relative from the start of a fragment.<br>
Victor Vran (Steam version) and Surviving Mars use LZ4 as compression.<br>
ZSTD is used by Surviving Mars's mod editor.<br>
The challenge hpks of Tropico 5 are compressed like a fragmented file.

| Offset | Size | Value                                         |
|--------|------|-----------------------------------------------|
| 0      | 4    | Magic number, `0x42494C5A` (`"ZLIB"`)<br>`0x20345A4C` (`"LZ4 "`)<br>`0x4454535A` (`"ZSTD"`)  |
| 4      | 4    | Size of the inflated data in bytes            |
| 8      | 4    | Inflated chunk size, `0x0800` (`32768`)       |
| 12     | 4    | Chunk offset in bytes, `0x10` for one chunk   |
| *      | 4    | Additional chunk offsets (optional)           |

### `_filedates` File
HPK files can contain a `_filedates` file with the last modification times of the files and directories.<br>
Each line consists of a path and a file time, separated by equals signs.
```
path/to/file=value
path/to/folder=value
```
Path is the basic path to the file but for Grand Ages: Rome the actual path is prefixed by<br>
the basename of the original HPK file (Shaders.hpk `"Shaders/Shaders/VertexAnim.fx"`).<br>
Value is either the Windows file time (`"default"`) or the Windows file time divided by 2000 (`"short"`).

| Format  | Games                       |
|---------|-----------------------------|
| default | Grand Ages: Rome, Tropico 3 |
| short   | Tropico 4, Omerta           |
