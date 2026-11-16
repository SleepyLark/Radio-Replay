//! The stream cookie outlives every callback; libmpv closes it before its owner is dropped.
use radio_replay_network::transport::Stream;
use std::{
    cell::UnsafeCell,
    ffi::{c_char, c_int, c_void, CStr},
    io::Read,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
struct Entry {
    key: String,
    url: String,
    error: Arc<Mutex<String>>,
}
#[derive(Default)]
pub(crate) struct Slot {
    active: Mutex<Option<Entry>>,
    counter: AtomicU64,
    #[cfg(test)]
    pub fixture: Mutex<Option<Vec<u8>>>,
}
impl Slot {
    pub fn error(&self) -> String {
        let error = self
            .active
            .lock()
            .ok()
            .and_then(|active| active.as_ref().map(|entry| entry.error.clone()));
        error
            .and_then(|error| error.lock().ok().map(|s| s.clone()))
            .unwrap_or_default()
    }
    pub fn prepare(&self, url: &str) -> Result<String, String> {
        radio_replay_network::validate(url)?;
        let key = format!("rrnet://{}", self.counter.fetch_add(1, Ordering::Relaxed));
        *self
            .active
            .lock()
            .map_err(|_| "Network state unavailable")? = Some(Entry {
            key: key.clone(),
            url: url.into(),
            error: Arc::new(Mutex::new(String::new())),
        });
        Ok(key)
    }
}
#[repr(C)]
pub(crate) struct Info {
    cookie: *mut c_void,
    read: Option<unsafe extern "C" fn(*mut c_void, *mut c_char, u64) -> i64>,
    seek: Option<unsafe extern "C" fn(*mut c_void, i64) -> i64>,
    size: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    close: Option<unsafe extern "C" fn(*mut c_void)>,
    cancel: Option<unsafe extern "C" fn(*mut c_void)>,
}
struct Cookie {
    reader: UnsafeCell<Box<dyn Read + Send>>,
    cancelled: Arc<AtomicBool>,
}
// libmpv serializes read calls and calls close only after other callbacks end.
// cancel may run concurrently and only accesses the separate atomic flag.
unsafe extern "C" fn read(cookie: *mut c_void, buf: *mut c_char, size: u64) -> i64 {
    if size == 0 {
        return 0;
    }
    if cookie.is_null() || buf.is_null() {
        return -1;
    }
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let cookie = cookie.cast::<Cookie>();
        let reader = &mut *(*cookie).reader.get();
        let buf = std::slice::from_raw_parts_mut(buf.cast::<u8>(), size.min(65536) as usize);
        reader.read(buf).map(|n| n as i64).unwrap_or(-1)
    }))
    .unwrap_or(-1)
}
unsafe extern "C" fn close(cookie: *mut c_void) {
    drop(Box::from_raw(cookie.cast::<Cookie>()));
}
unsafe extern "C" fn cancel(cookie: *mut c_void) {
    (*cookie.cast::<Cookie>())
        .cancelled
        .store(true, Ordering::Release);
}
pub(crate) unsafe extern "C" fn open(
    data: *mut c_void,
    uri: *mut c_char,
    info: *mut Info,
) -> c_int {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<(), String> {
        let slot = &*data.cast::<Slot>();
        let uri = CStr::from_ptr(uri)
            .to_str()
            .map_err(|_| "Invalid stream handle")?;
        let (url, error) = slot
            .active
            .lock()
            .map_err(|_| "Network state unavailable")?
            .as_ref()
            .filter(|entry| entry.key == uri)
            .map(|entry| (entry.url.clone(), entry.error.clone()))
            .ok_or("Unknown stream handle")?;
        #[cfg(test)]
        let fixture = slot
            .fixture
            .lock()
            .map_err(|_| "Fixture lock failed")?
            .clone();
        #[cfg(not(test))]
        let fixture: Option<Vec<u8>> = None;
        let (reader, cancelled): (Box<dyn Read + Send>, Arc<AtomicBool>) =
            if let Some(bytes) = fixture {
                (
                    Box::new(std::io::Cursor::new(bytes)),
                    Arc::new(AtomicBool::new(false)),
                )
            } else {
                let reader = Stream::start(url, error)?;
                let cancelled = reader.cancelled.clone();
                (Box::new(reader), cancelled)
            };
        let cookie = Box::into_raw(Box::new(Cookie {
            reader: UnsafeCell::new(reader),
            cancelled,
        }));
        *info = Info {
            cookie: cookie.cast(),
            read: Some(read),
            seek: None,
            size: None,
            close: Some(close),
            cancel: Some(cancel),
        };
        Ok(())
    }))
    .map(|r| if r.is_ok() { 0 } else { -13 })
    .unwrap_or(-13)
}
