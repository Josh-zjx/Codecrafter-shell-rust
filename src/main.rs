use std::path::{Path, PathBuf};
use std::process::exit;
use std::process::Command;
use std::{env, fs};
//use std::io::prelude::*;
#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    let stdin = io::stdin();
    let mut input = String::new();
    print!("$ ");
    io::stdout().flush().unwrap();
    while stdin.read_line(&mut input).is_ok() {
        let input_string = input.strip_suffix('\n').unwrap();
        if let Some(command) = parse_input(input_string) {
            match command {
                ShellCommand::EXIT(val) => exit(val),
                ShellCommand::ECHO(argument) => {
                    println!("{}", argument);
                }
                ShellCommand::TYPE(argument) => match type_of_command(argument) {
                    CommandType::Builtin => {
                        println!("{} is a shell builtin", argument);
                    }
                    CommandType::Program(path) => {
                        println!("{} is {}", argument, path.to_str().unwrap());
                    }
                    CommandType::Nonexistent => {
                        println!("{} not found", argument);
                    }
                },
                ShellCommand::Program((command, arguments)) => {
                    let command_type = type_of_command(command);
                    match command_type {
                        CommandType::Nonexistent => {
                            println!("{}: command not found", input_string);
                        }
                        CommandType::Program(path) => {
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

#[derive(Debug, Clone)]
pub enum ShellCommand<'a> {
    EXIT(i32),
    ECHO(&'a str),
    TYPE(&'a str), //ERROR(&'a str),
    Program((&'a str, &'a str)),
}

fn parse_input(input: &str) -> Option<ShellCommand> {
    let (command, arguments) = input.split_once(' ')?;
    match command {
        "exit" => Some(ShellCommand::EXIT(arguments.parse::<i32>().unwrap())),
        "echo" => Some(ShellCommand::ECHO(arguments)),
        "type" => Some(ShellCommand::TYPE(arguments)),
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
        _default => {
            if let Ok(path) = env::var("PATH") {
                let paths: Vec<&str> = path.split(':').collect();
                for path in paths.iter() {
                    let folder = fs::read_dir(path).unwrap();
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
