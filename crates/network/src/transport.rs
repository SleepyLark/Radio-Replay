//! A bounded cancellable byte stream; all HTTP connections are DNS-pinned.
use reqwest::{Client, Response};
use std::{
    io::{self, Read},
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc, Mutex,
    },
    time::Duration,
};
use url::Url;
const WAIT: Duration = Duration::from_millis(25);
const READ_TIMEOUT: Duration = Duration::from_secs(10);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
struct Permit;
impl Drop for Permit {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::AcqRel);
    }
}
enum Packet {
    Data(Vec<u8>),
    End,
    Error,
}
pub struct Stream {
    rx: Receiver<Packet>,
    pending: Vec<u8>,
    offset: usize,
    pub cancelled: Arc<AtomicBool>,
}
impl Stream {
    pub fn start(url: String, error: Arc<Mutex<String>>) -> Result<Self, String> {
        super::validate(&url)?;
        let mut count = ACTIVE.load(Ordering::Acquire);
        loop {
            if count >= 4 {
                return Err(
                    "Network reader limit reached; retry after previous connections close".into(),
                );
            }
            match ACTIVE.compare_exchange_weak(
                count,
                count + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(n) => count = n,
            }
        }

        let permit = Permit;
        let (tx, rx) = mpsc::sync_channel(4);
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        std::thread::Builder::new().name("radio-http".into()).spawn(move||{
            let _permit=permit;
            let result=tokio::runtime::Builder::new_current_thread().enable_all().build()
                .map_err(|_|"Could not start network runtime".to_string()).and_then(|runtime|runtime.block_on(async{
                    tokio::select!{ result=transfer(url,&tx,&signal)=>result,_=cancelled_wait(&signal)=>Ok(()) }
                }));
            if let Err(reason)=result {if let Ok(mut slot)=error.lock(){*slot=reason;}send(&tx,Packet::Error,&signal);}else{send(&tx,Packet::End,&signal);}
        }).map_err(|_|"Could not start network reader")?;
        Ok(Self {
            rx,
            pending: Vec::new(),
            offset: 0,
            cancelled,
        })
    }
}
impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Stream cancelled",
                ));
            }
            if self.offset < self.pending.len() {
                let size = buf.len().min(self.pending.len() - self.offset);
                buf[..size].copy_from_slice(&self.pending[self.offset..self.offset + size]);
                self.offset += size;
                return Ok(size);
            }
            match self.rx.recv_timeout(WAIT) {
                Ok(Packet::Data(bytes)) => {
                    self.pending = bytes;
                    self.offset = 0;
                }
                Ok(Packet::End) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
                Ok(Packet::Error) => return Err(io::Error::other("Network audio failed")),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
fn send(tx: &SyncSender<Packet>, mut packet: Packet, cancel: &AtomicBool) {
    loop {
        if cancel.load(Ordering::Acquire) {
            return;
        }
        match tx.try_send(packet) {
            Ok(()) => return,
            Err(TrySendError::Disconnected(_)) => return,
            Err(TrySendError::Full(back)) => {
                packet = back;
                std::thread::sleep(WAIT);
            }
        }
    }
}
async fn cancelled_wait(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Acquire) {
        tokio::time::sleep(WAIT).await;
    }
}
async fn response(url: &Url) -> Result<Response, String> {
    let host = url.host_str().ok_or("Missing stream host")?;
    let port = url.port_or_known_default().ok_or("Missing stream port")?;
    let addresses: Vec<SocketAddr> = if let Some(ip) = url.host().and_then(|host| match host {
        url::Host::Ipv4(ip) => Some(ip.into()),
        url::Host::Ipv6(ip) => Some(ip.into()),
        _ => None,
    }) {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::time::timeout(
            Duration::from_secs(5),
            tokio::net::lookup_host((host, port)),
        )
        .await
        .map_err(|_| "Stream DNS timed out")?
        .map_err(|_| "Stream DNS lookup failed")?
        .take(65)
        .collect()
    };
    super::validate_addresses(&addresses)?;
    pinned_response(url, &addresses).await
}
async fn pinned_response(url: &Url, addresses: &[SocketAddr]) -> Result<Response, String> {
    let host = url.host_str().ok_or("Missing host")?;
    let client = Client::builder()
        .use_rustls_tls()
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .referer(false)
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(READ_TIMEOUT)
        .http1_only()
        .pool_max_idle_per_host(0)
        .resolve_to_addrs(host, addresses)
        .user_agent("RadioReplay/0.6.6")
        .build()
        .map_err(|_| "Could not initialize secure HTTP transport")?;
    client
        .get(url.clone())
        .header(
            "Accept",
            "audio/*,application/ogg,application/octet-stream;q=0.5",
        )
        .header("Icy-MetaData", "0")
        .header("Accept-Encoding", "identity")
        .send()
        .await
        .map_err(|_| "Stream connection failed (network, TLS, or timeout)")
        .map_err(String::from)
}
async fn transfer(
    value: String,
    tx: &SyncSender<Packet>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let mut url = super::validate(&value)?;
    let mut body = None;
    for hop in 0..=4 {
        let result = response(&url).await?;
        if result.status().is_redirection() {
            if hop == 4 {
                return Err("Stream exceeded the redirect limit".into());
            }
            let location = result
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("Invalid stream redirect")?;
            url = super::redirect(&url, location)?;
            continue;
        }
        if !result.status().is_success() {
            return Err(format!("Stream returned HTTP {}", result.status().as_u16()));
        }
        if result
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .is_some_and(|v| {
                v.to_str()
                    .map(|s| !s.eq_ignore_ascii_case("identity"))
                    .unwrap_or(true)
            })
        {
            return Err(
                "HTTP content compression is unsupported; server must send direct audio bytes"
                    .into(),
            );
        }
        let mime = result
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !mime.is_empty()
            && !mime.starts_with("audio/")
            && !matches!(
                mime.as_str(),
                "application/ogg" | "application/octet-stream"
            )
        {
            return Err("Server did not return direct audio; webpages, nested playlists and HLS are unsupported".into());
        }
        if matches!(mime.as_str(), "audio/x-mpegurl" | "audio/mpegurl") {
            return Err("Nested network playlists are not supported".into());
        }
        if result.headers().contains_key("icy-metaint") {
            return Err(
                "Server requires interleaved ICY metadata, which is not supported in this preview"
                    .into(),
            );
        }
        body = Some(result);
        break;
    }
    let mut body = body.ok_or("No stream response")?;
    while let Some(bytes) = body
        .chunk()
        .await
        .map_err(|_| "Stream stalled or disconnected")?
    {
        if cancel.load(Ordering::Acquire) {
            return Ok(());
        }
        for chunk in bytes.chunks(32 * 1024) {
            send(tx, Packet::Data(chunk.to_vec()), cancel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };
    fn server(
        status: &str,
        headers: &str,
        body: &[u8],
    ) -> (SocketAddr, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let reply = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",
            body.len()
        );
        let body = body.to_vec();
        let thread = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") && request.len() < 8192 {
                socket.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            socket.write_all(reply.as_bytes()).unwrap();
            socket.write_all(&body).unwrap();
            String::from_utf8(request).unwrap()
        });
        (addr, thread)
    }
    fn run(future: impl std::future::Future<Output = ()>) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future);
    }
    #[test]
    fn pinned_http_has_no_ambient_credentials_and_does_not_follow_redirects() {
        run(async {
            let (addr, server) = server(
                "302 Found",
                "Location: http://127.0.0.1:1/private\r\nSet-Cookie: secret=value\r\n",
                b"",
            );
            let url =
                Url::parse(&format!("http://radio.example.org:{}/audio", addr.port())).unwrap();
            let response = pinned_response(&url, &[addr]).await.unwrap();
            assert_eq!(response.status(), 302);
            let location = response.headers()["location"].to_str().unwrap();
            assert!(super::super::redirect(&url, location).is_err());
            let request = server.join().unwrap().to_ascii_lowercase();
            for header in [
                "authorization:",
                "cookie:",
                "referer:",
                "proxy-authorization:",
            ] {
                assert!(!request.contains(header));
            }
            assert!(request.contains("icy-metadata: 0"));
        });
    }
    #[test]
    fn body_reads_are_incremental_and_cancel_unblocks_reader() {
        run(async {
            let (addr, server) = server("200 OK", "Content-Type: audio/mpeg\r\n", b"audio fixture");
            let url =
                Url::parse(&format!("http://radio.example.org:{}/audio", addr.port())).unwrap();
            let mut response = pinned_response(&url, &[addr]).await.unwrap();
            assert_eq!(
                response.chunk().await.unwrap().unwrap(),
                b"audio fixture".as_slice()
            );
            server.join().unwrap();
            let (_tx, rx) = mpsc::sync_channel(4);
            let cancelled = Arc::new(AtomicBool::new(false));
            let signal = cancelled.clone();
            let mut stream = Stream {
                rx,
                pending: Vec::new(),
                offset: 0,
                cancelled,
            };
            let handle = std::thread::spawn(move || {
                let mut out = [0; 1];
                stream.read(&mut out)
            });
            std::thread::sleep(Duration::from_millis(40));
            signal.store(true, Ordering::Release);
            assert!(handle.join().unwrap().is_err());
        });
    }
}
