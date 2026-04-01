---
name: create-skill
description: Create a new oxshell skill from a natural language description
when_to_use: When the user asks to create, make, build, or define a new skill, automation, or reusable workflow
allowed-tools: [bash, file_read, file_write, file_edit, glob]
context: inline
user-invocable: true
---

# Create a New oxshell Skill

The user wants to create a new custom skill. Follow these steps precisely:

## Step 1: Understand the request

The user said: **$ARGUMENTS**

Analyze what the user wants the skill to do. Identify:
- What the skill should accomplish (the core action)
- What tools it needs (bash, file_read, file_write, file_edit, glob, grep)
- Whether it should run inline (fast, inject prompt into conversation) or forked (isolated, multi-step with own tool loop)
- What arguments the user might pass when invoking it

## Step 2: Choose a name

Pick a short, kebab-case name that describes the action (e.g., `fix-lint`, `add-test`, `deploy`, `explain-error`).

## Step 3: Create the skill directory and SKILL.md

Use the `file_write` tool to create the skill file at:
```
.oxshell/skills/<name>/SKILL.md
```

The SKILL.md must follow this exact format:

```markdown
---
name: <kebab-case-name>
description: <one-line description of what the skill does>
when_to_use: <when the model should auto-invoke this skill>
allowed-tools: [<comma-separated list of tools the skill needs>]
context: <inline or fork>
---

<The actual prompt that will be executed when the skill runs>
<Use $ARGUMENTS for the full user input>
<Use $1, $2, etc. for positional arguments>
<Use ${SKILL_DIR} to reference files in the skill's directory>
```

### Rules for writing good skill prompts:

1. **Be specific** — Tell the model exactly what steps to follow
2. **Use numbered steps** — Break complex tasks into clear phases
3. **Specify tool usage** — Say "Use the bash tool to run..." or "Read the file with file_read"
4. **Handle errors** — Include "If X fails, then Y" instructions
5. **Keep it focused** — One skill = one job done well
6. **Use $ARGUMENTS** — So the user can customize behavior at runtime

### Context selection guide:
- Use `inline` for: simple prompts, explanations, code generation (fast, shared context)
- Use `fork` for: multi-step workflows that use tools (isolated, up to 10 turns)

## Step 4: Verify the skill was created

Use `bash` to verify the file exists:
```bash
cat .oxshell/skills/<name>/SKILL.md
```

## Step 5: Report to the user

Tell the user:
- The skill name and how to invoke it: `/<name>` or `/<name> <args>`
- What it does
- What tools it uses
- Whether it runs inline or forked
- Remind them they can edit `.oxshell/skills/<name>/SKILL.md` to customize it

If the user didn't provide enough details, ask clarifying questions before creating the skill.
