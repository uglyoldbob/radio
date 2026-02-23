mod internal;

pub fn install_swu(fp: std::path::PathBuf) -> Result<(), ()> {
    use std::io::Read;
    let metadata = std::fs::metadata(&fp).map_err(|_| ())?;
    let swu_length = metadata.len() as usize;

    let f = std::fs::File::open(fp).map_err(|_| ())?;
    let mut swu_reader = std::io::BufReader::new(f);

    let info = {
        use std::ffi::CString;
        use std::os::raw::c_char;
        let s = "update.swu";
        let c_string = CString::new(s).expect("String contains internal null bytes");
        let bytes_with_nul = c_string.as_bytes_with_nul();
        if bytes_with_nul.len() > 512 {
            panic!("String is too long for the [c_char; 512] buffer");
        }
        let mut c_array: [c_char; 512] = [0; 512];
        for (dest, src) in c_array.iter_mut().zip(bytes_with_nul.iter()) {
            *dest = *src as c_char;
        }
        c_array
    };
    let mut req = internal::swupdate_request {
        apiversion: internal::SWUPDATE_API_VERSION,
        source: internal::sourcetype_SOURCE_LOCAL,
        dry_run: internal::run_type_RUN_INSTALL,
        len: swu_length,
        info,
        software_set: [0; 256],
        running_mode: [0; 256],
        disable_store_swu: true,
    };
    unsafe { internal::swupdate_prepare_req(&mut req as *mut internal::swupdate_request) };
    let f = unsafe {
        internal::ipc_inst_start_ext(
            &mut req as *mut internal::swupdate_request as *mut std::ffi::c_void,
            std::mem::size_of::<internal::swupdate_request>() as isize,
        )
    };
    {
        let mut buffer = [0_u8; 4096];

        loop {
            let count = swu_reader.read(&mut buffer).map_err(|_| ())?;
            if count == 0 {
                break;
            }
            unsafe { internal::ipc_send_data(f, &mut buffer as *mut u8, count as i32) };
        }
    }
    unsafe { internal::ipc_end(f) };

    unsafe { internal::ipc_wait_for_complete(None) };
    Ok(())
}

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
