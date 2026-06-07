#[cfg(target_os = "vita")]
#[no_mangle]
pub extern "C" fn fchown(_fd: i32, _owner: u32, _group: u32) -> i32 {
    -1
}

#[cfg(target_os = "vita")]
#[no_mangle]
pub unsafe extern "C" fn readlink(
    _path: *const core::ffi::c_char,
    _buf: *mut core::ffi::c_char,
    _bufsiz: usize,
) -> isize {
    -1
}
