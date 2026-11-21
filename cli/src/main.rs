mod args;
mod errors;

use args::Args;
use errors::CliError;
use parser::Statement;
use std::{
    io::{self, Write},
    path::Path,
    process::ExitCode,
};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), CliError> {
    // Parse command line arguments
    let args: Args = Args::new();

    if args.format.requires_output_path() && args.output_path.is_none() {
        return Err(CliError::OutputPathRequired(
            args.format.extension().to_owned(),
        ));
    }

    let output_path = args.output_path.map(|p| {
        if p.is_dir() {
            let file_name = args
                .statement_path
                .file_name()
                .expect("statement path should have a file name");
            p.join(Path::new(file_name).with_extension(args.format.extension()))
        } else {
            p
        }
    });

    // obtain the parsed statement
    let file = std::fs::File::open(&args.statement_path)?;
    let stmt = Statement::new(&file, args.password.as_deref())?;

    // export mpesa statement into specified format
    let output = export::run(stmt, &args.format)?;

    if let Some(output_path) = output_path {
        std::fs::write(&output_path, &output)?;
        println!(
            "{} bytes written to {}",
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
