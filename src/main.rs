use std::path::{Path, PathBuf};
use std::process::exit;
use std::process::Command;
use std::{env, fs};
//use std::io::prelude::*;
#[allow(unused_imports)]
use std::io::{self, Write};

// TOKENIZER ANALYSIS — Location A: New module-level tokenizer (e.g. `mod tokenizer;`)
//
// A tokenizer/lexer could be introduced as a separate module (`src/tokenizer.rs` or
// `src/lexer.rs`) imported here. This is the top of the file where modules are declared.
//
// Pros:
//   - Clean separation of concerns; parsing logic doesn't pollute main.rs.
//   - Independently testable — unit tests can live inside the module.
//   - Follows Rust idioms: a dedicated module with its own types (Token enum, Lexer struct).
//   - Easy to extend later for pipes, redirection, escapes, etc.
//   - Does not touch existing code structure; callers simply use the new API.
//
// Cons:
//   - Requires changing the ShellCommand enum and parse_input() signature to accept
//     a Vec<String> (tokens) instead of raw &str, which touches many call sites.
//   - Slightly more complex project structure for a small codebase.
//   - Adds a new file that must be maintained.
//
// Verdict: RECOMMENDED — best long-term choice. Keeps concerns separated and testable.

fn main() {
    let stdin = io::stdin();
    let mut input = String::new();
    print!("$ ");
    io::stdout().flush().unwrap();
    while stdin.read_line(&mut input).is_ok() {
        let input_string = input.strip_suffix('\n').unwrap();

        // TOKENIZER ANALYSIS — Location B: Between input reading and parse_input()
        //
        // A tokenizer could be called right here, converting `input_string` into a
        // Vec<String> of tokens before passing them to parse_input().
        //   e.g.:  let tokens = tokenize(input_string);
        //          if let Some(command) = parse_input_from_tokens(&tokens) { ... }
        //
        // Pros:
        //   - Natural pipeline: read → tokenize → parse → execute.
        //   - Tokenization happens once, all downstream code works with clean tokens.
        //   - Keeps main() as the orchestrator, easy to follow the flow.
        //   - Quoted strings are resolved before any command matching happens.
        //
        // Cons:
        //   - Requires changing parse_input() to accept tokens (Vec<String>) instead of &str.
        //   - ShellCommand enum must change from &str to String (owned) for token data.
        //   - All command handlers (echo, cd, type, etc.) would need updated signatures.
        //
        // Verdict: RECOMMENDED — cleanest integration point in the main loop.

        if let Some(command) = parse_input(input_string) {
            match command {
                ShellCommand::EXIT(val) => exit(val),
                ShellCommand::ECHO(argument) => {
                    // TOKENIZER ANALYSIS — Location C: Inside ECHO handler
                    //
                    // Currently, `argument` is the raw string after "echo ".
                    // With quotes, `echo "hello   world"` must preserve inner spaces and
                    // strip the quotes. This logic could live here, per-command.
                    //
                    // Pros:
                    //   - Minimal change — only touch ECHO, which is the most quote-sensitive
                    //     builtin.
                    //   - No changes to parse_input() or ShellCommand enum.
                    //
                    // Cons:
                    //   - Duplicates quote-handling logic if other commands also need it (they do).
                    //   - Violates DRY: cd, type, and external programs all need quoting too.
                    //   - Becomes unmaintainable as more quoting rules are added.
                    //   - Doesn't handle cases where the command name itself is quoted.
                    //
                    // Verdict: NOT RECOMMENDED — only viable as a quick throwaway hack.
                    println!("{}", argument);
                }
                ShellCommand::TYPE(argument) => handle_command_type(argument),
                ShellCommand::PWD() => {
                    println!("{}", std::env::current_dir().unwrap().to_str().unwrap())
                }
                ShellCommand::CD(argument) => handle_command_cd(argument),
                ShellCommand::Program((command, arguments)) => {
                    let command_type = type_of_command(command);
                    match command_type {
                        CommandType::Nonexistent => {
                            println!("{}: command not found", input_string);
                        }
                        CommandType::Program(path) => {
                            // TOKENIZER ANALYSIS — Location D: arguments.split(' ') call
                            //
                            // This is where arguments are split into individual args for
                            // external programs. Currently uses naive space-splitting,
                            // which breaks quoted arguments like:
                            //   cat "file with spaces.txt"  →  ["file", "with", "spaces.txt"]
                            //
                            // A tokenizer could replace `arguments.split(' ')` with
                            // pre-tokenized Vec<String> passed down from parse_input().
                            //
                            // Pros:
                            //   - Directly fixes the most visible quoting bug for external
                            //     programs.
                            //   - Minimal change if only external programs need fixing.
                            //
                            // Cons:
                            //   - Only fixes external programs; echo, cd, type still broken.
                            //   - Patching here means tokenization is buried inside execution,
                            //     not in the parsing layer where it belongs.
                            //   - If ShellCommand::Program still holds raw &str, you'd need
                            //     to re-tokenize here on every execution.
                            //   - Harder to test in isolation.
                            //
                            // Verdict: NOT RECOMMENDED as sole location — the fix belongs
                            //   upstream in the parser so all commands benefit.
                            let output = Command::new(path)
                                .args(arguments.split(' '))
                                .output()
                                .expect("fail to run program");
                            print!("{}", String::from_utf8_lossy(&output.stdout))
                        }
                        CommandType::Builtin => {}
                    };
                }
            }
        } else {
            println!("{}: command not found", input_string);
        }
        input.clear();
        print!("$ ");
        io::stdout().flush().unwrap();
    }
}

