//! Small libmpv client boundary. Handles stay on their creating worker thread.
//! Only local audio is accepted in this stage; network stations are a later mode.
pub mod player;
use libloading::Library;
use std::{
    ffi::{c_char, c_int, c_ulong, c_void, CStr, CString},
    marker::PhantomData,
    path::Path,
    rc::Rc,
};

type Handle = *mut c_void;
type Create = unsafe extern "C" fn() -> Handle;
type Initialize = unsafe extern "C" fn(Handle) -> c_int;
type SetString = unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int;
type Command = unsafe extern "C" fn(Handle, *const *const c_char) -> c_int;
type Get = unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int;
type GetString = unsafe extern "C" fn(Handle, *const c_char) -> *mut c_char;
type Wait = unsafe extern "C" fn(Handle, f64) -> *const RawEvent;
type Destroy = unsafe extern "C" fn(Handle);
type ErrorString = unsafe extern "C" fn(c_int) -> *const c_char;
type Free = unsafe extern "C" fn(*mut c_void);
#[repr(C)]
struct RawEvent {
    id: c_int,
    error: c_int,
    userdata: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    None,
    Loaded,
    Ended,
    Failed(String),
    Shutdown,
    Other,
}

pub struct Mpv {
    handle: Handle,
    set: SetString,
    command: Command,
    get: Get,
    get_string: GetString,
    wait: Wait,
    destroy: Destroy,
    error: ErrorString,
    free: Free,
    _library: Library,
    _thread: PhantomData<Rc<()>>,
}
fn string(value: &str) -> Result<CString, String> {
    CString::new(value).map_err(|_| "Audio argument contains a NUL byte".into())
}
impl Mpv {
    pub fn open(path: &Path) -> Result<Self, String> {
        // Full DLL path, with dependencies resolved from its directory/system dirs.
        #[cfg(windows)]
        let path = std::fs::canonicalize(path)
            .map_err(|e| format!("Cannot locate libmpv at {}: {e}", path.display()))?;
        #[cfg(windows)]
        let library: Library =
            unsafe { libloading::os::windows::Library::load_with_flags(&path, 0x100 | 0x1000) }
                .map_err(|e| format!("Cannot load libmpv at {}: {e}", path.display()))?
                .into();
        #[cfg(not(windows))]
        let library = unsafe { Library::new(path) }
            .map_err(|e| format!("Cannot load libmpv at {}: {e}", path.display()))?;
        unsafe {
            let api = *library
                .get::<unsafe extern "C" fn() -> c_ulong>(b"mpv_client_api_version\0")
                .map_err(|e| e.to_string())?;
            if api() >> 16 != 2 {
                return Err("This audio backend requires libmpv client API major 2".into());
            }
            let create = *library
                .get::<Create>(b"mpv_create\0")
                .map_err(|e| e.to_string())?;
            let initialize = *library
                .get::<Initialize>(b"mpv_initialize\0")
                .map_err(|e| e.to_string())?;
            let option = *library
                .get::<SetString>(b"mpv_set_option_string\0")
                .map_err(|e| e.to_string())?;
            let mut mpv = Self {
                handle: std::ptr::null_mut(),
                set: *library
                    .get::<SetString>(b"mpv_set_property_string\0")
                    .map_err(|e| e.to_string())?,
                command: *library
                    .get::<Command>(b"mpv_command\0")
                    .map_err(|e| e.to_string())?,
                get: *library
                    .get::<Get>(b"mpv_get_property\0")
                    .map_err(|e| e.to_string())?,
                get_string: *library
                    .get::<GetString>(b"mpv_get_property_string\0")
                    .map_err(|e| e.to_string())?,
                wait: *library
                    .get::<Wait>(b"mpv_wait_event\0")
                    .map_err(|e| e.to_string())?,
                destroy: *library
                    .get::<Destroy>(b"mpv_terminate_destroy\0")
                    .map_err(|e| e.to_string())?,
                error: *library
                    .get::<ErrorString>(b"mpv_error_string\0")
                    .map_err(|e| e.to_string())?,
                free: *library
                    .get::<Free>(b"mpv_free\0")
                    .map_err(|e| e.to_string())?,
                _library: library,
                _thread: PhantomData,
            };
            mpv.handle = create();
            if mpv.handle.is_null() {
                return Err("libmpv could not create a playback context".into());
            }
            for (name, value) in [
                ("config", "no"),
                ("load-scripts", "no"),
                ("ytdl", "no"),
                ("terminal", "no"),
                ("vid", "no"),
                ("audio-display", "no"),
                ("osc", "no"),
                ("input-default-bindings", "no"),
                ("input-vo-keyboard", "no"),
                ("idle", "yes"),
                ("pause", "yes"),
                ("access-references", "no"),
                ("load-unsafe-playlists", "no"),
                ("demuxer", "lavf"),
                ("demuxer-lavf-o", "protocol_whitelist=file"),
                ("network-timeout", "10"),
                ("demuxer-max-bytes", "8MiB"),
                ("demuxer-max-back-bytes", "0"),
                ("demuxer-readahead-secs", "5"),
            ] {
                mpv.check(option(
                    mpv.handle,
                    string(name)?.as_ptr(),
                    string(value)?.as_ptr(),
                ))?;
            }
            mpv.check(initialize(mpv.handle))?;
            Ok(mpv)
        }
    }
    fn check(&self, result: c_int) -> Result<(), String> {
        if result >= 0 {
            return Ok(());
        }
        let message = unsafe { CStr::from_ptr((self.error)(result)) }.to_string_lossy();
        Err(format!("libmpv: {message} ({result})"))
    }
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        let name = string(name)?;
        let value = string(value)?;
        self.check(unsafe { (self.set)(self.handle, name.as_ptr(), value.as_ptr()) })
    }
    pub fn command(&mut self, args: &[&str]) -> Result<(), String> {
        let strings = args
            .iter()
            .map(|s| string(s))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pointers: Vec<_> = strings.iter().map(|s| s.as_ptr()).collect();
        pointers.push(std::ptr::null());
        self.check(unsafe { (self.command)(self.handle, pointers.as_ptr()) })
    }
    pub fn load_local(&mut self, path: &Path) -> Result<(), String> {
        let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
        let format = match path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "mp3" => "mp3",
            "m4a" => "mov",
            "aac" => "aac",
            "ogg" => "ogg",
            "wav" => "wav",
            "flac" => "flac",
            _ => return Err("Unsupported local audio extension".into()),
        };
        // Force an audio demuxer so a disguised playlist cannot open references.
        self.set("demuxer-lavf-format", format)?;
        self.command(&["loadfile", &path.to_string_lossy(), "replace"])
    }
    pub fn number(&self, name: &str) -> Option<f64> {
        let name = string(name).ok()?;
        let mut value = 0.0f64;
        if unsafe {
            (self.get)(
                self.handle,
                name.as_ptr(),
                5,
                &mut value as *mut _ as *mut c_void,
            )
        } < 0
            || !value.is_finite()
        {
            None
        } else {
            Some(value)
        }
    }
    pub fn flag(&self, name: &str) -> Option<bool> {
        let name = string(name).ok()?;
        let mut value = 0i32;
        if unsafe {
            (self.get)(
                self.handle,
                name.as_ptr(),
                3,
                &mut value as *mut _ as *mut c_void,
            )
        } < 0
        {
            None
        } else {
            Some(value != 0)
        }
    }
    pub fn text(&self, name: &str) -> Option<String> {
        let name = string(name).ok()?;
        let value = unsafe { (self.get_string)(self.handle, name.as_ptr()) };
        if value.is_null() {
            return None;
        }
        let result = unsafe { CStr::from_ptr(value) }
            .to_string_lossy()
            .into_owned();
        unsafe { (self.free)(value as *mut c_void) };
        Some(result)
    }
    pub fn event(&mut self, timeout: f64) -> Event {
        let event = unsafe { (self.wait)(self.handle, timeout) };
        if event.is_null() {
            return Event::None;
        }
        let event = unsafe { &*event };
        match event.id {
            0 => Event::None,
            1 => Event::Shutdown,
            8 => Event::Loaded,
            7 if !event.data.is_null() => {
                let end = unsafe { &*(event.data as *const EndFile) };
                match end.reason {
                    0 => Event::Ended,
                    4 | 5 => Event::Failed(if end.error < 0 {
                        self.check(end.error).unwrap_err()
                    } else {
                        "Audio references/playlists are disabled in local playback mode".into()
                    }),
                    _ => Event::Other,
                }
            }
            _ => Event::Other,
        }
    }
}
impl Drop for Mpv {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.destroy)(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_runtime_is_an_error_not_a_process_crash() {
        assert!(Mpv::open(Path::new("/missing/libmpv-does-not-exist.dll")).is_err());
    }
    #[test]
    fn rejects_nul_arguments() {
        assert!(string("bad\0argument").is_err());
    }
}
