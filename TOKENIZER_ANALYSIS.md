# Tokenizer/Lexer Implementation Analysis

## Purpose

This document analyzes all potential locations in the codebase where a tokenizer/lexer
could be introduced to support **single-quote** and **double-quote** parsing in the shell.

The shell currently has **one source file** (`src/main.rs`, ~143 lines of functional code)
with no modules, no tests, and a simple split-on-first-space parser.

---

## Background: Current Parsing Flow

```
stdin.read_line()          →  raw line (String)
  ↓
strip_suffix('\n')         →  input_string (&str)
  ↓
parse_input(input_string)  →  Option<ShellCommand>
  ↓                              ↓
  ↓                        split_once(' ') splits into (command, arguments)
  ↓                        match command name → return ShellCommand variant
  ↓
match ShellCommand         →  execute builtin or external program
  ↓
arguments.split(' ')       →  (only for external programs) naive arg splitting
```

**Key problem:** There is no tokenizer. The input is split once on the first space
character to separate the command name from its arguments. For external programs,
arguments are further split on every space. Quoted strings are not handled at all.

---

## Quoting Rules to Support

| Quote Type     | Splits on Space? | Variable Expansion? | Escape Sequences? |
|----------------|-------------------|---------------------|--------------------|
| No quotes      | Yes               | Yes (future)        | Yes (future)       |
| Single quotes  | No                | No                  | No                 |
| Double quotes  | No                | Yes (future)        | Partial (`\"`, `\\`, `\$`) |

---

## Analyzed Locations

Six locations (A–F) were identified and annotated inline in `src/main.rs`.
Each is assessed below with a detailed pros/cons analysis.

---

### Location A — New Separate Module (`src/tokenizer.rs`)

**Where:** Declared at the top of `src/main.rs` as `mod tokenizer;`, implemented in
a new file `src/tokenizer.rs` (or `src/lexer.rs`).

**How it would work:**
```rust
// src/main.rs
mod tokenizer;
use tokenizer::tokenize;

// In main loop:
let tokens = tokenize(input_string);  // Vec<String>
if let Some(command) = parse_input(&tokens) { ... }
```

**Pros:**
- **Separation of concerns** — tokenization logic is fully isolated from command
  dispatch and execution.
- **Independently testable** — `#[cfg(test)]` module inside `tokenizer.rs` can
  exhaustively test quoting edge cases without running the full shell.
- **Idiomatic Rust** — a dedicated module with its own types (`Token` enum,
  `Lexer` struct with iterator) follows standard Rust project conventions.
- **Extensible** — when pipes (`|`), redirections (`>`, `>>`, `<`), command
  substitution (`` `...` ``), or escape sequences need to be added later, the
  tokenizer module grows naturally without bloating `main.rs`.
- **Reusable** — other parts of the codebase (e.g., tab completion, syntax
  highlighting) could use the same tokenizer.

**Cons:**
- **Broader refactor required** — `ShellCommand` enum must change from `&str` to
  `String` or `Vec<String>`. `parse_input()` signature changes. All command
  handlers need updating.
- **More files to maintain** — adds a file to a currently single-file project.
- **Over-engineering risk** — for a CodeCrafters exercise, this level of structure
  may be unnecessary.

**Verdict: ⭐ BEST CHOICE for production-quality or long-lived code.**

---

### Location B — Between Input Reading and `parse_input()` (main loop, line 37)

**Where:** In `main()`, between `let input_string = ...` and the call to `parse_input()`.

**How it would work:**
```rust
let input_string = input.strip_suffix('\n').unwrap();
let tokens = tokenize(input_string);   // ← NEW
if let Some(command) = parse_input_from_tokens(&tokens) { ... }
```

**Pros:**
- **Clean pipeline** — the flow becomes `read → tokenize → parse → execute`,
  which is easy to understand and matches how real shells work.
- **Single tokenization pass** — tokens are computed once and passed down,
  avoiding redundant work.
- **All commands benefit** — echo, cd, type, and external programs all receive
  properly tokenized input.

**Cons:**
- **Same refactor as Location A** — `parse_input()` and `ShellCommand` must change.
- **Tokenizer function lives in `main.rs`** — unless combined with Location A,
  the tokenization logic is defined in `main.rs`, mixing concerns.
- **Not independently testable** (unless extracted into a function or module).

**Verdict: ⭐ RECOMMENDED — this is the natural *call site* for any tokenizer.
Best combined with Location A (module definition) to get both clean call site
and clean implementation.**

---

### Location C — Inside the ECHO Command Handler (line 61 area)

