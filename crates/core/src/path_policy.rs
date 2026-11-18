//! Imported documents cannot authorize Windows network or device filesystem access.
use std::path::Path;

pub fn validate_import(path: &Path) -> Result<(), String> {
    let path = path.to_string_lossy().replace('/', "\\");
    let upper = path.to_ascii_uppercase();
    let ordinary_extended_drive = upper.strip_prefix("\\\\?\\").is_some_and(|tail| {
        let bytes = tail.as_bytes();
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1..3] == *b":\\"
    });
    if (path.starts_with("\\\\") && !ordinary_extended_drive)
        || upper.starts_with("\\??\\")
        || upper.starts_with("\\DEVICE\\")
        || remote_drive(&path)
    {
        return Err("Network and device locations are not accepted from imported documents. Use Add folder to select a network library explicitly.".into());
    }
    Ok(())
}

#[cfg(windows)]
fn remote_drive(path: &str) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    let path = path.strip_prefix("\\\\?\\").unwrap_or(path).as_bytes();
    if path.len() < 2 || !path[0].is_ascii_alphabetic() || path[1] != b':' {
        return false;
    }
    let root = [path[0] as u16, b':' as u16, b'\\' as u16, 0];
    // DRIVE_REMOTE identifies mapped network drives without opening a media file.
    unsafe { GetDriveTypeW(root.as_ptr()) == 4 }
}
#[cfg(not(windows))]
fn remote_drive(_: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_windows_network_and_device_names_before_filesystem_access() {
        for value in [
            r"\\host\share\song.mp3",
            "//host/share/song.mp3",
            r"\\?\UNC\host\share\song.mp3",
            r"\\.\C:\song.mp3",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\song.mp3",
            r"\??\UNC\host\share\song.mp3",
            r"\Device\Mup\host\share\song.mp3",
        ] {
            assert!(validate_import(Path::new(value)).is_err(), "{value}");
        }
        for value in ["songs/song.mp3", "/music/song.mp3"] {
            assert!(validate_import(Path::new(value)).is_ok(), "{value}");
        }
    }
}
