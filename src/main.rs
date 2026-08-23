mod args;
mod export;

use args::Args;
use m_parser::{MParserError, Statement};
use std::{
    io::{self, Write},
    process::ExitCode,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), MParserError> {
    // Parse command line arguments
    let args: Args = Args::new();

    if args.format.requires_output_path() {
        return Err(MParserError::OutputPathRequired);
    }

    // obtain the parsed statement
    let stmt = Statement::new(args.statement_path, args.password.as_deref())?;

    // export mpesa statement into specified format
    let output = export::run(stmt, &args.format)?;

    if let Some(output_path) = args.output_path {
        std::fs::write(&output_path, &output)?;
        println!(
            "{} output written to {}",
            output.len(),
            output_path.display()
        );
    } else {
        write_bytes_to_stdout(&output)?;
    }

    Ok(())
}

fn write_bytes_to_stdout(bytes: &[u8]) -> io::Result<()> {
    let stdout = io::stdout();
    let mut lock = stdout.lock();
    lock.write_all(bytes)
}
