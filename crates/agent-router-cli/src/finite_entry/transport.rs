use super::{EntryResult, input::Input, output::Output};
use router_adapter_api::{content_digest, strict_json_limited};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use serde_json::json;
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(super) struct Broker {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<EntryResult<(usize, Input)>>>,
}
impl Broker {
    pub(super) fn start(
        listener: TcpListener,
        mut input: Input,
        output: Output,
        model: String,
        effort: String,
    ) -> EntryResult<Self> {
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let join = std::thread::spawn(move || {
            let mut count = 0usize;
            let mut seen = BTreeSet::new();
            while !flag.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                };
                stream.set_nonblocking(true)?;
                let Some((request, body)) = read_http(&mut stream, &flag)? else {
                    continue;
                };
                if request.starts_with("GET /__router_probe__ ") {
                    send(&mut stream, b"ok", &flag)?;
                    continue;
                }
                if !request.starts_with("POST /v1/responses ") || count >= 9 {
                    return Err("local endpoint or request limit".into());
                }
                let value = strict_json_limited(&body, 262144)?;
                if value["model"] != model
                    || value["reasoning"]["effort"] != effort
                    || value["stream"] != true
                    || value.get("store").is_some_and(|v| v != false)
                {
                    return Err("received request contradicts approved selection".into());
                }
                if !seen.insert(content_digest(&serde_json::to_vec(&value)?)) {
                    return Err("duplicate task request".into());
                }
                count += 1;
                output.emit("entry.request", json!({"request_id":count,"request":value}))?;
                let raw = input
                    .line(262144, Instant::now() + Duration::from_secs(5))
                    .map_err(|_| "response input")?;
                let response = strict_json_limited(&raw, 262144)?;
                let sse = response["sse"].as_str().ok_or("response body")?;
                if sse.is_empty()
                    || sse.len() > 262144
                    || response
                        != json!({"schema_version":1,"kind":"response","session_id":output.session_id,"submission_sha256":output.submission_sha256,"request_id":count,"sse":sse})
                {
                    return Err("response binding".into());
                }
                send(&mut stream, sse.as_bytes(), &flag)?;
            }
            Ok((count, input))
        });
        Ok(Self {
            stop,
            join: Some(join),
        })
    }
    pub(super) fn finish(mut self) -> EntryResult<(usize, Input)> {
        self.stop.store(true, Ordering::SeqCst);
        self.join
            .take()
            .ok_or("broker owner")?
            .join()
            .map_err(|_| "broker panic")?
    }
}
impl Drop for Broker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
fn ready(stream: &TcpStream, flags: PollFlags, end: Instant, stop: &AtomicBool) -> EntryResult<()> {
    loop {
        if stop.load(Ordering::SeqCst) {
            return Err("broker stopped".into());
        }
        let left = end
            .checked_duration_since(Instant::now())
            .ok_or("HTTP timeout")?
            .min(Duration::from_millis(100));
        let timeout = Timespec {
            tv_sec: 0,
            tv_nsec: left.as_nanos().try_into()?,
        };
        let mut fds = [PollFd::new(stream, flags)];
        match poll(&mut fds, Some(&timeout)) {
            Ok(0) | Err(rustix::io::Errno::INTR) => continue,
            Ok(_) => return Ok(()),
            Err(e) => return Err(e.into()),
        }
    }
}
fn read_http(stream: &mut TcpStream, stop: &AtomicBool) -> EntryResult<Option<(String, Vec<u8>)>> {
    let end = Instant::now() + Duration::from_secs(5);
    let mut bytes = Vec::new();
    let mut parsed: Option<(String, usize, usize)> = None;
    loop {
        if let Some((ref request, start, len)) = parsed {
            if bytes.len() == start + len {
                return Ok(Some((request.clone(), bytes[start..].to_vec())));
            }
            if bytes.len() > start + len {
                return Err("HTTP pipelining refused".into());
            }
        }
        ready(stream, PollFlags::IN, end, stop)?;
        let mut chunk = [0; 4096];
        let n = match stream.read(&mut chunk) {
            Ok(0) if bytes.is_empty() => return Ok(None), // TCP-only preparation canary.
            Ok(0) => return Err("incomplete HTTP".into()),
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > 278528 {
            return Err("HTTP size".into());
        }
        if parsed.is_none() {
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                if end > 16384 {
                    return Err("header size".into());
                }
                let text = std::str::from_utf8(&bytes[..end])?;
                let mut lines = text.split("\r\n");
                let request = lines.next().ok_or("HTTP request")?.to_owned();
                let mut length = None;
                for line in lines {
                    let (name, value) = line.split_once(':').ok_or("HTTP header")?;
                    if name.eq_ignore_ascii_case("authorization")
                        || name.eq_ignore_ascii_case("proxy-authorization")
                        || name.eq_ignore_ascii_case("transfer-encoding")
                    {
                        return Err("forbidden HTTP header".into());
                    }
                    if name.eq_ignore_ascii_case("content-length") {
                        if length.is_some() {
                            return Err("duplicate content length".into());
                        }
                        length = Some(value.trim().parse::<usize>()?);
                    }
                }
                let length = length.unwrap_or(0);
                if length > 262144 {
                    return Err("body size".into());
                }
                parsed = Some((request, end + 4, length));
            } else if bytes.len() > 16384 {
                return Err("header size".into());
            }
        }
    }
}
fn send(stream: &mut TcpStream, body: &[u8], stop: &AtomicBool) -> EntryResult<()> {
    let mut response=format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).into_bytes();
    response.extend_from_slice(body);
    let end = Instant::now() + Duration::from_secs(5);
    let mut n = 0;
    while n < response.len() {
        ready(stream, PollFlags::OUT, end, stop)?;
        match stream.write(&response[n..]) {
            Ok(0) => return Err("HTTP closed".into()),
            Ok(w) => n += w,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::File,
        io::{BufRead, BufReader},
        net::Shutdown,
        os::{fd::OwnedFd, unix::net::UnixStream},
    };
    fn http(bytes: &[u8]) -> EntryResult<Option<(String, Vec<u8>)>> {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.write_all(bytes).unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_nonblocking(true).unwrap();
        read_http(&mut stream, &AtomicBool::new(false))
    }
    #[test]
    fn http_refuses_credentials_smuggling_partial_bodies_and_oversize() {
        for raw in [
            "POST /v1/responses HTTP/1.1\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
            "POST /v1/responses HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            "POST /v1/responses HTTP/1.1\r\nAuthorization: secret-fixture\r\n\r\n",
            "POST /v1/responses HTTP/1.1\r\nContent-Length: 262145\r\n\r\n",
            "POST /v1/responses HTTP/1.1\r\nContent-Length: 2\r\n\r\n{",
            "POST /v1/responses HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}GET /more HTTP/1.1\r\n\r\n",
        ] {
            assert!(http(raw.as_bytes()).is_err(), "{raw}");
        }
        assert!(http(b"").unwrap().is_none());
        assert_eq!(
            http(b"POST /v1/responses HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}")
                .unwrap()
                .unwrap()
                .1,
            b"{}"
        );
    }
    #[test]
    fn stopped_broker_does_not_wait_for_partial_http() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        client.write_all(b"POST /v1/responses").unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let join = std::thread::spawn(move || read_http(&mut stream, &flag));
        std::thread::sleep(Duration::from_millis(20));
        let begin = Instant::now();
        stop.store(true, Ordering::SeqCst);
        assert!(join.join().unwrap().is_err());
        assert!(begin.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn actual_broker_requires_matching_parent_response_and_no_duplicate_request() {
        for mode in 0..5 {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let (child_in, mut parent_in) = UnixStream::pair().unwrap();
            let (child_out, parent_out) = UnixStream::pair().unwrap();
            parent_out
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let input = Input::from_file(File::from(OwnedFd::from(child_in)));
            let output = Output::from_file(
                File::from(OwnedFd::from(child_out)),
                "session".into(),
                "task".into(),
            )
            .unwrap();
            let broker =
                Broker::start(listener, input, output, "pinned".into(), "medium".into()).unwrap();
            let body=serde_json::to_vec(&json!({"model":"pinned","reasoning":{"effort":"medium"},"stream":true,"store":false})).unwrap();
            let request = [
                format!(
                    "POST /v1/responses HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
                &body,
            ]
            .concat();
            let mut client = TcpStream::connect(addr).unwrap();
            client
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            client.write_all(&request).unwrap();
            let mut raw = String::new();
            BufReader::new(parent_out).read_line(&mut raw).unwrap();
            let frame: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(frame["request_id"], 1);
            assert_eq!(frame["request"]["model"], "pinned");
            let mut reply = json!({"schema_version":1,"kind":"response","session_id":"session","submission_sha256":"task","request_id":1,"sse":"data: fixture\n\n"});
            if mode == 1 {
                reply["session_id"] = json!("stale");
            }
            if mode == 2 {
                reply["request_id"] = json!(2);
            }
            if mode != 3 {
                writeln!(parent_in, "{reply}").unwrap();
            }
            if mode == 4 {
                writeln!(parent_in, "next-approval").unwrap();
            }
            drop(parent_in);
            let mut response = String::new();
            client.read_to_string(&mut response).unwrap();
            if mode == 0 || mode == 4 {
                assert!(response.starts_with("HTTP/1.1 200"));
                assert!(response.ends_with("data: fixture\n\n"));
                if mode == 4 {
                    let (count, mut recovered) = broker.finish().unwrap();
                    assert_eq!(count, 1);
                    assert_eq!(
                        recovered
                            .line(128, Instant::now() + Duration::from_secs(1))
                            .unwrap_or_else(|_| panic!("approval input lost")),
                        b"next-approval"
                    );
                    continue;
                }
                let mut duplicate = TcpStream::connect(addr).unwrap();
                duplicate
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                duplicate.write_all(&request).unwrap();
                let mut bytes = Vec::new();
                let _ = duplicate.read_to_end(&mut bytes);
                assert!(bytes.is_empty());
            } else {
                assert!(response.is_empty());
            }
            assert!(broker.finish().is_err());
        }
    }
}
