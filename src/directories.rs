use std::{
    fs::{self, DirEntry},
    path::PathBuf,
};

use crate::{Error, Result};
use std::str::FromStr;

#[derive(Debug)]
pub struct Directory {
    pub path: PathBuf,
    pub directories: Vec<Directory>,
    pub files: Vec<PathBuf>,
}

impl Directory {
    pub fn new(starting_dir: &str) -> Result<Self> {
        create_dirs(Self {
            path: PathBuf::from_str(starting_dir)
                .map_err(|_| Error::InvalidPath(starting_dir.to_owned()))?,
            directories: vec![],
            files: vec![],
        })
    }
}

pub fn get_dir_content<'a>(path: PathBuf) -> Result<Vec<DirEntry>> {
    Ok(fs::read_dir(path)?
        .filter_map(|result| result.ok())
        .collect())
}

pub fn create_dirs(mut directory: Directory) -> Result<Directory> {
    let dirs = get_dir_content(directory.path.clone())?;

    for dir in dirs.iter() {
        if dir.path().is_dir() {
            let this_dir = Directory {
                path: dir.path(),
                directories: vec![],
                files: vec![],
            };
            directory.directories.push(create_dirs(this_dir)?)
        } else if dir.path().is_file() {
            directory.files.push(dir.path())
        }
    }

    Ok(directory)
}
