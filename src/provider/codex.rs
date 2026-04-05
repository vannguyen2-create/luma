/// Codex provider — OpenAI Responses API at chatgpt.com/backend-api/codex.
use crate::core::provider::Provider;
use crate::core::types::{Message, Role, ToolCall, ToolCallFunction, ToolSchema, ThinkingLevel, Usage};
use crate::event::Event;
use anyhow::{bail, Result};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const CODEX_ENDPOINT: &str = "https://chatgpt.com/backend-api/codex/responses";

/// Codex provider using the Responses API.
pub struct CodexProvider {
    model: String,
    api_key: String,
    account_id: Option<String>,
    thinking: ThinkingLevel,
    session_id: Option<String>,
}

impl CodexProvider {
    /// Create with model, token, optional account ID, and session ID for cache routing.
    pub fn new(model: &str, api_key: &str, account_id: Option<String>, session_id: &str) -> Self {
        Self {
            model: model.to_owned(),
            api_key: api_key.to_owned(),
            account_id,
            thinking: ThinkingLevel::Low,
            session_id: Some(session_id.to_owned()),
        }
    }
}

impl Provider for CodexProvider {
    fn name(&self) -> &str { "codex" }
    fn thinking(&self) -> ThinkingLevel { self.thinking }
    fn set_thinking(&mut self, level: ThinkingLevel) { self.thinking = level; }

    fn stream<'a>(
        &'a self,
        messages: &'a [Message],
        tools: &'a [ToolSchema],
        tx: mpsc::Sender<Event>,
        cancel: CancellationToken,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(Message, Usage)>> + Send + 'a>> {
        Box::pin(async move {
        let system = extract_system(messages);
        let input = build_input(messages);
        let api_tools = to_api_tools(tools);

        let mut body = serde_json::json!({
            "model": self.model,
            "input": input,
            "store": false,
            "stream": true,
        });

        if !system.is_empty() {
            body["instructions"] = system.into();
        }
        if !api_tools.is_empty() {
            body["tools"] = api_tools.into();
        }
        if let Some(key) = &self.session_id {
            body["prompt_cache_key"] = serde_json::json!(key);
        }

        let client = reqwest::Client::new();
        let mut req = client.post(CODEX_ENDPOINT)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body);

        if let Some(aid) = &self.account_id {
            req = req.header("chatgpt-account-id", aid.as_str());
        }

        let response = req.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let msg = serde_json::from_str::<serde_json::Value>(&body).ok()
                .and_then(|v| v["error"]["message"].as_str().or(v["message"].as_str()).map(|s| s.to_owned()))
                .unwrap_or_else(|| body[..body.len().min(200)].to_owned());
            bail!("{status}: {msg}");
        }

        let mut text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut buf = String::new();
        let mut usage = Usage::default();
        let mut response = response;

        loop {
            let chunk = tokio::select! {
                c = response.chunk() => c?,
                _ = cancel.cancelled() => { bail!("Aborted"); }
            };
            let Some(chunk) = chunk else { break };
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].to_owned();
                buf = buf[pos + 1..].to_owned();

                let Some(raw) = line.strip_prefix("data:") else { continue; };
                let raw = raw.trim();
                let Ok(event) = serde_json::from_str::<serde_json::Value>(raw) else { continue; };

                let event_type = event["type"].as_str().unwrap_or("");

                match event_type {
                    "response.output_text.delta" => {
                        if let Some(delta) = event["delta"].as_str() {
                            text.push_str(delta);
                            let _ = tx.try_send(Event::Token(delta.to_owned()));
                        }
                    }
                    "response.reasoning_summary_text.delta" => {
                        if let Some(delta) = event["delta"].as_str() {
                            let _ = tx.try_send(Event::Thinking(delta.to_owned()));
                        }
                    }
                    "response.completed" => {
                        // Extract tool calls from output
                        if let Some(output) = event["response"]["output"].as_array() {
                            for item in output {
                                if item["type"] == "function_call" {
                                    tool_calls.push(ToolCall {
                                        id: item["call_id"].as_str().unwrap_or("").to_owned(),
                                        r#type: "function".into(),
                                        function: ToolCallFunction {
                                            name: item["name"].as_str().unwrap_or("").to_owned(),
                                            arguments: item["arguments"].as_str().unwrap_or("{}").to_owned(),
                                        },
                                    });
                                }
                            }
                        }
                        // Usage
                        if let Some(u) = event["response"]["usage"].as_object() {
                            let cached = u.get("input_tokens_details")
                                .and_then(|d| d.get("cached_tokens"))
                                .and_then(|v| v.as_u64());
                            let u_data = Usage {
                                input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                                output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                                cache_read: cached,
                                cache_write: None,
                            };
                            usage = u_data.clone();
                            let _ = tx.try_send(Event::Usage(u_data));
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok((Message {
            role: Role::Assistant,
            content: text,
            tool_call_id: None,
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
        }, usage))
    })
    }
}

fn extract_system(messages: &[Message]) -> String {
    messages.iter()
        .filter(|m| m.role == Role::System)
        .map(|m| m.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn build_input(messages: &[Message]) -> Vec<serde_json::Value> {
    let mut input = Vec::new();
    for msg in messages {
        if msg.role == Role::System { continue; }
        match msg.role {
            Role::User => {
                input.push(serde_json::json!({"role": "user", "content": msg.content}));
            }
            Role::Assistant => {
                if let Some(tcs) = &msg.tool_calls {
                    for tc in tcs {
                        input.push(serde_json::json!({
                            "type": "function_call",
                            "name": tc.function.name,
                            "call_id": tc.id,
                            "arguments": tc.function.arguments,
                        }));
                    }
                }
                if !msg.content.is_empty() {
                    input.push(serde_json::json!({"role": "assistant", "content": msg.content}));
                }
            }
            Role::Tool => {
                input.push(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": msg.tool_call_id.as_deref().unwrap_or(""),
                    "output": msg.content,
                }));
            }
            _ => {}
        }
    }
    input
}

fn to_api_tools(tools: &[ToolSchema]) -> Vec<serde_json::Value> {
    tools.iter().map(|t| serde_json::json!({
        "type": "function",
        "name": t.name,
        "description": t.description,
        "parameters": t.parameters,
    })).collect()
}
