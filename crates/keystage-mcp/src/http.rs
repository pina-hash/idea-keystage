//! A minimal HTTP/1.1 client for `127.0.0.1` only: one request per
//! connection, `Connection: close`, a body read to its `Content-Length`.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

/// Largest answer read (a 4 MiB chunk as base64, plus room).
pub const MAX_BODY: usize = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

/// The request bytes.
pub fn request_bytes(
    method: &str,
    port: u16,
    path: &str,
    token: &str,
    body: Option<&str>,
) -> Vec<u8> {
    let mut s = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nAccept: application/json\r\nConnection: close\r\n"
    );
    match body {
        Some(b) => {
            s.push_str("Content-Type: application/json\r\n");
            s.push_str(&format!("Content-Length: {}\r\n\r\n", b.len()));
            s.push_str(b);
        }
        None => s.push_str("\r\n"),
    }
    s.into_bytes()
}

/// Reads a response.
pub fn read_response(r: impl Read) -> io::Result<Response> {
    let mut r = BufReader::new(r);
    let mut line = String::new();
    r.read_line(&mut line)?;
    let status: u16 = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no status line"))?;
    let mut len: Option<usize> = None;
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            break;
        }
        let t = line.trim_end();
        if t.is_empty() {
            break;
        }
        if let Some((k, v)) = t.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().ok();
            }
        }
    }
    let mut body = Vec::new();
    match len {
        Some(n) if n > MAX_BODY => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "answer too large",
            ));
        }
        Some(n) => {
            body.resize(n, 0);
            r.read_exact(&mut body)?;
        }
        None => {
            r.take(MAX_BODY as u64 + 1).read_to_end(&mut body)?;
            if body.len() > MAX_BODY {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "answer too large",
                ));
            }
        }
    }
    let body = String::from_utf8(body)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "not UTF-8"))?;
    Ok(Response { status, body })
}

/// One request to `127.0.0.1:port`.
pub fn send(
    method: &str,
    port: u16,
    path: &str,
    token: &str,
    body: Option<&str>,
    wait: Duration,
) -> io::Result<Response> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3))?;
    s.set_nodelay(true)?;
    s.set_read_timeout(Some(wait))?;
    s.set_write_timeout(Some(Duration::from_secs(10)))?;
    s.write_all(&request_bytes(method, port, path, token, body))?;
    s.flush()?;
    read_response(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_name_loopback_and_carry_the_token() {
        let b = String::from_utf8(request_bytes(
            "POST",
            47820,
            "/api/control/call",
            "abc",
            Some("{}"),
        ))
        .expect("u");
        assert!(b.starts_with("POST /api/control/call HTTP/1.1\r\nHost: 127.0.0.1:47820\r\n"));
        assert!(b.contains("Authorization: Bearer abc\r\n"));
        assert!(b.contains("Content-Length: 2\r\n\r\n{}"));
        assert!(!b.contains("Origin"), "never an Origin header");
        let g = String::from_utf8(request_bytes("GET", 1, "/x", "t", None)).expect("u");
        assert!(g.ends_with("\r\n\r\n"));
    }

    #[test]
    fn responses_are_read_to_their_length() {
        let raw = b"HTTP/1.1 200 OK\r\ncontent-length: 11\r\nx: y\r\n\r\n{\"ok\":true}trailing";
        let r = read_response(&raw[..]).expect("r");
        assert_eq!(
            r,
            Response {
                status: 200,
                body: "{\"ok\":true}".into()
            }
        );
        let raw = b"HTTP/1.1 401 Unauthorized\r\n\r\nno";
        assert_eq!(read_response(&raw[..]).expect("r").status, 401);
        assert!(read_response(&b"garbage"[..]).is_err());
        let big = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert!(read_response(big.as_bytes()).is_err());
    }
}
