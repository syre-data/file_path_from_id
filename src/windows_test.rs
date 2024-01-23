use super::*;

#[test]
fn get_volume_handle_from_path_works() {
    let volume = "C:\\".encode_utf16().collect();
    unsafe {
        get_volume_handle_from_path(&volume).unwrap();
    }
}
