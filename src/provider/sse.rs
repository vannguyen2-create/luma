/// SSE (Server-Sent Events) streaming parser for LLM APIs.
use anyhow::{bail, Result};
use tokio_util::sync::CancellationToken;

/// A parsed SSE event with type and JSON data.
pub struct SseEvent {
    #[allow(dead_code)]
    pub event_type: String,
    pub data: serde_json::Value,
}

/// Build and send an SSE POST request, then stream events via callback.
pub async fn post_sse(
    url: &str,
    headers: &[(&str, &str)],
    body: &serde_json::Value,
    cancel: &CancellationToken,
    mut on_event: impl FnMut(SseEvent),
) -> Result<()> {
    let client = reqwest::Client::new();
    let mut req = client.post(url)
        .header("Content-Type", "application/json")
        .json(body);

    for (k, v) in headers {
        req = req.header(*k, *v);
    }

    let response = req.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<serde_json::Value>(&body).ok()
            .and_then(|v| {
                v["error"]["message"].as_str()
                    .or_else(|| v["message"].as_str())
                    .or_else(|| v["error"].as_str())
                    .map(|s| s.to_owned())
            })
            .unwrap_or_else(|| body[..body.len().min(200)].to_owned());
        bail!("{status}: {msg}");
    }

    let mut buf = String::new();
    let mut current_event = String::new();
    let mut response = response;

    loop {
        let chunk = tokio::select! {
            c = response.chunk() => c?,
            _ = cancel.cancelled() => { bail!("Aborted"); }
        };
        let Some(chunk) = chunk else { break; };
        buf.push_str(&String::from_utf8_lossy(&chunk));

        let mut start = 0;
        while let Some(rel_pos) = buf[start..].find('\n') {
            let newline_pos = start + rel_pos;
            let line = &buf[start..newline_pos];
            start = newline_pos + 1;

            if let Some(rest) = line.strip_prefix("event:") {
                current_event.clear();
                current_event.push_str(rest.trim());
            } else if let Some(rest) = line.strip_prefix("data:") {
                let raw = rest.trim();
                if raw == "[DONE]" { continue; }
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(raw) {
                    let event_type = if current_event.is_empty() {
                        data.get("type").and_then(|t| t.as_str()).unwrap_or("").to_owned()
                    } else {
                        current_event.clone()
                    };
                    on_event(SseEvent { event_type, data });
                }
            } else if line.is_empty() {
                current_event.clear();
            }
        }
        if start > 0 {
            buf.drain(..start);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_event_fields() {
        let event = SseEvent {
            event_type: "message".into(),
            data: serde_json::json!({"text": "hi"}),
        };
        assert_eq!(event.event_type, "message");
        assert_eq!(event.data["text"], "hi");
    }
}
