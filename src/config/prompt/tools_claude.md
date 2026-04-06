# Tool Usage

- Use dedicated tools (`Read`, `Write`, `Edit`, `Glob`, `Grep`) over `Bash` for file operations. Each tool description says when to use it.
- Maximize parallel tool calls for independent operations. Only serialize when one depends on another.
- Never use placeholders or guess missing parameters.