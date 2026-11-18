//! The OS owns the lock lifetime; the loopback channel only requests window focus.
use std::{
    fs::{File, OpenOptions, TryLockError},
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::Path,
    time::Duration,
};

pub struct Instance {
    _lock: File,
}

pub fn acquire(
    directory: &Path,
    activate: impl Fn() + Send + 'static,
) -> Result<Option<Instance>, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("instance.lock"))
        .map_err(|e| e.to_string())?;
    let endpoint = directory.join("instance.port");
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            // The first process may still be writing its endpoint during startup.
            for _ in 0..20 {
                let mut value = String::new();
                let port = File::open(&endpoint)
                    .and_then(|f| f.take(16).read_to_string(&mut value))
                    .ok()
                    .and_then(|_| value.trim().parse::<u16>().ok())
                    .filter(|port| *port != 0);
                if let Some(port) = port {
                    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
                    if TcpStream::connect_timeout(&address, Duration::from_millis(100))
                        .and_then(|mut stream| {
                            stream.set_write_timeout(Some(Duration::from_millis(100)))?;
                            stream.write_all(b"activate")
                        })
                        .is_ok()
                    {
                        return Ok(None);
                    }
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            return Err(
                "Radio Replay is already running, but its window could not be activated.".into(),
            );
        }
        Err(TryLockError::Error(error)) => {
            return Err(format!(
                "Cannot lock the application data directory: {error}"
            ))
        }
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    std::fs::write(
        endpoint,
        listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .port()
            .to_string(),
    )
    .map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("radio-instance".into())
        .spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                let mut request = [0; 8];
                if stream.read_exact(&mut request).is_ok() && &request == b"activate" {
                    activate();
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Some(Instance { _lock: file }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secondary_process() {
        let Some(directory) = std::env::var_os("RADIO_REPLAY_INSTANCE_TEST_DIR") else {
            return;
        };
        assert!(acquire(Path::new(&directory), || {}).unwrap().is_none());
    }
    #[test]
    fn second_process_activates_the_owner_and_the_lock_releases() {
        let directory = std::env::temp_dir().join(format!(
            "radio-instance-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (send, receive) = std::sync::mpsc::channel();
        let instance = acquire(&directory, move || {
            let _ = send.send(());
        })
        .unwrap()
        .unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "single_instance::tests::secondary_process"])
            .env("RADIO_REPLAY_INSTANCE_TEST_DIR", &directory)
            .status()
            .unwrap();
        assert!(status.success());
        receive.recv_timeout(Duration::from_secs(2)).unwrap();
        drop(instance);
        let replacement = acquire(&directory, || {}).unwrap().unwrap();
        drop(replacement);
        assert_eq!(directory.parent(), Some(std::env::temp_dir().as_path()));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
