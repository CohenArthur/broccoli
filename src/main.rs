mod args;
mod block;
mod instruction;
mod interpreter;
mod parser;
mod value;

use args::Args;

fn main() {
    let args = Args::handle();

    println!("{:#?}", args.input);
}