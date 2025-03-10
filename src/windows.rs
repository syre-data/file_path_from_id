use file_id::FileId;
use std::{
    fs,
    io::{self},
    mem,
    os::windows::io::AsRawHandle,
    path::PathBuf,
    ptr::null,
};
use windows_sys::Win32::Foundation::HANDLE;

#[cfg_attr(feature = "tracing", tracing::instrument(level = "debug"))]
pub fn path_from_id(id: &FileId) -> Result<PathBuf, Error> {
    let file = unsafe { file_handle_from_id(id)? };
    unsafe { path_from_handle(&file) }
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "debug"))]
pub fn path_from_file(file: &fs::File) -> Result<PathBuf, Error> {
    unsafe { path_from_handle(file) }
}

// Gets the path to a file from its handle.
#[cfg_attr(feature = "tracing", tracing::instrument(level = "trace"))]
unsafe fn path_from_handle(file: &fs::File) -> Result<PathBuf, Error> {
    use windows_sys::Win32::{
        Foundation::MAX_PATH,
        Storage::FileSystem::{FILE_NAME_NORMALIZED, GetFinalPathNameByHandleW},
    };
    use windows_sys::core::PWSTR;

    let path = [0; MAX_PATH as usize];
    let size = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle() as HANDLE,
            path.as_ptr() as PWSTR,
            MAX_PATH,
            FILE_NAME_NORMALIZED,
        )
    };

    if size == 0 {
        #[cfg(feature = "tracing")]
        tracing::trace!("could not get path from handle");

        Err(Error::FinalPathName(io::Error::last_os_error()))
    } else if size > MAX_PATH {
        #[cfg(feature = "tracing")]
        tracing::trace!("could not get path from handle");

        Err(Error::FinalPathName(io::Error::new(
            io::ErrorKind::OutOfMemory,
            format!("path buffer requires {size} bytes but only {MAX_PATH} were allocated"),
        )))
    } else {
        let path = path.into_iter().take(size as usize).collect::<Vec<_>>();
        let Ok(path) = String::from_utf16(path.as_slice()) else {
            return Err(Error::FinalPathName(io::Error::new(
                io::ErrorKind::InvalidData,
                "could not decode path",
            )));
        };

        Ok(PathBuf::from(PathBuf::from(path)))
    }
}

/// Gets a file handle from an id.
#[cfg_attr(feature = "tracing", tracing::instrument(level = "trace"))]
unsafe fn file_handle_from_id(file_id: &FileId) -> Result<fs::File, Error> {
    use std::{os::raw::c_void, os::windows::prelude::*};
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{
            ExtendedFileIdType, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_ID_128,
            FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FILE_SHARE_READ, OpenFileById,
        },
    };

    match file_id {
        FileId::HighRes {
            volume_serial_number,
            file_id,
        } => {
            let volume_path_name =
                unsafe { get_volume_path_name_from_serial_number(volume_serial_number.clone())? };

            let file_id_descriptor = FILE_ID_DESCRIPTOR {
                dwSize: mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
                Type: ExtendedFileIdType,
                Anonymous: FILE_ID_DESCRIPTOR_0 {
                    ExtendedFileId: FILE_ID_128 {
                        Identifier: file_id.to_ne_bytes(),
                    },
                },
            };

            let volume_handle = unsafe { get_volume_handle_from_path(&volume_path_name)? };
            let handle = unsafe {
                OpenFileById(
                    volume_handle as HANDLE,
                    &file_id_descriptor as *const FILE_ID_DESCRIPTOR,
                    FILE_GENERIC_READ,
                    FILE_SHARE_READ,
                    null() as *const SECURITY_ATTRIBUTES,
                    FILE_FLAG_BACKUP_SEMANTICS,
                )
            };

            if handle == INVALID_HANDLE_VALUE {
                #[cfg(feature = "tracing")]
                tracing::trace!("could not get file handle from id");

                return Err(Error::OpenFile(io::Error::last_os_error()));
            }

            let file = unsafe { fs::File::from_raw_handle(handle as *mut c_void) };
            Ok(file)
        }

        FileId::LowRes {
            volume_serial_number: _,
            file_index: _,
        } => todo!(),

        FileId::Inode {
            device_id: _,
            inode_number: _,
        } => return Err(Error::InvalidFileId),
    }
}

/// Gets the volume path from its serial number.
#[cfg_attr(feature = "tracing", tracing::instrument(level = "trace"))]
unsafe fn get_volume_path_name_from_serial_number(serial_number: u64) -> Result<Vec<u16>, Error> {
    use windows_sys::Win32::{
        Foundation::{ERROR_NO_MORE_FILES, GetLastError, INVALID_HANDLE_VALUE, MAX_PATH},
        Storage::FileSystem::{FindFirstVolumeW, FindNextVolumeW, FindVolumeClose},
    };
    use windows_sys::core::PWSTR;
    let volume_name = [0; MAX_PATH as usize];
    let volume_handle = unsafe { FindFirstVolumeW(volume_name.as_ptr() as PWSTR, MAX_PATH) };

    loop {
        if volume_handle == INVALID_HANDLE_VALUE {
            #[cfg(feature = "tracing")]
            tracing::trace!("could not get file handle from id");

            return Err(Error::FindVolume(io::Error::last_os_error()));
        }

        let volume_path_names = unsafe { get_volume_path_names(&volume_name)? };
        for path_name in volume_path_names {
            let volume_path_sn = unsafe { get_volume_serial_number_from_path(&path_name)? };
            if volume_path_sn == serial_number {
                return Ok(path_name);
            }
        }

        let ret = unsafe {
            FindNextVolumeW(
                volume_handle as HANDLE,
                volume_name.as_ptr() as PWSTR,
                MAX_PATH,
            )
        };

        if ret == 0 {
            if unsafe { GetLastError() } == ERROR_NO_MORE_FILES {
                unsafe {
                    FindVolumeClose(volume_handle as HANDLE);
                }
                break;
            } else {
                #[cfg(feature = "tracing")]
                tracing::trace!("could not get volume path name from serial number");

                return Err(Error::FindVolume(io::Error::last_os_error()));
            }
        }
    }

    Err(Error::FindVolume(io::Error::new(
        io::ErrorKind::NotFound,
        "no volume matching the serial number",
    )))
}

