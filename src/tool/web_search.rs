/// WebSearch tool — client-side web search via external APIs.
///
/// Used as fallback when the provider has no built-in search.
/// Supports Exa, Tavily, and SearXNG backends.
use crate::core::tool::Tool;
use crate::core::types::ToolSchema;
use anyhow::{bail, Result};
use std::pin::Pin;
use std::future::Future;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Search backend configuration.
#[derive(Clone)]
pub enum SearchBackend {
    Exa { api_key: String },
    Tavily { api_key: String },
    SearXNG { base_url: String },
}

/// Client-side web search tool.
pub struct WebSearchTool {
    backend: SearchBackend,
}

impl WebSearchTool {
    pub fn new(backend: SearchBackend) -> Self {
        Self { backend }
    }
}

impl Tool for WebSearchTool {
    fn name(&self) -> &str { "WebSearch" }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: "WebSearch".into(),
            description: concat!(
                "Search the web for current information.\n",
                "- Returns titles, URLs, and snippets for top results.\n",
                "- Use for documentation, API references, error messages, latest versions.\n",
                "- Do not use for questions answerable from codebase alone.",
            ).into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "max_results": {
                        "type": "number",
                        "description": "Maximum results to return (default 5)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    fn execute(
        &self,
        args: serde_json::Value,
        _output_tx: mpsc::Sender<String>,
        _cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = Result<String>> + Send + '_>> {
        Box::pin(async move {
            let query = args["query"].as_str().unwrap_or("");
            if query.is_empty() { bail!("missing query"); }
            let max = args["max_results"].as_u64().unwrap_or(5) as usize;

            let results = match &self.backend {
                SearchBackend::Exa { api_key } => search_exa(api_key, query, max).await?,
                SearchBackend::Tavily { api_key } => search_tavily(api_key, query, max).await?,
                SearchBackend::SearXNG { base_url } => search_searxng(base_url, query, max).await?,
            };

            if results.is_empty() {
                return Ok("No results found.".into());
            }

            // Stream structured output for UI: "title\nurl\nsnippet\n\n"
            for r in &results {
                let mut entry = format!("{}\n{}\n", r.title, r.url);
                if !r.snippet.is_empty() {
                    entry.push_str(&format!("{}\n", r.snippet));
                }
                entry.push('\n');
                let _ = _output_tx.send(entry).await;
            }

            // Model-facing result: numbered list
            let mut output = String::new();
            for (i, r) in results.iter().enumerate() {
                output.push_str(&format!("{}. {}\n   {}\n", i + 1, r.title, r.url));
                if !r.snippet.is_empty() {
                    output.push_str(&format!("   {}\n", r.snippet));
                }
                output.push('\n');
            }
            Ok(output)
        })
    }
}

struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

async fn search_exa(api_key: &str, query: &str, max: usize) -> Result<Vec<SearchResult>> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.exa.ai/search")
        .header("x-api-key", api_key)
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "query": query,
            "num_results": max,
            "type": "keyword"
        }))
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;
    let results = body["results"].as_array()
        .map(|arr| arr.iter().map(|r| SearchResult {
            title: r["title"].as_str().unwrap_or("").to_owned(),
            url: r["url"].as_str().unwrap_or("").to_owned(),
            snippet: r["text"].as_str().unwrap_or("").chars().take(200).collect(),
        }).collect())
        .unwrap_or_default();
    Ok(results)
}

async fn search_tavily(api_key: &str, query: &str, max: usize) -> Result<Vec<SearchResult>> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.tavily.com/search")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "api_key": api_key,
            "query": query,
            "max_results": max,
        }))
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;
    let results = body["results"].as_array()
        .map(|arr| arr.iter().map(|r| SearchResult {
            title: r["title"].as_str().unwrap_or("").to_owned(),
            url: r["url"].as_str().unwrap_or("").to_owned(),
            snippet: r["content"].as_str().unwrap_or("").chars().take(200).collect(),
        }).collect())
        .unwrap_or_default();
    Ok(results)
}

async fn search_searxng(base_url: &str, query: &str, max: usize) -> Result<Vec<SearchResult>> {
    let client = reqwest::Client::new();
    let url = format!("{}/search", base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .query(&[("q", query), ("format", "json"), ("pageno", "1")])
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;
    let results = body["results"].as_array()
        .map(|arr| arr.iter().take(max).map(|r| SearchResult {
            title: r["title"].as_str().unwrap_or("").to_owned(),
            url: r["url"].as_str().unwrap_or("").to_owned(),
            snippet: r["content"].as_str().unwrap_or("").chars().take(200).collect(),
        }).collect())
        .unwrap_or_default();
    Ok(results)
}
