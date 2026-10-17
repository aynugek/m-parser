use std::path::PathBuf;

use export::FileFormat;

#[derive(Debug, argh::FromArgs)]
#[argh(
    description = "{command_name} is a tool to convert mpesa statements to csv/xlsx/md.\n\n",
    example = "\
    {command_name} --format csv <statement.pdf> <password>\n\
    {command_name} --format xlsx <statement.pdf> <password>\n\
    {command_name} --format md <statement.pdf> <password>"
)]
pub struct Args {
    /// destination file format
    #[argh(option, short = 'f', default = "FileFormat::Csv")]
    pub format: FileFormat,

    /// output filename
    #[argh(option, short = 'o', arg_name = "output")]
    pub output_path: Option<PathBuf>,

    /// mpesa statement pdf
    #[argh(positional, arg_name = "statement")]
    pub statement_path: PathBuf,

    /// mpesa statement password
    #[argh(positional)]
    pub password: Option<String>,
}

impl Args {
    pub fn new() -> Self {
        argh::from_env()
    }
}