**Where:** Inside the `ShellCommand::ECHO(argument)` match arm in `main()`.

**How it would work:**
```rust
ShellCommand::ECHO(argument) => {
    let processed = strip_quotes(argument);  // ← NEW
    println!("{}", processed);
}
```

**Pros:**
- **Minimal change** — only the ECHO handler is modified.
- **No changes to `parse_input()` or `ShellCommand`**.
- **Quick to implement** for a narrow fix.

**Cons:**
- **Violates DRY** — quote stripping must be duplicated in every command handler
  that needs it (cd, type, external programs).
- **Incomplete solution** — doesn't handle cases where the *command name* is quoted
  (e.g., `'echo' hello`), or where a single argument contains a mix of quoted and
  unquoted parts (e.g., `echo hello" "world`).
- **Doesn't split properly** — `echo 'hello'  'world'` needs tokenization to
  produce two separate tokens, but `argument` is still one raw string.
- **Unmaintainable** — as more quoting rules are added, per-handler logic becomes
  a mess of special cases.

**Verdict: ❌ NOT RECOMMENDED — only works as a throwaway hack for a single
test case. Does not generalize.**

---

### Location D — At `arguments.split(' ')` (external program execution, line 93)

**Where:** Inside the `ShellCommand::Program` handler, where arguments are split
for `Command::new(path).args(...)`.

**How it would work:**
```rust
// Instead of:
.args(arguments.split(' '))
// Use:
.args(tokenize_arguments(arguments))  // ← quote-aware splitting
```

**Pros:**
- **Directly fixes the most user-visible bug** — external programs receive
  correctly quoted arguments.
- **Small, contained change** — only one line is affected.

**Cons:**
- **Only fixes external programs** — builtin commands (echo, cd, type) still
  receive raw unsplit arguments and must handle quoting separately.
- **Tokenization in the wrong layer** — argument splitting is an execution concern
  here, but tokenization is a parsing concern. Mixing them makes the architecture
  harder to reason about.
- **Re-tokenizes on every call** — if `ShellCommand::Program` still holds raw `&str`,
  tokenization happens at execution time rather than parse time.
- **Doesn't handle quoted command names** — `'cat' file.txt` would fail before
  reaching this code because `parse_input()` wouldn't recognize `'cat'`.
- **Harder to test in isolation** — testing requires setting up `CommandType::Program`
  and a real or mock executable.

**Verdict: ❌ NOT RECOMMENDED as the sole location. The fix belongs upstream.**

---

### Location E — `ShellCommand` Enum Modification (line 161)

**Where:** The `ShellCommand` enum definition.

**This is not a tokenizer location per se, but a required change for any tokenizer.**

**Current state:**
```rust
pub enum ShellCommand<'a> {
    EXIT(i32),
    ECHO(&'a str),        // raw argument string
    CD(&'a str),
    TYPE(&'a str),
    PWD(),
    Program((&'a str, &'a str)),  // (command, raw args)
}
```

**Option E1 — `&str` → `String`:**
```rust
pub enum ShellCommand {
    EXIT(i32),
    ECHO(String),
    CD(String),
    TYPE(String),
    PWD,
    Program(String, String),
}
```
- Removes lifetime parameter (simpler).
- Each variant holds one combined argument string (quote-stripped but not split).
- Still can't represent multiple arguments as separate tokens.

**Option E2 — `&str` → `Vec<String>`:**
```rust
pub enum ShellCommand {
    EXIT(i32),
    ECHO(Vec<String>),
    CD(Vec<String>),
    TYPE(Vec<String>),
    PWD,
    Program(String, Vec<String>),
}
```
- Removes lifetime parameter.
- Each variant holds a list of fully tokenized, quote-stripped arguments.
- Mirrors the traditional `argc`/`argv` model used by all real shells.
- Enables proper multi-argument handling (e.g., `echo hello world` → `["hello", "world"]`).

**Verdict: Option E2 is the correct choice.** Regardless of where the tokenizer
function lives, this enum must change to carry tokenized data.

---

### Location F — Inside `parse_input()` Function (line 189)

**Where:** Embedding tokenization logic directly within the existing `parse_input()`
function body.

**How it would work:**
```rust
fn parse_input(input: &str) -> Option<ShellCommand> {
    let tokens = tokenize(input);   // inline tokenizer call or inline logic
    let command = tokens.first()?;
    let arguments = tokens[1..].to_vec();
    match command.as_str() {
        "exit" => Some(ShellCommand::EXIT(arguments[0].parse().unwrap())),
        "echo" => Some(ShellCommand::ECHO(arguments)),
        ...
    }
}
```

