use file_id::FileId;
use std::io;
use std::path::PathBuf;
use std::process::Command;

#[cfg_attr(feature = "tracing", tracing::instrument(level = "debug"))]
pub fn path_from_id(id: &FileId) -> Result<PathBuf, Error> {
    match id {
        FileId::Inode {
            device_id,
            inode_number,
        } => get_path_from_id(device_id, inode_number),
        _ => Err(Error::InvalidFileId),
    }
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "trace"))]
fn get_path_from_id(device_id: &u64, inode_number: &u64) -> Result<PathBuf, Error> {
    let output = match Command::new("sh")
        .arg("-c")
        .arg(format!("getfileinfo /.vol/{device_id}/{inode_number}"))
        .output()
    {
        Ok(output) => output,
        Err(err) => return Err(Error::Command(err)),
    };

    let output = match String::from_utf8(output.stdout) {
        Ok(output) => output,
        Err(err) => return Err(Error::Decode(err)),
    };

    for line in output.split("\n") {
        let Some((key, value)) = line.split_once(":") else {
            continue;
        };

        let key = key.trim();
        if key == "directory" || key == "file" {
            let file = value.trim().trim_matches('"');
            return Ok(PathBuf::from(file));
        }
    }

    Err(Error::NoFileInfo)
}

#[derive(Debug)]
pub enum Error {
    InvalidFileId,
    Command(io::Error),
    Decode(std::string::FromUtf8Error),
    NoFileInfo,
}
