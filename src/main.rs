use std::process::exit;
//use std::io::prelude::*;
#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.
    //println!("Logs from your program will appear here!");

    // Uncomment this block to pass the first stage

    // Wait for user input
    let stdin = io::stdin();
    let mut input = String::new();
    print!("$ ");
    io::stdout().flush().unwrap();
    while stdin.read_line(&mut input).is_ok() {
        let input_string = input.strip_suffix('\n').unwrap();
        if let Some(command) = parse_input(&input_string) {
            match command {
                Command::EXIT(val) => exit(val),
                Command::ECHO(argument) => {
                    println!("{}", argument);
                } //_default => {}
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
pub enum Command<'a> {
    EXIT(i32),
    ECHO(&'a str),
    //ERROR(&'a str),
}

fn parse_input(input: &str) -> Option<Command> {
    let (command, arguments) = input.split_once(' ')?;
    match command {
        "exit" => Some(Command::EXIT(arguments.parse::<i32>().unwrap())),
        "echo" => Some(Command::ECHO(arguments)),
        _default => None,
    }
    //Command::ERROR(input)
}
