# m-parser

Parses M-Pesa statement PDFs into structured data and exports them to CSV, Markdown, XLSX, html or JSON.

## Usage

```sh
m-parser [-f <format>] [-o <output>] [--] <statement> [<password>]
```

| Flag | Description | Default |
|---|---|---|
| `-f`, `--format` | Output format: `csv`, `md`/`markdown`, `xlsx`, `json`, `html` | `csv` |
| `-o`, `--output` | Output file path | `<statement>.<format extension>` |
| `statement` | Path to the M-Pesa statement PDF | required |
| `password` | Statement password, if it's encrypted | optional |

Text extraction and table detection are done via [pdf-inspector](https://github.com/firecrawl/pdf-inspector).

## Library

The `m_parser` crate exposes `Statement::new(path, password)`, which parses a
statement PDF into a `Statement`:

- `meta`: customer name, mobile number, email, statement period, request date
- `summary`: totals paid in/out per transaction type
- `transactions`: every individual transaction (receipt no., completion time, details, status, paid in, withdrawn, balance)

## Project layout

- `src/main.rs` / `src/args.rs` — CLI entry point and argument parsing
- `src/export/` — file-format exporters (`csv`, `markdown`, `xlsx`, `json`, `html`)
- `src/lib.rs` — `Statement::new`, which extracts text/tables from the PDF and delegates to `meta`/`summary`/`transaction`
- `src/meta.rs` — customer/statement metadata parsing
- `src/summary.rs` — summary table parsing (`TransactionType`, `Summary`)
- `src/transaction.rs` — detailed transaction table parsing (`Transaction`, `TransactionStatus`)
- `src/money.rs` — `Money` type: parsing, display formatting, Excel serialization
- `src/errors.rs` — `MParserError`

Sample statement for testing: `statements/statement_1_123456.pdf` (password `123456`).

