use std::{
    collections::BTreeMap,
    fmt,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

use router_types::{BackendProposal, BackendRequest};
use serde::Deserialize;

use crate::{
    BackendError, DecisionBackend, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, Result, validate_request,
};

const MAX_HEADER_BYTES: usize = 16 * 1024;

/// An explicit numeric loopback address avoids DNS, proxy and redirect egress.
/// This transport starts no server and installs no model. Authentication is
/// held in memory and never included in diagnostic formatting or errors.
pub struct JevSidecarBackend {
    endpoint: SocketAddr,
    bearer: Option<String>,
}

impl fmt::Debug for JevSidecarBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JevSidecarBackend")
            .field("endpoint", &self.endpoint)
            .field("authentication_configured", &self.bearer.is_some())
            .finish()
    }
}

impl JevSidecarBackend {
    pub fn new(endpoint: SocketAddr, bearer: Option<String>) -> Result<Self> {
        if !endpoint.ip().is_loopback() || endpoint.port() == 0 {
            return Err(BackendError::Endpoint);
        }
        if bearer.as_ref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 4096
                || !value.bytes().all(|b| (33..=126).contains(&b))
        }) {
            return Err(BackendError::Invalid);
        }
        Ok(Self { endpoint, bearer })
    }

    fn request(&self, request: &BackendRequest, deadline: Instant) -> Result<BackendProposal> {
        validate_request(request)?;
        if request.candidates.is_empty() {
            return Err(BackendError::Abstained);
        }
        let criteria: BTreeMap<_, _> = request
            .candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.id.clone(),
                    format!(
                        "utility_micros={} cost_microusd={} tokens={} elapsed_ms={}",
                        candidate.utility_micros,
                        candidate.cost_microusd,
                        candidate.tokens,
                        candidate.elapsed_ms
                    ),
                )
            })
            .collect();
        let body = serde_json::to_vec(&serde_json::json!({
            "state": request,
            "questions": { "route": {
                "type": "choice",
                "instructions": "Select only one supplied opaque candidate ID using the supplied numerical features. Return confidence and probabilities. Do not invent a candidate.",
                "criteria": criteria
            }}
        })).map_err(|_| BackendError::Schema)?;
        let response = self.post("/v1/systemone", &body, deadline)?;
        let value = router_adapter_api::strict_json(&response).map_err(|_| BackendError::Schema)?;
        let envelope: JevResponse =
            serde_json::from_value(value).map_err(|_| BackendError::Schema)?;
        let answer = envelope.answers.get("route").ok_or(BackendError::Schema)?;
        if envelope.answers.len() != 1 {
            return Err(BackendError::Schema);
        }
        let _ = &answer.answer_type;
        probability(answer.confidence)?;
        if let Some(action) = &answer.action {
            probability(action.act_probability)?;
        }
        // Laya's legacy confidence is entropy, while answer_confidence is the
        // chosen answer's reported confidence. Never treat entropy as that
        // probability when the more specific field or distribution is present.
        let confidence = answer
            .answer_confidence
            .or_else(|| {
                answer
                    .choice
                    .as_ref()
                    .and_then(|id| answer.probabilities.get(id).copied())
            })
            .unwrap_or(answer.confidence);
        let probabilities_micros = answer
            .probabilities
            .iter()
            .map(|(key, value)| Ok((key.clone(), probability(*value)?)))
            .collect::<Result<_>>()?;
        Ok(BackendProposal {
            schema_version: 1,
            artifact_digest: request.artifact_digest.clone(),
            candidate_id: answer.choice.clone(),
            confidence_micros: probability(confidence)?,
            probabilities_micros,
        })
    }

    fn post(&self, path: &str, body: &[u8], deadline: Instant) -> Result<Vec<u8>> {
        if body.len() > MAX_REQUEST_BYTES {
            return Err(BackendError::Limit);
        }
        let mut stream =
            TcpStream::connect_timeout(&self.endpoint, remaining(deadline)?).map_err(io_error)?;
        let mut headers = format!(
            "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nAccept: application/json\r\nAccept-Encoding: identity\r\nContent-Length: {}\r\nConnection: close\r\n",
            self.endpoint,
            body.len()
        );
        if let Some(bearer) = &self.bearer {
            headers.push_str(&format!("Authorization: Bearer {bearer}\r\n"));
        }
        headers.push_str("\r\n");
        write_all_deadline(&mut stream, headers.as_bytes(), deadline)?;
        write_all_deadline(&mut stream, body, deadline)?;
        read_response(&mut stream, deadline)
    }
}

impl DecisionBackend for JevSidecarBackend {
    fn name(&self) -> &'static str {
        "jev-sidecar"
    }
    fn requires_data_policy(&self) -> bool {
        true
    }
    fn propose(&self, request: &BackendRequest, deadline: Instant) -> Result<BackendProposal> {
        self.request(request, deadline)
    }
}