/// Get a paths within the given volume.
#[cfg_attr(feature = "tracing", tracing::instrument(level = "trace"))]
unsafe fn get_volume_path_names(volume_name: &[u16]) -> Result<Vec<Vec<u16>>, Error> {
    use windows_sys::Win32::{
        Foundation::MAX_PATH, Storage::FileSystem::GetVolumePathNamesForVolumeNameW,
    };
    use windows_sys::core::PWSTR;

    let volume_paths = [0; MAX_PATH as usize];
    let mut volume_paths_size: u32 = 0;
    let ret = unsafe {
        GetVolumePathNamesForVolumeNameW(
            volume_name.as_ptr(),
            volume_paths.as_ptr() as PWSTR,
            MAX_PATH,
            &mut volume_paths_size as *mut u32,
        )
    };

    if ret == 0 {
        #[cfg(feature = "tracing")]
        tracing::trace!("could not get volume path names");

        return Err(Error::VolumePathNames(io::Error::last_os_error()));
    }

    let mut volume_path_names = Vec::with_capacity((volume_paths_size / 8) as usize);
    let mut volume_path = Vec::<u16>::with_capacity(8);
    let mut idx: usize = 0;
    while idx < volume_paths_size as usize {
        let c = volume_paths[idx];
        if c == 0 {
            if volume_path.len() > 0 {
                volume_path.push(0); // terminating null byte
                volume_path_names.push(volume_path.clone());
                volume_path.clear();
            }
        } else {
            volume_path.push(c);
        }

        idx += 1;
    }

    Ok(volume_path_names)
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "trace"))]
unsafe fn get_volume_serial_number_from_path(path_name: &Vec<u16>) -> Result<u64, Error> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };

    let file_handle = unsafe { get_volume_handle_from_path(path_name)? };
    let mut info: FILE_ID_INFO = unsafe { mem::zeroed() };
    let ret = unsafe {
        GetFileInformationByHandleEx(
            file_handle,
            FileIdInfo,
            &mut info as *mut FILE_ID_INFO as _,
            mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };

    if ret == 0 {
        #[cfg(feature = "tracing")]
        tracing::trace!("could not get volume serial number from path");

        return Err(Error::FileInformationByHandle(io::Error::last_os_error()));
    }

    Ok(info.VolumeSerialNumber)
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "trace"))]
unsafe fn get_volume_handle_from_path(path_name: &Vec<u16>) -> Result<HANDLE, Error> {
    use std::os::raw::c_void;
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, OPEN_EXISTING,
        },
    };

    let file_handle = unsafe {
        CreateFileW(
            path_name.as_ptr(),
            0,
            FILE_SHARE_READ,
            null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            null::<*mut c_void>() as HANDLE,
        )
    };

    if file_handle == INVALID_HANDLE_VALUE {
        #[cfg(feature = "tracing")]
        tracing::trace!("could not get volume handle from path");

        return Err(Error::VolumeHandle(io::Error::last_os_error()));
    }

    Ok(file_handle as HANDLE)
}

#[derive(Debug)]
pub enum Error {
    InvalidFileId,
    VolumeHandle(io::Error),
    FileInformationByHandle(io::Error),
    FindVolume(io::Error),
    VolumePathNames(io::Error),
    OpenFile(io::Error),
    FinalPathName(io::Error),
}

#[cfg(test)]
mod test {
    use std::fs;

    #[cfg(feature = "tracing")]
    use test_log::test;

    #[test]
    pub fn get_path_from_id() {
        const FILENAME: &str = "__tmp_id__";
        let path = std::env::current_dir().unwrap().join(FILENAME);
        let f = fs::File::create(&path).unwrap();
        let id = file_id::get_file_id(&path).unwrap();

        let path = fs::canonicalize(&path).unwrap();
        drop(f);

        let found = super::path_from_id(&id).unwrap();
        fs::remove_file(&path).unwrap();
        assert_eq!(found, path);
    }

    #[test]
    pub fn get_path_from_file() {
        const FILENAME: &str = "__tmp_handle__";
        let path = std::env::current_dir().unwrap().join(FILENAME);
        let f = fs::File::create(&path).unwrap();
        let found = super::path_from_file(&f).unwrap();

        let path = fs::canonicalize(&path).unwrap();
        fs::remove_file(&path).unwrap();

        assert_eq!(found, path);
    }
}
