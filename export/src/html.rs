use std::io::{BufWriter, Write};

use parser::{Statement, SummaryHeader, TransactionHeader};

use crate::ExportError;

/// Exports a [`Statement`] as an HTML document
pub fn export(stmt: Statement) -> Result<Vec<u8>, ExportError> {
    let mut w = BufWriter::new(vec![]);

    writeln!(w, "<!DOCTYPE html>")?;
    writeln!(w, "<html lang=\"en\">")?;
    writeln!(w, "<head>")?;
    writeln!(w, "<meta charset=\"UTF-8\">")?;
    writeln!(
        w,
        "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">"
    )?;
    writeln!(w, "<title>MPESA STATEMENT</title>")?;
    writeln!(w, "<style>{}</style>", STYLE)?;
    writeln!(w, "</head>")?;
    writeln!(w, "<body>")?;

    writeln!(w, "<h1>MPESA STATEMENT</h1>")?;

    writeln!(w, "<p>")?;
    writeln!(
        w,
        "<strong>Customer Name:</strong> <em>{}</em><br>",
        escape(stmt.meta.customer_name())
    )?;
    writeln!(
        w,
        "<strong>Mobile Number:</strong> <em>{}</em><br>",
        escape(stmt.meta.customer_mobile_number())
    )?;
    writeln!(
        w,
        "<strong>Email Address:</strong> <em>{}</em><br>",
        escape(stmt.meta.customer_email_address())
    )?;
    writeln!(
        w,
        "<strong>Statement Period:</strong> <em>{}</em><br>",
        escape(&stmt.meta.statement_period().to_string())
    )?;
    writeln!(
        w,
        "<strong>Request Date:</strong> <em>{}</em>",
        escape(&stmt.meta.statement_request_date().to_string())
    )?;
    writeln!(w, "</p>")?;

    writeln!(w, "<hr>")?;

    write_summary_table(&mut w, &stmt)?;

    writeln!(w, "<hr>")?;

    write_detailed_statement_table(&mut w, &stmt)?;

    writeln!(w, "</body>")?;
    writeln!(w, "</html>")?;

    Ok(w.into_inner().map_err(|e| e.into_error())?)
}

/// Writes the summary table
fn write_summary_table(w: &mut impl Write, stmt: &Statement) -> std::io::Result<()> {
    writeln!(w, "<table class=\"summary-table\">")?;
    writeln!(w, "<thead>")?;
    writeln!(w, "<tr>")?;
    writeln!(
        w,
        "<th style=\"text-align: left;\">{}</th>",
        SummaryHeader::TransactionType
    )?;
    writeln!(
        w,
        "<th style=\"text-align: right;\">{}</th>",
        SummaryHeader::PaidIn
    )?;
    writeln!(
        w,
        "<th style=\"text-align: right;\">{}</th>",
        SummaryHeader::PaidOut
    )?;
    writeln!(w, "</tr>")?;
    writeln!(w, "</thead>")?;
    writeln!(w, "<tbody>")?;

    for summary in &stmt.summary {
        writeln!(w, "<tr>")?;
        writeln!(
            w,
            "<td style=\"text-align: left;\">{}</td>",
            escape(&summary.transaction_type().to_string())
        )?;
        writeln!(
            w,
            "<td style=\"text-align: right;\">{}</td>",
            escape(&summary.paid_in().to_string())
        )?;
        writeln!(
            w,
            "<td style=\"text-align: right;\">{}</td>",
            escape(&summary.paid_out().to_string())
        )?;
        writeln!(w, "</tr>")?;
    }

    writeln!(w, "</tbody>")?;
    writeln!(w, "</table>")
}

/// Writes the detailed transaction table
fn write_detailed_statement_table(w: &mut impl Write, stmt: &Statement) -> std::io::Result<()> {
    writeln!(w, "<table class=\"transactions-table\">")?;
    writeln!(w, "<thead>")?;
    writeln!(w, "<tr>")?;
    writeln!(
        w,
        "<th style=\"text-align: center;\">{}</th>",
        TransactionHeader::ReceiptNo
    )?;
    writeln!(
        w,
        "<th style=\"text-align: center;\">{}</th>",
        TransactionHeader::CompletionTime
    )?;
    writeln!(
        w,
        "<th style=\"text-align: left;\">{}</th>",
        TransactionHeader::Details
    )?;
    writeln!(
        w,
        "<th style=\"text-align: center;\">{}</th>",
        TransactionHeader::TransactionStatus
    )?;
    writeln!(
        w,
        "<th style=\"text-align: right;\">{}</th>",
        TransactionHeader::PaidIn
    )?;
    writeln!(
        w,
        "<th style=\"text-align: right;\">{}</th>",
        TransactionHeader::Withdrawn
    )?;
    writeln!(
        w,
        "<th style=\"text-align: right;\">{}</th>",
        TransactionHeader::Balance
    )?;
    writeln!(w, "</tr>")?;
    writeln!(w, "</thead>")?;
    writeln!(w, "<tbody>")?;

    for transaction in &stmt.transactions {
        writeln!(w, "<tr>")?;
        writeln!(
            w,
            "<td style=\"text-align: center;\">{}</td>",
            escape(transaction.receipt_no())
        )?;
        writeln!(
            w,
            "<td style=\"text-align: center;\">{}</td>",
            escape(&transaction.completion_time().to_string())
        )?;
        writeln!(
            w,
            "<td style=\"text-align: left;\">{}</td>",
            escape(transaction.details())
        )?;
        writeln!(
            w,
            "<td style=\"text-align: center;\">{}</td>",
            escape(&transaction.transaction_status().to_string())
        )?;
        writeln!(
            w,
            "<td style=\"text-align: right;\">{}</td>",
            escape(&transaction.paid_in().to_string())
        )?;
        writeln!(
            w,
            "<td style=\"text-align: right;\">{}</td>",
            escape(&transaction.withdrawn().to_string())
        )?;
        writeln!(
            w,
            "<td style=\"text-align: right;\">{}</td>",
            escape(&transaction.balance().to_string())
        )?;
        writeln!(w, "</tr>")?;
    }

    writeln!(w, "</tbody>")?;
    writeln!(w, "</table>")
}

/// Escapes HTML-sensitive characters
fn escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const STYLE: &str = r#"
body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Arial, sans-serif;
    max-width: 960px;
    margin: 2rem auto;
    padding: 0 1rem;
    color: #1a1a1a;
}
h1 {
    text-align: center;
}
table {
    width: 100%;
    border-collapse: collapse;
    margin: 1rem 0 2rem;
}
th, td {
    border: 1px solid #ccc;
    padding: 0.5rem 0.75rem;
}
th {
    background-color: #f2f2f2;
}
hr {
    border: none;
    border-top: 1px solid #ccc;
    margin: 1.5rem 0;
}
"#;
