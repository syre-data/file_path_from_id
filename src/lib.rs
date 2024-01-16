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

pub fn path_from_id(id: FileId) -> io::Result<PathBuf> {
    let file_handle = unsafe { file_handle_from_id(id)? };
    unsafe { path_from_handle(&file_handle) }
}

// Gets the path to a file from its handle.
unsafe fn path_from_handle(file: &fs::File) -> io::Result<PathBuf> {
    use windows_sys::core::PWSTR;
    use windows_sys::Win32::{
        Foundation::MAX_PATH,
        Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED},
    };

    let path = [0; MAX_PATH as usize];
    let size = GetFinalPathNameByHandleW(
        file.as_raw_handle() as HANDLE,
        path.as_ptr() as PWSTR,
        MAX_PATH,
        FILE_NAME_NORMALIZED,
    );

    if size == 0 {
        Err(io::Error::last_os_error())
    } else if size > MAX_PATH {
        Err(io::Error::new(
            io::ErrorKind::OutOfMemory,
            format!("path buffer requires {size} bytes but only {MAX_PATH} were allocated"),
        ))
    } else {
        let path = path.into_iter().take(size as usize).collect::<Vec<_>>();
        let Ok(path) = String::from_utf16(path.as_slice()) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "could not decode path",
            ));
        };

        Ok(PathBuf::from(PathBuf::from(path)))
    }
}

/// Gets a file handle from an id.
unsafe fn file_handle_from_id(file_id: FileId) -> io::Result<fs::File> {
    use std::{os::raw::c_void, os::windows::prelude::*};
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{
            ExtendedFileIdType, OpenFileById, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ,
            FILE_ID_128, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FILE_SHARE_READ,
        },
    };

    match file_id {
        FileId::HighRes {
            volume_serial_number,
            file_id,
        } => {
            let volume_path_name = get_volume_path_name_from_serial_number(volume_serial_number)?;
            let file_id_descriptor = FILE_ID_DESCRIPTOR {
                dwSize: mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
                Type: ExtendedFileIdType,
                Anonymous: FILE_ID_DESCRIPTOR_0 {
                    ExtendedFileId: FILE_ID_128 {
                        Identifier: file_id.to_ne_bytes(),
                    },
                },
            };

            let volume_handle = get_volume_handle_from_path(&volume_path_name)?;
            let handle = OpenFileById(
                volume_handle as HANDLE,
                &file_id_descriptor as *const FILE_ID_DESCRIPTOR,
                FILE_GENERIC_READ,
                FILE_SHARE_READ,
                null() as *const SECURITY_ATTRIBUTES,
                FILE_FLAG_BACKUP_SEMANTICS,
            );

            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }

            Ok(fs::File::from_raw_handle(handle as *mut c_void))
        }

        _ => todo!(),
    }
}

/// Gets the volume path from its serial number.
unsafe fn get_volume_path_name_from_serial_number(serial_number: u64) -> io::Result<Vec<u16>> {
    use windows_sys::core::PWSTR;
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE, MAX_PATH},
        Storage::FileSystem::{FindFirstVolumeW, FindNextVolumeW, FindVolumeClose},
    };
    let volume_name = [0; MAX_PATH as usize];
    let volume_handle = FindFirstVolumeW(volume_name.as_ptr() as PWSTR, MAX_PATH);

    loop {
        if volume_handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let volume_path_names = get_volume_path_names(&volume_name)?;
        for path_name in volume_path_names {
            let volume_path_sn = get_volume_serial_number_from_path(&path_name)?;
            if volume_path_sn == serial_number {
                return Ok(path_name);
            }
        }

        let ret = FindNextVolumeW(
            volume_handle as HANDLE,
            volume_name.as_ptr() as PWSTR,
            MAX_PATH,
        );

        if ret == 0 {
            if GetLastError() == ERROR_NO_MORE_FILES {
                FindVolumeClose(volume_handle as HANDLE);
                break;
            } else {
                return Err(io::Error::last_os_error());
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no volume matching the serial number",
    ))
}

/// Get a paths within the given volume.
unsafe fn get_volume_path_names(volume_name: &[u16]) -> io::Result<Vec<Vec<u16>>> {
    use windows_sys::core::PWSTR;
    use windows_sys::Win32::{
        Foundation::MAX_PATH, Storage::FileSystem::GetVolumePathNamesForVolumeNameW,
    };

    let volume_paths = [0; MAX_PATH as usize];
    let mut volume_paths_size: u32 = 0;
    let ret = GetVolumePathNamesForVolumeNameW(
        volume_name.as_ptr(),
        volume_paths.as_ptr() as PWSTR,
        MAX_PATH,
        &mut volume_paths_size as *mut u32,
    );

    if ret == 0 {
        return Err(io::Error::last_os_error());
    }

    let mut volume_path_names = Vec::with_capacity((volume_paths_size / 8) as usize);
    let mut volume_path = Vec::<u16>::with_capacity(8);
    let mut idx: usize = 0;
    while idx < volume_paths_size as usize {
        let c = volume_paths[idx];
        if c == 0 {
            if volume_path.len() > 0 {
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

unsafe fn get_volume_serial_number_from_path(path_name: &Vec<u16>) -> io::Result<u64> {
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandleEx, FILE_ID_INFO,
    };

    let file_handle = get_volume_handle_from_path(path_name)?;
    let mut info: FILE_ID_INFO = mem::zeroed();
    let ret = GetFileInformationByHandleEx(
        file_handle,
        FileIdInfo,
        &mut info as *mut FILE_ID_INFO as _,
        mem::size_of::<FILE_ID_INFO>() as u32,
    );

    if ret == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(info.VolumeSerialNumber)
}

unsafe fn get_volume_handle_from_path(path_name: &Vec<u16>) -> io::Result<HANDLE> {
    use std::os::raw::c_void;
    use windows_sys::Win32::{
        Foundation::INVALID_HANDLE_VALUE,
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_SHARE_READ,
            OPEN_EXISTING,
        },
    };

    let file_handle = CreateFileW(
        path_name.as_ptr(),
        FILE_GENERIC_READ,
        FILE_SHARE_READ,
        null(),
        OPEN_EXISTING,
        FILE_FLAG_BACKUP_SEMANTICS,
        null::<*mut c_void>() as HANDLE,
    );

    if file_handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    Ok(file_handle as HANDLE)
}
