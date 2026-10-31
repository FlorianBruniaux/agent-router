// SPDX-FileCopyrightText: 2026 Florian Bruniaux
// SPDX-License-Identifier: Apache-2.0
//! Minimal HTTP/1.1 client over one loopback TCP connection. Single thread,
//! blocking sockets with short timeouts, so a deadline or a cancellation is
//! observed between reads and nothing outlives the call.
use super::{CancelToken, TransportError};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

pub const MAX_HEADER_BYTES: usize = 16 * 1024;
pub const MAX_RETRY_AFTER_SECS: u64 = 3600;
/// Upper bound of one blocking socket call, so cancellation stays responsive.
const POLL: Duration = Duration::from_millis(25);
const MAX_CHUNK_LINE: usize = 32;
/// Size line of 8 hex digits plus the two CRLF around a chunk.
const MAX_CHUNK_OVERHEAD: usize = 8 + 2 + 2;

pub struct Session<'a> {
    stream: TcpStream,
    deadline: Instant,
    cancel: &'a CancelToken,
    /// True once one response byte was read. A socket error before that is
    /// `Io`, after it the response was cut (`Truncated`).
    received: bool,
}

pub struct Head {
    pub status: u16,
    headers: Vec<(String, String)>,
}

pub enum BodyFraming {
    Length(usize),
    Chunked,
    /// No `Content-Length` and no chunking: the body ends when the server closes
    /// the connection, which is correct HTTP/1.1 for such a response and the
    /// request asks for it with `Connection: close`. A server that keeps the
    /// connection open after a complete stream is stopped by the deadline
    /// (`Timeout`), or by `Limit` if it keeps sending. Nothing partial is
    /// returned in either case.
    UntilClose,
}

impl Head {
    fn single(&self, name: &str) -> Result<Option<&str>, TransportError> {
        let mut found = self.headers.iter().filter(|(n, _)| n == name);
        let first = found.next();
        if found.next().is_some() {
            return Err(TransportError::Malformed);
        }
        Ok(first.map(|(_, v)| v.as_str()))
    }

    /// Only an all-digit value is a delay. Anything else, including an HTTP
    /// date, a sign or a repeated header, is dropped.
    pub fn retry_after(&self) -> Option<u64> {
        let value = self.single("retry-after").ok()??;
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(
            value
                .parse::<u64>()
                .map_or(MAX_RETRY_AFTER_SECS, |v| v.min(MAX_RETRY_AFTER_SECS)),
        )
    }

    /// Validates a 200 response head and returns how the body is framed.
    pub fn success_framing(&self, limit: usize) -> Result<BodyFraming, TransportError> {
        if self.single("content-encoding")?.is_some() {
            return Err(TransportError::Malformed);
        }
        let content_type = self
            .single("content-type")?
            .ok_or(TransportError::Malformed)?;
        // Media types are case-insensitive (RFC 9110 8.3.1).
        if !content_type
            .split(';')
            .next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("text/event-stream"))
        {
            return Err(TransportError::Malformed);
        }
        let length = self.single("content-length")?;
        let encoding = self.single("transfer-encoding")?;
        match (length, encoding) {
            (Some(_), Some(_)) => Err(TransportError::Malformed),
            (Some(length), None) => {
                if length.is_empty() || !length.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(TransportError::Malformed);
                }
                match length.parse::<usize>() {
                    Ok(n) if n <= limit => Ok(BodyFraming::Length(n)),
                    _ => Err(TransportError::Limit),
                }
            }
            (None, Some(value)) if value.eq_ignore_ascii_case("chunked") => {
                Ok(BodyFraming::Chunked)
            }
            (None, Some(_)) => Err(TransportError::Malformed),
            (None, None) => Ok(BodyFraming::UntilClose),
        }
    }
}

impl<'a> Session<'a> {
    pub fn connect(
        addr: SocketAddr,
        deadline: Instant,
        cancel: &'a CancelToken,
    ) -> Result<Self, TransportError> {
        Ok(Self {
            stream: connect(addr, deadline, cancel)?,
            deadline,
            cancel,
            received: false,
        })
    }

    fn budget(&self) -> Result<Duration, TransportError> {
        if self.cancel.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or(TransportError::Timeout)?;
        Ok(remaining.min(POLL))
    }

    pub fn write_all(&mut self, mut bytes: &[u8]) -> Result<(), TransportError> {
        while !bytes.is_empty() {
            let wait = self.budget()?;
            self.stream
                .set_write_timeout(Some(wait))
                .map_err(|_| TransportError::Io)?;
            match self.stream.write(bytes) {
                Ok(0) => return Err(TransportError::Io),
                Ok(n) => bytes = &bytes[n..],
                Err(e) if retryable(&e) => {}
                Err(_) => return Err(TransportError::Io),
            }
        }
        Ok(())
    }

