//! Bounded HTTP/1 request headers and immutable responses for the boot server.
//!
//! One request is accepted per connection. Bodies and transfer codings are
//! rejected; bytes following the first complete header are never another request.
use core::fmt::{self, Write};

pub const MAX_REQUEST: usize = 1024;
pub const PAGE: &[u8] = include_bytes!("../../../assets/index.html");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Ok,
    BadRequest,
    NotFound,
    MethodNotAllowed,
    Timeout,
    TooLarge,
    VersionNotSupported,
}

impl Status {
    fn reason(self) -> &'static str {
        match self {
            Self::Ok => "200 OK",
            Self::BadRequest => "400 Bad Request",
            Self::NotFound => "404 Not Found",
            Self::MethodNotAllowed => "405 Method Not Allowed",
            Self::Timeout => "408 Request Timeout",
            Self::TooLarge => "431 Request Header Fields Too Large",
            Self::VersionNotSupported => "505 HTTP Version Not Supported",
        }
    }

    fn body(self) -> &'static [u8] {
        match self {
            Self::Ok => PAGE,
            Self::BadRequest => b"Bad request\n",
            Self::NotFound => b"Not found\n",
            Self::MethodNotAllowed => b"Use GET or HEAD\n",
            Self::Timeout => b"Request timeout\n",
            Self::TooLarge => b"Request headers too large\n",
            Self::VersionNotSupported => b"Use HTTP/1.0 or HTTP/1.1\n",
        }
    }
}

/// Incremental request storage. Completion is sticky until the connection resets.
pub struct Request {
    bytes: [u8; MAX_REQUEST],
    length: usize,
    result: Option<(Status, bool)>,
}

impl Default for Request {
    fn default() -> Self {
        Self::new()
    }
}

impl Request {
    pub const fn new() -> Self {
        Self {
            bytes: [0; MAX_REQUEST],
            length: 0,
            result: None,
        }
    }

    /// Returns a response once headers complete or their size limit is reached.
    /// Extra input is ignored because the server always closes the connection.
    pub fn push(&mut self, input: &[u8]) -> Option<Response> {
        for &byte in input {
            if self.result.is_some() {
                break;
            }
            self.bytes[self.length] = byte;
            self.length += 1;
            if self.bytes[..self.length].ends_with(b"\r\n\r\n") {
                self.result = Some(parse(&self.bytes[..self.length]));
            } else if self.length == MAX_REQUEST {
                self.result = Some((Status::TooLarge, false));
            }
        }
        self.result
            .map(|(status, head)| Response::new(status, head))
    }
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

fn parse(input: &[u8]) -> (Status, bool) {
    let Ok(text) = core::str::from_utf8(input) else {
        return (Status::BadRequest, false);
    };
    let mut lines = text.split("\r\n");
    let mut words = lines.next().unwrap_or("").split(' ');
    let method = words.next().unwrap_or("");
    let target = words.next().unwrap_or("");
    let version = words.next().unwrap_or("");
    let head = method == "HEAD";
    if words.next().is_some()
        || !token(method)
        || !target.starts_with('/')
        || !target.bytes().all(|b| (0x21..=0x7e).contains(&b))
    {
        return (Status::BadRequest, head);
    }
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return (Status::VersionNotSupported, head);
    }
    let mut host = false;
    let mut length = false;
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return (Status::BadRequest, head);
        };
        if !token(name)
            || !value
                .bytes()
                .all(|b| b == b'\t' || (0x20..=0x7e).contains(&b))
        {
            return (Status::BadRequest, head);
        }
        let value = value.trim_matches([' ', '\t']);
        if name.eq_ignore_ascii_case("host") {
            if host || value.is_empty() || value.bytes().any(|b| b.is_ascii_whitespace()) {
                return (Status::BadRequest, head);
            }
            host = true;
        } else if name.eq_ignore_ascii_case("content-length") {
            if length
                || value.is_empty()
                || !value.bytes().all(|b| b.is_ascii_digit())
                || value.parse::<usize>() != Ok(0)
            {
                return (Status::BadRequest, head);
            }
            length = true;
        } else if name.eq_ignore_ascii_case("transfer-encoding")
            || name.eq_ignore_ascii_case("expect")
        {
            return (Status::BadRequest, head);
        }
    }
    if version == "HTTP/1.1" && !host {
        return (Status::BadRequest, head);
    }
    if !matches!(method, "GET" | "HEAD") {
        return (Status::MethodNotAllowed, head);
    }
    if target != "/" {
        return (Status::NotFound, head);
    }
    (Status::Ok, head)
}

/// A header and static body. `chunk` supports arbitrarily short TCP sends.
pub struct Response {
    pub status: Status,
    header: [u8; 256],
    header_length: usize,
    body: &'static [u8],
}

struct Header<'a> {
    bytes: &'a mut [u8],
    length: usize,
}
impl Write for Header<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.length.checked_add(text.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.length..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.length = end;
        Ok(())
    }
}

impl Response {
    pub fn new(status: Status, head: bool) -> Self {
        let mut response = Self {
            status,
            header: [0; 256],
            header_length: 0,
            body: if head { b"" } else { status.body() },
        };
        let mut header = Header {
            bytes: &mut response.header,
            length: 0,
        };
        // All interpolated strings are constants and fit the fixed header buffer.
        write!(
            header,
            "HTTP/1.1 {}\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n{}\r\n",
            status.reason(),
            status.body().len(),
            if status == Status::Ok {
                "text/html; charset=utf-8"
            } else {
                "text/plain; charset=utf-8"
            },
            if status == Status::MethodNotAllowed {
                "Allow: GET, HEAD\r\n"
            } else {
                ""
            }
        )
        .expect("constant HTTP headers fit");
        response.header_length = header.length;
        response
    }