**Pros:**
- **Self-contained** — the function already "owns" parsing, so adding tokenization
  here feels natural.
- **No new files or modules** — keeps the project as a single file.
- **Quick to implement** — `parse_input()` is short and easy to modify.

**Cons:**
- **Single Responsibility violation** — one function does both tokenization AND
  command dispatch. These are separate concerns.
- **Hard to unit test tokenization** — you'd have to test through `parse_input()`,
  which also involves command matching.
- **Scalability problem** — as the tokenizer grows (escape sequences, nested
  quotes, heredocs, glob expansion), this function balloons in size and complexity.
- **Still requires enum changes** — `ShellCommand` must change to `Vec<String>`.

**Verdict: ✅ ACCEPTABLE for a first quick iteration or a CodeCrafters exercise.
Should be refactored into a separate function/module when complexity grows.**

---

## Comparison Matrix

| Criterion                     | A (Module)  | B (Main loop) | C (ECHO)  | D (split)  | F (parse_input) |
|-------------------------------|:-----------:|:--------------:|:---------:|:----------:|:----------------:|
| Separation of concerns        | ⭐⭐⭐      | ⭐⭐           | ⭐        | ⭐         | ⭐⭐              |
| Testability                   | ⭐⭐⭐      | ⭐⭐           | ⭐        | ⭐         | ⭐⭐              |
| Minimizes changes             | ⭐          | ⭐⭐           | ⭐⭐⭐    | ⭐⭐⭐     | ⭐⭐              |
| Covers all commands           | ⭐⭐⭐      | ⭐⭐⭐         | ⭐        | ⭐         | ⭐⭐⭐            |
| Future extensibility          | ⭐⭐⭐      | ⭐⭐           | ⭐        | ⭐         | ⭐                |
| Handles quoted command names  | ⭐⭐⭐      | ⭐⭐⭐         | ❌        | ❌         | ⭐⭐⭐            |
| Implementation effort         | Medium      | Medium         | Low       | Low        | Low–Medium       |

*(⭐ = poor/low, ⭐⭐ = adequate, ⭐⭐⭐ = excellent; ❌ = not supported)*

---

## Recommended Approach

**Combine Locations A + B:**

1. **Create `src/tokenizer.rs`** (Location A) with a `pub fn tokenize(input: &str) -> Vec<String>`
   that implements a character-by-character state machine handling:
   - Unquoted mode: split on whitespace
   - Single-quote mode (`'...'`): accumulate literal characters until closing `'`
   - Double-quote mode (`"..."`): accumulate characters, interpret `\\`, `\"`, `\$`

2. **Call the tokenizer in main()** (Location B) before `parse_input()`:
   ```rust
   let tokens = tokenizer::tokenize(input_string);
   if let Some(command) = parse_input(&tokens) { ... }
   ```

3. **Update `ShellCommand` enum** (Location E, Option E2) to hold `Vec<String>`.

4. **Update `parse_input()`** (Location F) to accept `&[String]` and index into tokens
   instead of calling `split_once(' ')`.

This approach gives the best balance of clean architecture, testability, and
extensibility while keeping the implementation straightforward.

---

## Appendix: Quoting Edge Cases to Consider

When implementing, the tokenizer must handle these cases correctly:

| Input                          | Expected Tokens              | Notes                                  |
|--------------------------------|------------------------------|----------------------------------------|
| `echo hello world`            | `["echo", "hello", "world"]` | Basic whitespace splitting             |
| `echo "hello world"`          | `["echo", "hello world"]`    | Double quotes preserve spaces          |
| `echo 'hello world'`          | `["echo", "hello world"]`    | Single quotes preserve spaces          |
| `echo "hello 'world'"`        | `["echo", "hello 'world'"]`  | Single quotes literal inside doubles   |
| `echo 'hello "world"'`          | `["echo", "hello \"world\""]`| Double quotes are literal inside singles |
| `echo hello"  "world`         | `["echo", "hello  world"]`   | Adjacent quoted/unquoted concatenate   |
| `echo ""`                      | `["echo", ""]`               | Empty double-quoted string is a token  |
| `echo ''`                      | `["echo", ""]`               | Empty single-quoted string is a token  |
| `echo "hello\"world"`         | `["echo", "hello\"world"]`   | Escaped quote inside double quotes     |
| `echo   hello   world  `      | `["echo", "hello", "world"]` | Multiple spaces are one delimiter      |

---

*Analysis performed on the codebase at its current state (single-file shell, ~143 LOC).*
*Inline annotations are present in `src/main.rs` at each analyzed location.*
