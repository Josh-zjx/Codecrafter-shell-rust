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
        println!("{}: command not found", input.strip_suffix("\n").unwrap());
        input.clear();
        print!("$ ");
        io::stdout().flush().unwrap();
    }
}
