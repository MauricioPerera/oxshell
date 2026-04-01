---
name: explain
description: Read a file and explain the code in simple terms
when_to_use: When the user asks to explain, describe, or understand code
allowed-tools: [file_read, glob, grep]
context: inline
---

Read the file at $1 using the file_read tool. Then explain what the code does in simple, clear terms.

Break it down into:
1. **Purpose** — What problem does this code solve?
2. **How it works** — Step by step, what does the code do?
3. **Key patterns** — What design patterns, data structures, or techniques are used?

Keep the explanation beginner-friendly. Use analogies when helpful.
