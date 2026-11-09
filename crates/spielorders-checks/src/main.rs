use std::error::Error;

mod documents;
mod interface;
mod schemas;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [] => {
            let root = std::env::current_dir()?;
            schemas::check(&root)?;
            documents::check(&root)?;
            interface::check_header(&root)?;
            interface::run_walkthrough(&root)
        }
        [kind] if kind == "header" => interface::check_header(&std::env::current_dir()?),
        [kind, write] if kind == "header" && write == "write" => {
            interface::write_header(&std::env::current_dir()?)
        }
        [kind] if kind == "docs" => documents::check(&std::env::current_dir()?),
        [kind] if kind == "schemas" => schemas::check(&std::env::current_dir()?),
        _ => Err("usage: spielorders-checks [schemas|docs|header [write]]".into()),
    }
}