fn handle_command_type(argument: &str) {
    match type_of_command(argument) {
        CommandType::Builtin => {
            println!("{} is a shell builtin", argument);
        }
        CommandType::Program(path) => {
            println!("{} is {}", argument, path.to_str().unwrap());
        }
        CommandType::Nonexistent => {
            println!("{} not found", argument);
        }
    };
}

fn handle_command_cd(argument: &str) {
    if let Some(path) = argument.strip_prefix('~') {
        if std::env::set_current_dir(Path::new(&(std::env::var("HOME").unwrap() + path))).is_err() {
            println!("cd: {}: No such file or directory", argument);
        }
    } else if std::env::set_current_dir(Path::new(argument)).is_err() {
        println!("cd: {}: No such file or directory", argument);
    };
}

// TOKENIZER ANALYSIS — Location E: ShellCommand enum definition
//
// Currently, most variants hold `&'a str` — a borrowed slice of the raw input.
// To support quoting, tokens often need to be *owned* Strings (because quotes are
// stripped, escape sequences are resolved, and adjacent tokens are concatenated).
//
// This enum would need to change to support tokenized input. Two options:
//
//   Option E1: Change &str → String  (e.g., ECHO(String), CD(String))
//     Pros: Simple, direct, each variant owns its data.
//     Cons: Allocations for every command; lifetime simplicity traded for heap usage.
//
//   Option E2: Change &str → Vec<String>  (e.g., ECHO(Vec<String>), Program(String, Vec<String>))
//     Pros: Each variant gets a proper list of tokens — mirrors argc/argv.
//     Cons: More invasive change; all match arms need updating.
//
// Verdict: Changing this enum is REQUIRED regardless of where the tokenizer lives.
//   Option E2 (Vec<String>) is preferred because it models shell semantics correctly.
#[derive(Debug, Clone)]
pub enum ShellCommand<'a> {
    EXIT(i32),
    ECHO(&'a str),
    CD(&'a str),
    TYPE(&'a str), //ERROR(&'a str),
    PWD(),
    Program((&'a str, &'a str)),
}

// TOKENIZER ANALYSIS — Location F: Inside parse_input() function
//
// This is the current "parser." It splits on the first space and matches the command
// name. A tokenizer/lexer could be embedded directly inside this function.
//
//   e.g.:  let tokens = tokenize(input);  // handle quotes here
//          let command = &tokens[0];
//          let arguments = &tokens[1..];
//          match command.as_str() { ... }
//
// Pros:
//   - Contained change — only this function is modified.
//   - No new files or modules needed; quick to implement.
//   - The function already "owns" the parsing responsibility.
//
// Cons:
//   - Mixes tokenization and command dispatch in one function (two responsibilities).
//   - Harder to unit test the tokenizer independently.
//   - As the tokenizer grows (escape sequences, heredocs, pipes, etc.), this function
//     becomes bloated and hard to read.
//   - Still requires ShellCommand enum changes to hold Vec<String>.
//
// Verdict: ACCEPTABLE for a first iteration, but should be refactored into a
//   separate function or module as complexity grows.
fn parse_input(input: &str) -> Option<ShellCommand> {
    // TOKENIZER ANALYSIS — Location F-detail: The split point
    //
    // `input.split_once(' ')` is the current "tokenizer." It only handles one split.
    // This is the exact line that must be replaced by a proper tokenizer that is
    // aware of quoting rules:
    //   - Single quotes: preserve literal content, no variable expansion
    //   - Double quotes: preserve spaces but allow $variable expansion
    //   - Unquoted: split on whitespace
    //
    // The replacement would be:
    //   let tokens: Vec<String> = tokenize(input);
    //   let command = tokens.first()?;
    //   let arguments = &tokens[1..];
    let (command, arguments) = match input.find(' ') {
        Some(_index) => input.split_once(' ')?,
        None => (input, ""),
    };

    match command {
        "exit" => Some(ShellCommand::EXIT(arguments.parse::<i32>().unwrap())),
        "echo" => Some(ShellCommand::ECHO(arguments)),
        "type" => Some(ShellCommand::TYPE(arguments)),
        "pwd" => Some(ShellCommand::PWD()),
        "cd" => Some(ShellCommand::CD(arguments)),
        _default => Some(ShellCommand::Program((command, arguments))),
    }
}

#[derive(Debug, Clone)]
pub enum CommandType {
    Builtin,
    Nonexistent,
    Program(PathBuf),
}

fn type_of_command(command: &str) -> CommandType {
    match command {
        "echo" => CommandType::Builtin,
        "exit" => CommandType::Builtin,
        "type" => CommandType::Builtin,
        "pwd" => CommandType::Builtin,
        "cd" => CommandType::Builtin,
        _default => {
            if let Ok(path) = env::var("PATH") {
                let paths: Vec<&str> = path.split(':').collect();
                for path in paths.iter() {
                    //println!("{}", path);
                    let folder = match fs::read_dir(path) {
                        Ok(fold) => fold,
                        Err(_err) => continue,
                    };
                    for item in folder.into_iter() {
                        if item.as_ref().unwrap().file_name() == command {
                            return CommandType::Program(item.unwrap().path());
                        }
                    }
                }
                let full_path = Path::new(command);
                if full_path.exists() {
                    return CommandType::Program(full_path.to_path_buf());
                }
                CommandType::Nonexistent
            } else {
                CommandType::Nonexistent
            }
        }
    }
}
