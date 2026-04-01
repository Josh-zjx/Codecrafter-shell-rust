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
pub enum ShellCommand {
    Empty(),
    Exit(i32),
    Echo(Vec<String>, String, String, bool, bool), // (arguments, stdout, stderr, stdout_append, stderr_append)
    Cd(String),                                    // (path)
    Type(String),                                  //ERROR(&'a str),
    Pwd(),
    Program(String, Vec<String>, String, String, bool, bool), // (command, arguments, stdout, stderr, stdout_append, stderr_append)
}

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
