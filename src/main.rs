// TODO(refactor): This single-file layout is becoming difficult to navigate. Consider
// splitting into focused modules:
//
//   src/
//   ├── main.rs          — REPL setup and top-level command dispatch only
//   ├── completion.rs    — CompletionHelper, Completer impl, and PATH-executable list
//   ├── parser.rs        — tokenizer (parse_input), ShellCommand enum, and redirect extraction
//   ├── commands.rs      — CommandType enum, type_of_command(), handle_command_type(),
//                          handle_command_cd()
//   └── redirect.rs      — open_for_redirect() helper and Redirect struct
//
// Pro: each module has a clear responsibility; unit-tests can target individual modules.
// Con: requires explicit pub re-exports and a small refactor of cross-module references.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::process::Command;
use std::{env, fs};
//use std::io::prelude::*;
#[allow(unused_imports)]
use std::io::{self, Write};

use rustyline::completion::{Completer, Pair};
use rustyline::history::FileHistory;
use rustyline::{config, Context};
use rustyline_derive::{Helper, Highlighter, Hinter, Validator};

// TODO(refactor): Consider moving CompletionHelper and its Completer impl to
// src/completion.rs.
//
// Pro: tab-completion is a self-contained concern that can evolve independently;
//      the PATH-executable list built in main() could be encapsulated here too,
//      eliminating the duplication with type_of_command() (see comment there).
// Con: requires making the struct pub and providing a constructor, and re-exporting
//      the type in main.rs.
//
// Additionally, the file-completion branch (lines below) unwraps DirEntry errors
// inline. Extracting it gives a clean place to introduce proper error handling.
#[derive(Helper, Hinter, Highlighter, Validator)]
struct CompletionHelper {
    builtins: Vec<String>,
}
impl Completer for CompletionHelper {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let start = line[..pos].rfind(' ').map_or(0, |i| i + 1);
        let prefix = &line[start..pos];
        if start == 0 {
            // NOTE: must be command
            let mut candidates: Vec<Pair> = self
                .builtins
                .iter()
                .filter(|cmd| cmd.starts_with(prefix))
                .map(|s| Pair {
                    display: s.to_string(),
                    replacement: s.to_string() + " ",
                })
                .collect();
            candidates.sort_by(|a, b| a.display.cmp(&b.display));
            Ok((start, candidates))
        } else {
            // NOTE: could be arguments

            let seg_start = prefix.rfind('/').map_or(0, |i| i + 1);
            let incomplete_segment = &prefix[seg_start..];
            let mut current_directory = env::current_dir().unwrap();
            let mut prev_segments = "".to_string();
            if seg_start != 0 {
                prev_segments = prefix[0..seg_start].to_string();
                for segment in prev_segments.split('/') {
                    current_directory.push(segment);
                }
            };
            let folder = match fs::read_dir(current_directory) {
                Ok(fold) => fold,
                Err(_err) => return Ok((seg_start, Vec::new())),
            };

            let mut candidates: Vec<Pair> = folder
                .into_iter()
                .filter(|item| {
                    item.as_ref()
                        .unwrap()
                        .file_name()
                        .to_str()
                        .unwrap()
                        .to_string()
                        .starts_with(incomplete_segment)
                })
                .map(|item| {
                    if item.as_ref().unwrap().metadata().unwrap().is_dir() {
                        let name = item
                            .as_ref()
                            .unwrap()
                            .file_name()
                            .to_str()
                            .unwrap()
                            .to_string();
                        Pair {
                            display: name.clone() + "/",
                            replacement: prev_segments.clone() + &name + "/",
                        }
                    } else {
                        let name = item
                            .as_ref()
                            .unwrap()
                            .file_name()
                            .to_str()
                            .unwrap()
                            .to_string();
                        Pair {
                            display: name.clone(),
                            replacement: prev_segments.clone() + &name + " ",
                        }
                    }
                })
                .collect();
            candidates.sort_by(|a, b| a.display.cmp(&b.display));
            Ok((start, candidates))
        }
    }
}

