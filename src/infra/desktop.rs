use std::{
    io,
    path::{Path, PathBuf},
};

#[cfg(windows)]
pub fn downloads_directory() -> io::Result<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_Downloads, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath},
    };

    let mut pointer = std::ptr::null_mut();
    // SAFETY: the API writes an allocated, NUL-terminated UTF-16 string to pointer.
    // Copy it before freeing with CoTaskMemFree on both success and failure.
    // DONT_VERIFY returns the configured path even when absent; CREATE is never set.
    unsafe {
        let result = SHGetKnownFolderPath(
            &FOLDERID_Downloads,
            KF_FLAG_DONT_VERIFY as u32,
            std::ptr::null_mut(),
            &mut pointer,
        );
        let path = if result >= 0 && !pointer.is_null() {
            let mut length = 0;
            while *pointer.add(length) != 0 {
                length += 1;
            }
            Ok(PathBuf::from(OsString::from_wide(
                std::slice::from_raw_parts(pointer, length),
            )))
        } else {
            Err(io::Error::other(format!(
                "Windows Known Folder error: 0x{result:08X}"
            )))
        };
        CoTaskMemFree(pointer.cast());
        path
    }
}

#[cfg(not(windows))]
pub fn downloads_directory() -> io::Result<PathBuf> {
    dirs::download_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "ダウンロードフォルダが OS に設定されていません",
        )
    })
}

pub fn open_directory(path: &Path) -> io::Result<()> {
    if !std::fs::metadata(path)?.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "出力先がフォルダではありません",
        ));
    }
    #[cfg(windows)]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(windows, target_os = "macos")))]
    let program = "xdg-open";
    let status = super::process::command(program).arg(path).status()?;
    // Explorer may return 1 when handing the request to an existing Explorer process.
    #[cfg(not(windows))]
    if !status.success() {
        return Err(io::Error::other(format!("{program}: {status}")));
    }
    #[cfg(windows)]
    let _ = status;
    Ok(())
}
