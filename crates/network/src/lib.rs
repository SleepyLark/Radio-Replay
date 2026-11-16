//! HTTP audio policy. Parsing/importing a URL performs no network access.
use std::net::{IpAddr, SocketAddr};
use url::{Host, Url};
#[cfg(feature = "transport")]
pub mod transport;

pub fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !matches!(a, 0 | 10 | 127 | 224..=255)
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 169 && b == 254)
                && !(a == 172 && (16..=31).contains(&b))
                && !(a == 192 && (b == 168 || b == 0 && (c == 0 || c == 2) || b == 88 && c == 99))
                && !(a == 198 && (b == 18 || b == 19 || b == 51 && c == 100))
                && !(a == 203 && b == 0 && c == 113)
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            // Public global unicast only; block transition/special/documentation ranges.
            s[0] & 0xe000 == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && !(s[0] == 0x3fff && s[1] & 0xf000 == 0)
        }
    }
}
pub fn validate(value: &str) -> Result<Url, String> {
    if value.len() > 8192
        || value.chars().any(|c| c.is_control() || c.is_whitespace())
        || value.contains('\\')
    {
        return Err("Stream URL contains invalid characters or is too long".into());
    }
    let url = Url::parse(value).map_err(|_| "Invalid stream URL")?;
    if url.as_str().len() > 8192 {
        return Err("Encoded stream URL is too long".into());
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Only HTTP and HTTPS audio streams are supported".into());
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port() == Some(0)
    {
        return Err("URL credentials, fragments and port zero are not supported".into());
    }
    match url.host().ok_or("Stream URL needs a host")? {
        Host::Ipv4(ip) if !public_ip(ip.into()) => {
            return Err("Private, loopback and special-purpose destinations are blocked".into())
        }
        Host::Ipv6(ip) if !public_ip(ip.into()) => {
            return Err("Private, loopback and special-purpose destinations are blocked".into())
        }
        Host::Domain(host) => {
            let host = host.trim_end_matches('.').to_ascii_lowercase();
            if !host.contains('.')
                || [
                    "localhost",
                    "local",
                    "internal",
                    "lan",
                    "home",
                    "test",
                    "invalid",
                    "example",
                ]
                .iter()
                .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
            {
                return Err("A public Internet hostname is required".into());
            }
        }
        _ => {}
    }
    Ok(url)
}
pub fn validate_addresses(addresses: &[SocketAddr]) -> Result<(), String> {
    if addresses.is_empty() || addresses.len() > 64 || addresses.iter().any(|a| !public_ip(a.ip()))
    {
        return Err("DNS must resolve exclusively to public Internet addresses".into());
    }
    Ok(())
}
pub fn redirect(from: &Url, location: &str) -> Result<Url, String> {
    let to = from.join(location).map_err(|_| "Invalid stream redirect")?;
    let to = validate(to.as_str())?;
    if from.scheme() == "https" && to.scheme() != "https" {
        return Err("HTTPS-to-HTTP redirects are blocked".into());
    }
    Ok(to)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_nonpublic_and_ambiguous_urls() {
        for value in [
            "file:///C:/private.mp3",
            "ftp://radio.example.org/a",
            "http://127.1/",
            "http://2130706433/",
            "http://0x7f000001/",
            "http://[::1]/",
            "http://[::ffff:8.8.8.8]/",
            "http://169.254.169.254/",
            "http://192.168.1.1/",
            "http://10.0.0.1/",
            "http://100.100.100.200/",
            "http://localhost./",
            "http://printer.local/",
            "http://radio/",
            "http://user:secret@radio.example.org/",
            "http://radio.example.org/#x",
            "http://radio.example.org:0/",
            "http://radio.example.org/a\r\nheader",
            "http://radio.example.org\\@127.0.0.1/",
        ] {
            assert!(validate(value).is_err(), "{value}");
        }
        assert!(validate("https://radio.example.org:8443/live?token=value").is_ok());
        assert!(validate("http://8.8.8.8/audio").is_ok());
    }
    #[test]
    fn checks_all_dns_answers_and_redirects() {
        let public: SocketAddr = "8.8.8.8:443".parse().unwrap();
        assert!(validate_addresses(&[public]).is_ok());
        assert!(validate_addresses(&[public, "127.0.0.1:443".parse().unwrap()]).is_err());
        let url = validate("https://radio.example.org/live").unwrap();
        for target in [
            "http://radio.example.org/live",
            "https://192.168.0.1/",
            "file:///etc/passwd",
        ] {
            assert!(redirect(&url, target).is_err());
        }
        assert_eq!(redirect(&url, "/other").unwrap().path(), "/other");
        for ip in [
            "0.0.0.0",
            "192.0.2.1",
            "198.18.0.1",
            "224.1.1.1",
            "::",
            "fc00::1",
            "fe80::1",
            "64:ff9b::a00:1",
            "2002:7f00:1::",
            "2001:db8::1",
            "3fff::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    }
}
