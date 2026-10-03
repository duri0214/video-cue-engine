use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};

/// Only immediate, regular MP4 files are included; directories and symlinks are skipped.
pub fn find_mp4s(folder: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4"))
        {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}

/// Never creates the selected parent, including a missing OS Downloads directory.
pub fn check_output(folder: &Path) -> io::Result<()> {
    for entry in fs::read_dir(folder)? {
        entry?;
    }
    let probe = tempfile::Builder::new()
        .prefix(".video-cue-check-")
        .tempdir_in(folder)?;
    let mut file = tempfile::NamedTempFile::new_in(probe.path())?;
    file.write_all(b"video-cue-engine output check\n")?;
    file.flush()?;
    file.close()?;
    probe.close()
}

pub fn create_batch_directory(folder: &Path) -> io::Result<PathBuf> {
    Ok(tempfile::Builder::new()
        .prefix("video-cue-engine-batch-")
        .tempdir_in(folder)?
        .keep())
}
