//! Just enough HTTP/1.1 for the host API: one request per connection,
//! bounded headers and body, JSON answers, `Connection: close`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::{Backing, Host, Request, Response};

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 64 * 1024;
/// A connection's whole life.
const DEADLINE: Duration = Duration::from_secs(15);

pub async fn serve(listener: TcpListener, host: &'static Host, backing: Arc<dyn Backing>) {
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            continue;
        };
        let backing = backing.clone();
        tauri::async_runtime::spawn(async move {
            let request = match tokio::time::timeout(DEADLINE, read(&mut stream)).await {
                Ok(Ok(request)) => request,
                Ok(Err(response)) => {
                    let _ = stream.write_all(&write(&response)).await;
                    let _ = stream.shutdown().await;
                    return;
                }
                Err(_) => return,
            };
            // Plugin AI: async, and a run streams for as long as it runs
            // (plugin-ai US-5), so no deadline.
            if crate::plugin_ai::host::handles(&request.path) {
                crate::plugin_ai::host::serve(&mut stream, host, backing, request).await;
                return;
            }
            let _ = tokio::time::timeout(DEADLINE, async {
                // Vaults open the keychain: a blocking thread.
                let response = tauri::async_runtime::spawn_blocking(move || host.handle(backing.as_ref(), &request))
                    .await
                    .unwrap_or_else(|e| super::error(500, "internal", e.to_string()));
                let _ = stream.write_all(&write(&response)).await;
                let _ = stream.shutdown().await;
            })
            .await;
        });
    }
}

/// Read one request from `stream`.
async fn read(stream: &mut (impl AsyncReadExt + Unpin)) -> Result<Request, Response> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(at) = find(&buffer, b"\r\n\r\n") {
            break at;
        }
        if buffer.len() > MAX_HEAD {
            return Err(super::error(431, "bad_request", "headers too large"));
        }
        let n = stream.read(&mut chunk).await.map_err(|_| super::error(400, "bad_request", "read failed"))?;
        if n == 0 {
            return Err(super::error(400, "bad_request", "incomplete request"));
        }
        buffer.extend_from_slice(&chunk[..n]);
    };
    let mut request = parse_head(&buffer[..head_end])?;
    let length = request.body.len();
    let mut body = buffer[head_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).await.map_err(|_| super::error(400, "bad_request", "read failed"))?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    request.body = body;
    Ok(request)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The request line and headers; `body` comes back sized to Content-Length
/// (zeros) for [`read`] to fill.
pub fn parse_head(head: &[u8]) -> Result<Request, Response> {
    let bad = |m: &str| super::error(400, "bad_request", m);
    let text = std::str::from_utf8(head).map_err(|_| bad("headers must be UTF-8"))?;
    let mut lines = text.split("\r\n");
    let line = lines.next().unwrap_or_default();
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(bad("bad request line"));
    };
    if !version.starts_with("HTTP/1.") {
        return Err(bad("HTTP/1.x only"));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut request = Request {
        method: method.to_string(),
        path: decode(path, false),
        query: parse_query(query),
        ..Default::default()
    };
    let mut length = 0usize;
    for header in lines {
        let Some((name, value)) = header.split_once(':') else { continue };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "authorization" => {
                request.key = value.strip_prefix("Bearer ").or_else(|| value.strip_prefix("bearer ")).map(|k| k.trim().to_string());
            }
            "origin" => request.origin = true,
            "content-length" => length = value.parse().map_err(|_| bad("bad Content-Length"))?,
            "transfer-encoding" => return Err(super::error(411, "bad_request", "send Content-Length, not chunks")),
            _ => {}
        }
    }
    if length > MAX_BODY {
        return Err(super::error(413, "bad_request", "body too large"));
    }
    request.body = vec![0; length];
    Ok(request)
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(k, true), decode(v, true))
        })
        .collect()
}

/// Percent-decoding (and `+` as a space in queries).
fn decode(text: &str, plus: bool) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("zz"), 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' if plus => out.push(b' '),
            other => out.push(other),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

pub fn write(response: &Response) -> Vec<u8> {
    let body = response.body.as_ref().map(|b| b.to_string()).unwrap_or_default();
    let mut out = format!("HTTP/1.1 {} {}\r\nConnection: close\r\nCache-Control: no-store\r\n", response.status, reason(response.status));
    if response.body.is_some() {
        out.push_str("Content-Type: application/json\r\n");
    }
    out.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    out.into_bytes()
}

#[cfg(test)]
pub fn decode_for_test(text: &str) -> String {
    decode(text, true)
}