    pub fn len(&self) -> usize {
        self.header_length + self.body.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the next contiguous part, or an empty slice after the response.
    pub fn chunk(&self, position: usize) -> &[u8] {
        if position < self.header_length {
            &self.header[position..self.header_length]
        } else {
            self.body
                .get(position - self.header_length..)
                .unwrap_or(&[])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(bytes: &[u8]) -> Status {
        Request::new().push(bytes).unwrap().status
    }

    #[test]
    fn incremental_request_and_pipeline_boundary() {
        let mut request = Request::new();
        for byte in b"GET / HTTP/1.1\r\nHost: kernel\r\n\r" {
            assert!(request.push(&[*byte]).is_none());
        }
        assert_eq!(
            request
                .push(b"\nGET /missing HTTP/1.0\r\n\r\n")
                .unwrap()
                .status,
            Status::Ok
        );
        assert_eq!(request.push(b"garbage").unwrap().status, Status::Ok);
    }

    #[test]
    fn routes_methods_versions_and_host() {
        for (bytes, expected) in [
            (&b"GET / HTTP/1.0\r\n\r\n"[..], Status::Ok),
            (b"HEAD / HTTP/1.1\r\nhOsT: kernel\r\n\r\n", Status::Ok),
            (b"GET /missing HTTP/1.0\r\n\r\n", Status::NotFound),
            (b"POST / HTTP/1.0\r\n\r\n", Status::MethodNotAllowed),
            (b"GET / HTTP/2.0\r\n\r\n", Status::VersionNotSupported),
            (b"GET / HTTP/1.1\r\n\r\n", Status::BadRequest),
        ] {
            assert_eq!(status(bytes), expected);
        }
    }

    #[test]
    fn ambiguous_and_malformed_framing_is_rejected() {
        for bytes in [
            &b"GET / HTTP/1.1\r\nHost: a\r\nHost: b\r\n\r\n"[..],
            b"GET / HTTP/1.1\r\nHost:\r\n\r\n",
            b"GET / HTTP/1.0\r\nContent-Length: 1\r\n\r\nx",
            b"GET / HTTP/1.0\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n",
            b"GET / HTTP/1.0\r\nContent-Length: +0\r\n\r\n",
            b"GET / HTTP/1.0\r\nContent-Length: 999999999999999999999999999999\r\n\r\n",
            b"GET / HTTP/1.0\r\nTransfer-Encoding: chunked\r\n\r\n",
            b"GET / HTTP/1.0\r\nExpect: 100-continue\r\n\r\n",
            b"GET / HTTP/1.0\r\n Host: a\r\n\r\n",
            b"GET / HTTP/1.0\r\nHost : a\r\n\r\n",
            b"GET / HTTP/1.0\r\nX: a\nb\r\n\r\n",
            b"GET / HTTP/1.0\r\nX: \0\r\n\r\n",
            b"GET / HTTP/1.0\r\nX: \xff\r\n\r\n",
            b"GET  / HTTP/1.0\r\n\r\n",
        ] {
            assert_eq!(status(bytes), Status::BadRequest, "{bytes:?}");
        }
        assert_eq!(
            status(b"GET / HTTP/1.0\r\nContent-Length: 0\r\n\r\n"),
            Status::Ok
        );
    }

    #[test]
    fn exact_header_bound_and_overflow() {
        let mut bytes = [b'a'; MAX_REQUEST];
        let prefix = b"GET / HTTP/1.0\r\nX: ";
        bytes[..prefix.len()].copy_from_slice(prefix);
        bytes[MAX_REQUEST - 4..].copy_from_slice(b"\r\n\r\n");
        assert_eq!(status(&bytes), Status::Ok);
        bytes[MAX_REQUEST - 4..].fill(b'a');
        let mut request = Request::new();
        assert_eq!(request.push(&bytes).unwrap().status, Status::TooLarge);
        assert_eq!(request.push(b"extra").unwrap().status, Status::TooLarge);
    }

    #[test]
    fn response_framing_and_short_writes() {
        for status in [
            Status::Ok,
            Status::BadRequest,
            Status::NotFound,
            Status::MethodNotAllowed,
            Status::Timeout,
            Status::TooLarge,
            Status::VersionNotSupported,
        ] {
            let response = Response::new(status, false);
            let header = core::str::from_utf8(response.chunk(0)).unwrap();
            assert!(header.contains("Connection: close\r\n"));
            let mut expected = [0; 32];
            let mut length = Header {
                bytes: &mut expected,
                length: 0,
            };
            write!(length, "Content-Length: {}\r\n", status.body().len()).unwrap();
            assert!(header.contains(core::str::from_utf8(&length.bytes[..length.length]).unwrap()));
            let mut position = 0;
            while position < response.len() {
                let chunk = response.chunk(position);
                assert!(!chunk.is_empty());
                position += chunk.len().min(7);
            }
            assert!(response.chunk(position).is_empty());
            let head = Response::new(status, true);
            assert_eq!(head.len(), head.header_length);
            assert_eq!(head.chunk(0), response.chunk(0));
        }
        assert!(
            PAGE.len() > 1460,
            "integration page exercises segmented responses"
        );
    }
}
