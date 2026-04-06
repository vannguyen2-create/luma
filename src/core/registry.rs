/// Tool registry — maps tool names to implementations.
use std::collections::HashMap;
use crate::core::tool::Tool;
use crate::core::types::ToolSchema;

/// Stores registered tools by name.
pub struct Registry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl Registry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { tools: HashMap::new() }
    }

    /// Register a tool by its name.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_owned(), tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// All tool schemas for the current provider.
    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| t.schema()).collect()
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
        fn name(&self) -> &str { "Read" }
        fn schema(&self) -> ToolSchema {
            ToolSchema {
                name: "Read".into(),
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
        assert!(reg.get("Read").is_some());
        assert!(reg.get("missing").is_none());
    }

    #[test]
    fn schemas_use_tool_names() {
        let mut reg = Registry::new();
        reg.register(Box::new(FakeTool));
        let schemas = reg.schemas();
        assert_eq!(schemas[0].name, "Read");
    }
}