/// Laya's optional local server exposes the same /v1/systemone choice contract.
#[derive(Debug)]
pub struct LayaLocalBackend(JevSidecarBackend);
impl LayaLocalBackend {
    pub fn new(endpoint: SocketAddr, bearer: Option<String>) -> Result<Self> {
        JevSidecarBackend::new(endpoint, bearer).map(Self)
    }
}
impl DecisionBackend for LayaLocalBackend {
    fn name(&self) -> &'static str {
        "laya-local"
    }
    fn requires_data_policy(&self) -> bool {
        true
    }
    fn propose(&self, request: &BackendRequest, deadline: Instant) -> Result<BackendProposal> {
        self.0.request(request, deadline)
    }
}

#[derive(Deserialize)]
struct JevResponse {
    answers: BTreeMap<String, ChoiceAnswer>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceAnswer {
    #[serde(rename = "type")]
    answer_type: ChoiceType,
    choice: Option<String>,
    confidence: f64,
    answer_confidence: Option<f64>,
    action: Option<ChoiceAction>,
    #[serde(default)]
    probabilities: BTreeMap<String, f64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ChoiceType {
    Choice,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChoiceAction {
    act_probability: f64,
}

fn probability(value: f64) -> Result<u32> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(BackendError::Schema);
    }
    Ok((value * 1_000_000.0).round() as u32)
}
fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|duration| !duration.is_zero())
        .ok_or(BackendError::Timeout)
}
fn io_error(error: std::io::Error) -> BackendError {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => BackendError::Timeout,
        _ => BackendError::Transport,
    }
}
fn write_all_deadline(stream: &mut TcpStream, mut bytes: &[u8], deadline: Instant) -> Result<()> {
    while !bytes.is_empty() {
        stream
            .set_write_timeout(Some(remaining(deadline)?))
            .map_err(io_error)?;
        let written = stream.write(bytes).map_err(io_error)?;
        if written == 0 {
            return Err(BackendError::Transport);
        }
        bytes = &bytes[written..];
    }
    Ok(())
}

fn read_response(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>> {
    let mut received = Vec::new();
    let mut expected = None;
    loop {
        stream
            .set_read_timeout(Some(remaining(deadline)?))
            .map_err(io_error)?;
        let mut buffer = [0u8; 4096];
        let length = stream.read(&mut buffer).map_err(io_error)?;
        if length == 0 {
            return Err(BackendError::Transport);
        }
        received.extend_from_slice(&buffer[..length]);
        if received.len() > MAX_HEADER_BYTES + MAX_RESPONSE_BYTES {
            return Err(BackendError::Limit);
        }
        if expected.is_none() {
            if let Some(header_end) = received.windows(4).position(|window| window == b"\r\n\r\n") {
                if header_end > MAX_HEADER_BYTES {
                    return Err(BackendError::Limit);
                }
                expected = Some((header_end + 4, response_length(&received[..header_end])?));
            } else if received.len() > MAX_HEADER_BYTES {
                return Err(BackendError::Limit);
            }
        }
        if let Some((offset, size)) = expected {
            if received.len() > offset + size {
                return Err(BackendError::Schema);
            }
            if received.len() == offset + size {
                return Ok(received.split_off(offset));
            }
        }
    }
}
fn response_length(header: &[u8]) -> Result<usize> {
    let header = std::str::from_utf8(header).map_err(|_| BackendError::Schema)?;
    let mut lines = header.split("\r\n");
    let status = lines.next().ok_or(BackendError::Schema)?;
    let mut status_fields = status.split_whitespace();
    if !matches!(status_fields.next(), Some("HTTP/1.0" | "HTTP/1.1"))
        || status_fields.next() != Some("200")
    {
        return Err(BackendError::Transport);
    }
    let mut length = None;
    let mut content_type = false;
    for line in lines {
        let (name, value) = line.split_once(':').ok_or(BackendError::Schema)?;
        match name.to_ascii_lowercase().as_str() {
            "content-length" => {
                if length.is_some() {
                    return Err(BackendError::Schema);
                }
                let parsed: usize = value.trim().parse().map_err(|_| BackendError::Schema)?;
                if parsed > MAX_RESPONSE_BYTES {
                    return Err(BackendError::Limit);
                }
                length = Some(parsed);
            }
            "content-type" => {
                if content_type
                    || value.split(';').next().map(str::trim) != Some("application/json")
                {
                    return Err(BackendError::Schema);
                }
                content_type = true;
            }
            "transfer-encoding" | "content-encoding" => return Err(BackendError::Schema),
            _ => {}
        }
    }
    if !content_type {
        return Err(BackendError::Schema);
    }
    length.ok_or(BackendError::Schema)
}