    /// Returns 0 only at end of stream.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, TransportError> {
        loop {
            let wait = self.budget()?;
            self.stream
                .set_read_timeout(Some(wait))
                .map_err(|_| TransportError::Io)?;
            match self.stream.read(buf) {
                Ok(n) => {
                    self.received |= n > 0;
                    return Ok(n);
                }
                Err(e) if retryable(&e) => {}
                // A reset after part of the response was read is a cut
                // response. Before any byte it is a socket failure.
                Err(_) if self.received => return Err(TransportError::Truncated),
                Err(_) => return Err(TransportError::Io),
            }
        }
    }

    /// Reads up to the blank line. Returns the head and the body bytes already
    /// received.
    pub fn read_head(&mut self) -> Result<(Head, Vec<u8>), TransportError> {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(end) = find(&buf, b"\r\n\r\n") {
                if end > MAX_HEADER_BYTES {
                    return Err(TransportError::Limit);
                }
                let head = parse_head(&buf[..end])?;
                return Ok((head, buf.split_off(end + 4)));
            }
            if buf.len() > MAX_HEADER_BYTES {
                return Err(TransportError::Limit);
            }
            let n = self.read(&mut chunk)?;
            if n == 0 {
                return Err(TransportError::Truncated);
            }
            buf.extend_from_slice(&chunk[..n]);
        }
    }

    pub fn read_body(
        &mut self,
        framing: BodyFraming,
        mut pending: Vec<u8>,
        limit: usize,
    ) -> Result<Vec<u8>, TransportError> {
        let mut chunk = [0u8; 4096];
        match framing {
            BodyFraming::Length(size) => {
                while pending.len() < size {
                    let n = self.read(&mut chunk)?;
                    if n == 0 {
                        return Err(TransportError::Truncated);
                    }
                    pending.extend_from_slice(&chunk[..n]);
                    if pending.len() > size {
                        return Err(TransportError::Malformed);
                    }
                }
                if pending.len() > size {
                    return Err(TransportError::Malformed);
                }
                Ok(pending)
            }
            BodyFraming::UntilClose => loop {
                if pending.len() > limit {
                    return Err(TransportError::Limit);
                }
                let n = self.read(&mut chunk)?;
                if n == 0 {
                    return Ok(pending);
                }
                pending.extend_from_slice(&chunk[..n]);
            },
            BodyFraming::Chunked => {
                let mut body = Vec::new();
                let mut raw_total = pending.len();
                let raw_cap = chunked_raw_cap(limit);
                loop {
                    if dechunk(&mut pending, &mut body, limit)? {
                        return Ok(body);
                    }
                    let n = self.read(&mut chunk)?;
                    if n == 0 {
                        return Err(TransportError::Truncated);
                    }
                    raw_total += n;
                    if raw_total > raw_cap {
                        return Err(TransportError::Limit);
                    }
                    pending.extend_from_slice(&chunk[..n]);
                }
            }
        }
    }
}

/// Most raw bytes a chunked body may take on the wire. A decoded body of at
/// most `limit` bytes needs at most one chunk per byte, and one chunk costs at
/// most `MAX_CHUNK_OVERHEAD` bytes (an 8 digit size line, two CRLF), so this cap
/// accepts every legal body within the limit. The terminal chunk fits in the
/// slack.
fn chunked_raw_cap(limit: usize) -> usize {
    limit
        .saturating_mul(1 + MAX_CHUNK_OVERHEAD)
        .saturating_add(MAX_HEADER_BYTES)
}

/// Consumes whole chunks from `buf`. Returns true once the terminal chunk and
/// the empty trailer have been read. Trailers and chunk extensions are refused.
fn dechunk(buf: &mut Vec<u8>, body: &mut Vec<u8>, limit: usize) -> Result<bool, TransportError> {
    loop {
        let Some(line_end) = find(buf, b"\r\n") else {
            if buf.len() > MAX_CHUNK_LINE {
                return Err(TransportError::Malformed);
            }
            return Ok(false);
        };
        let line = &buf[..line_end];
        if line.is_empty() || line.len() > 8 || !line.iter().all(u8::is_ascii_hexdigit) {
            return Err(TransportError::Malformed);
        }
        let size = usize::from_str_radix(
            std::str::from_utf8(line).map_err(|_| TransportError::Malformed)?,
            16,
        )
        .map_err(|_| TransportError::Malformed)?;
        if size == 0 {
            if buf.len() < line_end + 4 {
                return Ok(false);
            }
            if &buf[line_end + 2..line_end + 4] != b"\r\n" || buf.len() != line_end + 4 {
                return Err(TransportError::Malformed);
            }
            buf.clear();
            return Ok(true);
        }
        if body.len().saturating_add(size) > limit {
            return Err(TransportError::Limit);
        }
        let need = line_end + 2 + size + 2;
        if buf.len() < need {
            return Ok(false);
        }
        if &buf[need - 2..need] != b"\r\n" {
            return Err(TransportError::Malformed);
        }
        body.extend_from_slice(&buf[line_end + 2..line_end + 2 + size]);
        buf.drain(..need);
    }
}

fn connect(
    addr: SocketAddr,
    deadline: Instant,
    cancel: &CancelToken,
) -> Result<TcpStream, TransportError> {
    // Short attempts, so a full backlog cannot hold the call past a
    // cancellation or the deadline.
    loop {
        if cancel.is_cancelled() {
            return Err(TransportError::Cancelled);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or(TransportError::Timeout)?;
        match TcpStream::connect_timeout(&addr, remaining.min(POLL)) {
            Ok(stream) => return Ok(stream),
            Err(e) if retryable(&e) => {}
            Err(_) => return Err(TransportError::Io),
        }
    }
}

fn retryable(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::Interrupted
    )
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn parse_head(bytes: &[u8]) -> Result<Head, TransportError> {
    let text = std::str::from_utf8(bytes).map_err(|_| TransportError::Malformed)?;
    let mut lines = text.split("\r\n");
    let status_line = lines.next().ok_or(TransportError::Malformed)?;
    let mut fields = status_line.splitn(3, ' ');
    if !matches!(fields.next(), Some("HTTP/1.0" | "HTTP/1.1")) {
        return Err(TransportError::Malformed);
    }
    let code = fields.next().ok_or(TransportError::Malformed)?;
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(TransportError::Malformed);
    }
    let status = code.parse().map_err(|_| TransportError::Malformed)?;
    let mut headers = Vec::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(TransportError::Malformed)?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_graphic() && b != b':') {
            return Err(TransportError::Malformed);
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_owned()));
    }
    Ok(Head { status, headers })
}
