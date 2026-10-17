# m-parser

Parses M-Pesa statement PDFs into structured data and exports them to CSV, Markdown, XLSX, HTML or JSON.

## Usage

```sh
cargo run -p cli --release -- [-f <format>] [-o <output>] <statement> [<password>]
```

Or install/build the `mparser-cli` binary:

```sh
cargo install --path cli
mparser-cli [-f <format>] [-o <output>] <statement> [<password>]
```

| Flag | Description | Default |
|---|---|---|
| `-f`, `--format` | Output format: `csv`, `md`/`markdown`, `xlsx`, `json`, `html` | `csv` |
| `-o`, `--output` | Output file path, or a directory (the file is named after the statement) | stdout |
| `statement` | Path to the M-Pesa statement PDF | required |
| `password` | Statement password, if it's encrypted | optional |

Notes:

- Without `-o`, output is written to stdout.
- `xlsx` is binary and **requires** `-o`.

Examples:

```sh
mparser-cli statement.pdf 123456 > statement.csv
mparser-cli -f json -o ~/statements statement.pdf 123456
mparser-cli -f xlsx -o statement.xlsx statement.pdf 123456
```

## Library

The `parser` crate exposes `Statement::new(bytes, password)`, which parses the
bytes of a statement PDF into a `Statement`:

- `meta`: customer name, mobile number, email, statement period, request date
- `summary`: totals paid in/out per transaction type
- `transactions`: every individual transaction (receipt no., completion time, details, status, paid in, withdrawn, balance)

The `export` crate provides `export::run(statement, &FileFormat)`, which
returns the exported file as bytes.

## Project layout

Cargo workspace with the following crates:

```text
m-parser/
├── cli/                    # `mparser-cli` binary
│   └── src/
│       ├── main.rs         # entry point
│       ├── args.rs         # argument parsing
│       └── errors.rs       # CliError
|
├── export/                 # file-format exporters
│   └── src/
│       ├── lib.rs          # FileFormat and run
│       ├── csv.rs
│       ├── markdown.rs
│       ├── xlsx.rs
│       ├── json.rs
│       ├── html.rs
│       └── errors.rs       # ExportError
|
└── parser/                 # PDF statement parsing
    ├── statements/         # sample PDFs for testing
    └── src/
        ├── lib.rs
        ├── statement.rs    # Statement::new: extracts text/tables
        ├── meta.rs         # customer/statement metadata
        ├── summary.rs      # summary table (TransactionType, Summary)
        ├── transaction.rs  # transaction table (Transaction, TransactionStatus)
        ├── money.rs        # Money: tx amount parsing
        └── errors.rs       # MParserError
```

Sample statement for testing: `parser/statements/statement_1_123456.pdf` (password `123456`).

## License

MIT, see [LICENSE.md](LICENSE.md).
