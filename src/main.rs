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
        let command = parse_input(&input);
        match command {
            Command::ERROR(name) => {
                println!("{}: command not found", name);
            }
            Command::EXIT(val) => exit(val),
            _default => {}
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
    ERROR(&'a str),
}

fn parse_input(input: &str) -> Command {
    let input = input.strip_suffix('\n').unwrap();
    let input_group: Vec<_> = input.split(' ').collect();
    match *input_group.first().unwrap() {
        "exit" => Command::EXIT(input_group.get(1).unwrap().parse::<i32>().unwrap()),
        _default => Command::ERROR(input),
    }
    //Command::ERROR(input)
}
