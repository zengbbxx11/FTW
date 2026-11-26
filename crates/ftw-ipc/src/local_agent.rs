use crate::credentials;
use std::{fs, io, path::Path};

const NAME: &str = "local-agent.credential";

pub fn store(data: &Path, token: &str) -> io::Result<()> {
    let protected = credentials::protect(token)?;
    let destination = data.join(NAME);
    let staged = data.join("local-agent.credential.tmp");
    fs::write(&staged, protected)?;
    fs::rename(staged, destination)
}

pub fn load(data: &Path) -> io::Result<String> {
    credentials::read(&fs::read(data.join(NAME))?)
}

pub fn remove(data: &Path) -> io::Result<()> {
    match fs::remove_file(data.join(NAME)) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
