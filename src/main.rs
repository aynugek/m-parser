mod args;
mod export;

use args::Args;
use m_parser::MParserError;

fn main() -> Result<(), MParserError> {
    // Parse command line arguments
    let args: Args = Args::new();

    // select file location if output file is not given
    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| args.statement_path.with_extension(args.format.extension()));

    // export mpesa statement into specified format
    export::run(args.statement_path, args.password, output_path, args.format)?;

    Ok(())
}
