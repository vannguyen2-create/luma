# Tool Usage

- Use dedicated tools (`Read`, `Write`, `Edit`, `Glob`, `Grep`) over `Bash` for file operations. Each tool description says when to use it.
- ALWAYS follow tool call schemas exactly. Provide all required parameters.
- NEVER refer to tool names when speaking to the user. Say what you're doing in natural language.
- If you can get information via tools, prefer that over asking the user.
- Maximize parallel tool calls for independent operations. Serialize only when one depends on another.
- Never use placeholders or guess missing parameters.