fn main() {
    // TODO(refactor): The executable-program discovery below duplicates the PATH-scanning
    // logic that already exists in type_of_command().  Consider extracting a shared helper,
    // e.g. `fn scan_path_executables() -> Vec<String>`, in src/commands.rs (or
    // src/completion.rs if the list is only needed for tab-completion).
    //
    // Pro: single source of truth for PATH scanning; any fix (e.g. deduplication,
    //      permission checks) automatically benefits both completion and type-checking.
    // Con: the two callers serve slightly different purposes (completion hint list vs.
    //      command-existence check), so the shared function signature needs thought.
    let mut executable_programs: Vec<String> = vec![
        "echo".to_string(),
        "exit".to_string(),
        "pwd".to_string(),
        "cd".to_string(),
        "type".to_string(),
    ];
    if let Ok(path) = env::var("PATH") {
        let paths: Vec<&str> = path.split(':').collect();
        for path in paths.iter() {
            //println!("{}", path);
            let folder = match fs::read_dir(path) {
                Ok(fold) => fold,
                Err(_err) => continue,
            };
            for item in folder.into_iter() {
                let metadata = match fs::metadata(item.as_ref().unwrap().path()) {
                    Ok(meta) => meta,
                    Err(_err) => continue,
                };
                if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
                    executable_programs.push(
                        item.as_ref()
                            .unwrap()
                            .file_name()
                            .to_str()
                            .unwrap()
                            .to_string(),
                    );
                }
            }
        }
    }
    let config = config::Builder::new()
        .completion_type(config::CompletionType::List)
        .build();
    let mut rl = rustyline::Editor::<CompletionHelper, FileHistory>::with_config(config).unwrap();
    rl.set_helper(Some(CompletionHelper {
        builtins: executable_programs,
    }));

    // TODO(refactor): Rustyline is in use for interactive line-editing, but several
    // vanilla-REPL patterns remain:
    //
    // 1. `.expect("no input")` — rustyline::ReadlineError distinguishes Eof (Ctrl-D)
    //    and Interrupted (Ctrl-C) from real I/O errors.  These should be matched and
    //    handled gracefully (e.g. exit on Eof, continue on Interrupted) instead of
    //    panicking.  Example:
    //        match rl.readline("$ ") {
    //            Ok(line)                          => { /* process */ }
    //            Err(ReadlineError::Eof)            => break,
    //            Err(ReadlineError::Interrupted)    => continue,
    //            Err(err)                           => { eprintln!("error: {err}"); break }
    //        }
    //
    // 2. rl.add_history_entry() is never called, so FileHistory never records
    //    commands and the history is always empty.  Add:
    //        let _ = rl.add_history_entry(input_string);
    //    after a successful readline to enable up-arrow recall.
    //
    // 3. The io::stdout().flush() at the bottom of the loop is a leftover from the
    //    vanilla `print!` / `flush` REPL pattern and is not needed when rustyline
    //    manages the terminal.
    loop {
        let input_line = rl.readline("$ ").expect("no input");
        let input_string = input_line.strip_suffix('\n').unwrap_or(&input_line);

        if let Some(command) = parse_input(input_string) {
            match command {
                ShellCommand::Empty() => {
                    // NOTE: Empty command, just do nothing
                }
                ShellCommand::Exit(val) => exit(val),
                ShellCommand::Echo(argument, _rout, _rerr, _rout_append, _rerr_append) => {
                    // TODO(refactor): The fs::OpenOptions block below is duplicated verbatim
                    // in the ShellCommand::Program arm (and again for stderr there).
                    // Consider extracting a helper, e.g.:
                    //   fn open_for_redirect(path: &str, append: bool) -> io::Result<fs::File>
                    // Pro: eliminates ~20 lines of duplication; one place to fix error handling.
                    // Con: minor indirection; the helper would need to live in src/redirect.rs
                    //      or be a free function visible to both call sites.
                    //
                    // NOTE: the stderr branch below only writes an empty file regardless of
                    // any actual stderr content, which differs from the Program arm's behavior.
                    // This inconsistency should be addressed when the helper is extracted.
                    if _rout != "" {
                        // NOTE: stdout redirection
                        let mut f = if _rout_append {
                            fs::OpenOptions::new()
                                .append(true)
                                .create(true)
                                .open(_rout)
                                .expect("Unable to open file")
                        } else {
                            fs::OpenOptions::new()
                                .write(true)
                                .create(true)
                                .open(_rout)
                                .expect("Unable to open file")
                        };

                        f.write_all((argument.join(" ") + "\n").as_bytes())
                            .expect("Unable to write to file");
                    } else {
                        println!("{}", argument.join(" "));
                    }
                    if _rerr != "" {
                        // NOTE: stderr redirection
                        fs::write(_rerr, "").expect("Unable to write to file");
                    }
                }
                ShellCommand::Type(argument) => handle_command_type(argument.as_str()),
                ShellCommand::Pwd() => {
                    println!("{}", std::env::current_dir().unwrap().to_str().unwrap())
                }
                ShellCommand::Cd(argument) => handle_command_cd(argument.as_str()),
                ShellCommand::Program(
                    command,
                    arguments,
                    _rout,
                    _rerr,
                    _rout_append,
                    _rerr_append,
                ) => {
                    let command_type = type_of_command(command.as_str());
                    match command_type {
                        CommandType::Nonexistent => {
                            println!("{}: command not found", input_string);
                        }
                        CommandType::Program(_path) => {
                            let output = Command::new(command)
                                .args(arguments)
                                .output()
                                .expect("fail to run program");
                            // TODO(refactor): The four fs::OpenOptions blocks below (two for
                            // stdout, two for stderr) are structurally identical to the blocks
                            // in the ShellCommand::Echo arm above.  See the comment there for
                            // the suggested open_for_redirect() helper.
                            if _rout != "" {
                                // NOTE: stdout redirection
                                let mut f = if _rout_append {
                                    fs::OpenOptions::new()
                                        .append(true)
                                        .create(true)
                                        .open(_rout)
                                        .expect("Unable to open file")
                                } else {
                                    fs::OpenOptions::new()
                                        .write(true)
                                        .create(true)
                                        .open(_rout)
                                        .expect("Unable to open file")
                                };

                                f.write_all(&output.stdout)
                                    .expect("Unable to write to file");
                            } else {
                                print!("{}", String::from_utf8_lossy(&output.stdout));
                            }
                            if _rerr != "" {
                                // NOTE: stderr redirection
                                let mut f = if _rerr_append {
                                    fs::OpenOptions::new()
                                        .append(true)
                                        .create(true)
                                        .open(_rerr)
                                        .expect("Unable to open file")
                                } else {
                                    fs::OpenOptions::new()
                                        .write(true)
                                        .create(true)
                                        .open(_rerr)
                                        .expect("Unable to open file")
                                };

                                f.write_all(&output.stderr)
                                    .expect("Unable to write to file");
                            } else {
                                print!("{}", String::from_utf8_lossy(&output.stderr));
                            }
                        }
                        CommandType::Builtin => {}
                    };
                }
            }
        } else {
            println!("{}: command not found", input_string);
        }
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

#[derive(Debug, Clone)]
// TODO(refactor): The positional (String, String, bool, bool) redirect parameters in Echo
// and Program are not self-documenting and require readers to count positions.  Consider:
//
//   struct Redirect { path: String, append: bool }
//
//   Echo(Vec<String>, Option<Redirect>, Option<Redirect>)
//   Program(String, Vec<String>, Option<Redirect>, Option<Redirect>)
//
// Pro: intent is clear; extending to stdin redirect or multiple outputs only changes the struct.
// Con: all match arms in main() and parse_input() need updating; a one-time mechanical change.
pub enum ShellCommand {
    Empty(),
    Exit(i32),
    Echo(Vec<String>, String, String, bool, bool), // (arguments, stdout, stderr, stdout_append, stderr_append)
    Cd(String),                                    // (path)
    Type(String),                                  //ERROR(&'a str),
    Pwd(),
    Program(String, Vec<String>, String, String, bool, bool), // (command, arguments, stdout, stderr, stdout_append, stderr_append)
}

// TODO(refactor): Consider moving parse_input() and ShellCommand into src/parser.rs.
//
// Pro: all tokenization and AST-building logic lives in one module; it is the natural
//      place to add future features such as pipes (`|`), `&&`, `||`, or heredocs without
//      cluttering main.rs.
// Con: ShellCommand would need to be pub and re-imported in main.rs; minor boilerplate.
fn parse_input(input: &str) -> Option<ShellCommand> {
    // NOTE: Only space and tab are considered as delimiters right now, other operators might be appended later

    // NOTE: split_once isn't good enough. Need a full tokenizer to split input into strings
    // NOTE: It is OK to do all tokenization here
    let mut single_quote_open = false;
    let mut double_quote_open = false;
    let mut escape_next = false;
    let mut parsed_input: Vec<String> = vec![];
    let mut current_string: String = "".to_string();
    for c in input.chars() {
        if escape_next {
            // FIXME: Double quote escape is limited but not really handled here
            current_string += &c.to_string();
            escape_next = false;
        } else if c == '\'' && !double_quote_open {
            single_quote_open = !single_quote_open;
        } else if c == '\\' && !single_quote_open {
            escape_next = true;
        } else if c == '"' && !single_quote_open {
            double_quote_open = !double_quote_open;
        } else if (c == ' ' || c == '\t') && !single_quote_open && !double_quote_open {
            // FIXME: the union of matching delimiters feels ugly and nonextensible, but it is OK for now
            if !current_string.is_empty() {
                parsed_input.push(current_string.clone());
                current_string.clear();
            }
        } else {
            current_string += &c.to_string();
        }
    }
    if !current_string.is_empty() {
        parsed_input.push(current_string);
    }

    let (command, mut arguments) = if parsed_input.len() > 1 {
        (parsed_input[0].clone(), parsed_input[1..].to_vec())
    } else if parsed_input.len() == 1 {
        (parsed_input[0].clone(), Vec::new())
    } else {
        ("".to_string(), Vec::new())
    };

    match command.as_str() {
        "exit" => Some(ShellCommand::Exit(
            arguments
                .get(0)
                .unwrap_or(&"0".to_string())
                .parse::<i32>()
                .unwrap_or(0),
        )),
        "echo" => {
            // TODO(refactor): The redirect-extraction loop below (stdout then stderr) is
            // duplicated verbatim in the `_default` arm.  Consider extracting a helper:
            //
            //   fn extract_redirect(
            //       args: &mut Vec<String>,
            //       stdout_ops: &[&str],
            //       append_ops: &[&str],
            //   ) -> (String, bool)
            //
            // that scans `args`, removes the operator token and its target, and returns
            // (path, append_flag).  Call it twice per command (once for stdout, once for
            // stderr) to replace ~40 lines of duplicated code.
            //
            // Pro: DRY — any bug fix or extension (e.g. `&>` combined redirect) only needs
            //      to be made once.
            // Con: the helper mutates the argument list in place, so its signature must
            //      make that side-effect clear.
            let mut rout: String = "".to_string();
            let mut rout_append = false;
            let mut rout_i = 0;
            for (i, word) in arguments.iter().enumerate() {
                if word == "1>" || word == ">" {
                    // NOTE: stdout redirection
                    if arguments.len() > i + 1 {
                        rout = arguments[i + 1].clone();
                        rout_i = i;
                        break;
                    }
                } else if word == "1>>" || word == ">>" {
                    if arguments.len() > i + 1 {
                        rout = arguments[i + 1].clone();
                        rout_i = i;
                        rout_append = true;
                        break;
                    }
                }
            }
            if rout != "" {
                arguments.remove(rout_i + 1);
                arguments.remove(rout_i);
            }
            let mut rerr_append = false;
            let mut rerr: String = "".to_string();
            let mut rerr_i = 0;
            for (i, word) in arguments.iter().enumerate() {
                if word == "2>" {
                    // NOTE: stderr redirection
                    if arguments.len() > i + 1 {
                        rerr = arguments[i + 1].clone();
                        rerr_i = i;
                        break;
                    }
                } else if word == "2>>" {
                    // NOTE: stderr redirection
                    if arguments.len() > i + 1 {
                        rerr = arguments[i + 1].clone();
                        rerr_i = i;
                        rerr_append = true;
                        break;
                    }
                }
            }
            if rerr != "" {
                arguments.remove(rerr_i + 1);
                arguments.remove(rerr_i);
            }
            Some(ShellCommand::Echo(
                arguments,
                rout,
                rerr,
                rout_append,
                rerr_append,
            ))
        }
        "type" => Some(ShellCommand::Type(
            arguments.get(0).unwrap_or(&"".to_string()).to_string(),
        )),
        "pwd" => Some(ShellCommand::Pwd()),
        "cd" => Some(ShellCommand::Cd(
            arguments.get(0).unwrap_or(&"".to_string()).to_string(),
        )),
        "" => Some(ShellCommand::Empty()),
        _default => {
            // TODO(refactor): This redirect-extraction block is identical to the one in the
            // "echo" arm above.  See the comment there for the suggested extract_redirect()
            // helper.  Removing this duplication would also make it straightforward to add
            // support for new redirect operators (e.g. `&>`, `<`) in a single location.
            let mut rout: String = "".to_string();
            let mut rout_append = false;
            let mut rout_i = 0;
            for (i, word) in arguments.iter().enumerate() {
                if word == "1>" || word == ">" {
                    // NOTE: stdout redirection
                    if arguments.len() > i + 1 {
                        rout = arguments[i + 1].clone();
                        rout_i = i;
                        break;
                    }
                } else if word == "1>>" || word == ">>" {
                    if arguments.len() > i + 1 {
                        rout = arguments[i + 1].clone();
                        rout_i = i;
                        rout_append = true;
                        break;
                    }
                }
            }
            if rout != "" {
                arguments.remove(rout_i + 1);
                arguments.remove(rout_i);
            }
            let mut rerr_append = false;
            let mut rerr: String = "".to_string();
            let mut rerr_i = 0;
            for (i, word) in arguments.iter().enumerate() {
                if word == "2>" {
                    // NOTE: stderr redirection
                    if arguments.len() > i + 1 {
                        rerr = arguments[i + 1].clone();
                        rerr_i = i;
                        break;
                    }
                } else if word == "2>>" {
                    // NOTE: stderr redirection
                    if arguments.len() > i + 1 {
                        rerr = arguments[i + 1].clone();
                        rerr_i = i;
                        rerr_append = true;
                        break;
                    }
                }
            }
            if rerr != "" {
                arguments.remove(rerr_i + 1);
                arguments.remove(rerr_i);
            }
            Some(ShellCommand::Program(
                command,
                arguments,
                rout,
                rerr,
                rout_append,
                rerr_append,
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub enum CommandType {
    Builtin,
    Nonexistent,
    Program(PathBuf),
}

// TODO(refactor): Consider moving CommandType, type_of_command(), handle_command_type(),
// and handle_command_cd() to src/commands.rs.
//
// Pro: all command-related lookup and dispatch logic is co-located; easier to add new
//      builtins without touching parser or REPL code.
// Con: handle_command_cd() uses std::env::set_current_dir() which affects global process
//      state — document that side-effect clearly in the module.
//
// Additionally, type_of_command() walks every directory entry in every PATH directory on
// each invocation.  For shells with large PATH values this is expensive.  A simple
// HashMap<String, CommandType> cache (populated once at startup, invalidated on PATH
// change) would reduce repeated O(n) filesystem scans to O(1) lookups.
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
                            let metadata = match fs::metadata(item.as_ref().unwrap().path()) {
                                Ok(meta) => meta,
                                Err(_err) => continue,
                            };
                            if metadata.is_file() && metadata.permissions().mode() & 0o111 != 0 {
                                return CommandType::Program(item.unwrap().path());
                            }
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
