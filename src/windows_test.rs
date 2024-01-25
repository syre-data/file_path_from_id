use super::*;
use std::iter::once;

#[test]
fn get_volume_handle_from_path_works() {
    let volume = "C:\\".encode_utf16().chain(once(0)).collect();
    unsafe {
        get_volume_handle_from_path(&volume).unwrap();
    }
}
