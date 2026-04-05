/// Tool registry — maps canonical names to implementations, with provider wire name mapping.
use std::collections::HashMap;
use crate::core::tool::Tool;
use crate::core::types::ToolSchema;

/// Maps canonical tool names to provider-specific wire names.
pub fn wire_name_map(source: &str) -> HashMap<&'static str, &'static str> {
    match source {
        "anthropic" => HashMap::from([
            ("read", "Read"),
            ("write", "Write"),
            ("edit", "Edit"),
            ("bash", "Bash"),
            ("glob", "Glob"),
            ("grep", "Grep"),
        ]),
        "codex" => HashMap::from([
            ("bash", "exec_command"),
            ("apply_patch", "apply_patch"),
        ]),
        _ => HashMap::from([
            ("read", "read"),
            ("write", "write"),
            ("edit", "edit"),
            ("bash", "bash"),
            ("glob", "glob"),
            ("grep", "grep"),
        ]),
    }
}

/// Stores registered tools by canonical name.
pub struct Registry {
    tools: HashMap<String, Box<dyn Tool>>,
    wire_to_canonical: HashMap<String, String>,
    canonical_to_wire: HashMap<String, String>,
}

impl Registry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            wire_to_canonical: HashMap::new(),
            canonical_to_wire: HashMap::new(),
        }
    }

    /// Set wire name mapping for a provider source.
    pub fn set_wire_names(&mut self, source: &str) {
        self.wire_to_canonical.clear();
        self.canonical_to_wire.clear();
        for (canonical, wire) in wire_name_map(source) {
            self.wire_to_canonical.insert(wire.to_owned(), canonical.to_owned());
            self.canonical_to_wire.insert(canonical.to_owned(), wire.to_owned());
        }
    }

    /// Register a tool by canonical name.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_owned(), tool);
    }

    /// Look up a tool by wire name (from API response) or canonical name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        // Try wire → canonical first, then direct canonical
        let canonical = self.wire_to_canonical.get(name)
            .map(|s| s.as_str())
            .unwrap_or(name);
        self.tools.get(canonical).map(|t| t.as_ref())
    }

    /// All tool schemas with wire names for the current provider.
    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| {
            let mut schema = t.schema();
            if let Some(wire) = self.canonical_to_wire.get(&schema.name) {
                schema.name = wire.clone();
            }
            schema
        }).collect()
    }

    /// Number of registered tools.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tool::Tool;
    use crate::core::types::ToolSchema;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    struct FakeTool;

    impl Tool for FakeTool {
        fn name(&self) -> &str { "read" }
        fn schema(&self) -> ToolSchema {
            ToolSchema {
                name: "read".into(),
                description: "read a file".into(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            }
        }
        fn execute(
            &self,
            _args: serde_json::Value,
            _output_tx: mpsc::Sender<String>,
            _cancel: CancellationToken,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>> {
            Box::pin(async { Ok("done".into()) })
        }
    }

    #[test]
    fn register_and_lookup() {
        let mut reg = Registry::new();
        assert!(reg.is_empty());
        reg.register(Box::new(FakeTool));
        assert_eq!(reg.len(), 1);
        assert!(reg.get("read").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn wire_name_mapping_claude() {
        let mut reg = Registry::new();
        reg.register(Box::new(FakeTool));
        reg.set_wire_names("anthropic");

        // Schema uses wire name
        let schemas = reg.schemas();
        assert_eq!(schemas[0].name, "Read");

        // Lookup by wire name finds canonical tool
        assert!(reg.get("Read").is_some());
        // Lookup by canonical also works
        assert!(reg.get("read").is_some());
    }

    #[test]
    fn wire_name_mapping_codex() {
        let mut reg = Registry::new();
        // Codex only has bash + apply_patch
        struct FakeBash;
        impl Tool for FakeBash {
            fn name(&self) -> &str { "bash" }
            fn schema(&self) -> ToolSchema {
                ToolSchema { name: "bash".into(), description: "shell".into(), parameters: serde_json::json!({}) }
            }
            fn execute(&self, _: serde_json::Value, _: mpsc::Sender<String>, _: CancellationToken)
                -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<String>> + Send + '_>>
            { Box::pin(async { Ok("ok".into()) }) }
        }
        reg.register(Box::new(FakeBash));
        reg.set_wire_names("codex");

        let schemas = reg.schemas();
        assert_eq!(schemas[0].name, "exec_command");
        assert!(reg.get("exec_command").is_some());
        assert!(reg.get("bash").is_some());
    }
}
