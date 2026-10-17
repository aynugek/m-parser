use std::io::{BufWriter, Write};

use parser::{Statement, SummaryHeader, TransactionHeader};

use crate::ExportError;

/// Number of leading empty columns used to indent the detailed transaction table
const FIRST_DETAILED_STATEMENT_COL: usize = 3;

// Exports a [`Statement`] as CSV.
pub fn export(stmt: Statement) -> Result<Vec<u8>, ExportError> {
    let mut w = BufWriter::new(vec![]);

    writeln!(w, "MPESA STATEMENT")?;
    writeln!(w)?;

    // Wrote customer and statement details
    writeln!(w, "Customer Name:,{}", stmt.meta.customer_name())?;
    writeln!(w, "Mobile Number:,{}", stmt.meta.customer_mobile_number())?;
    writeln!(w, "Email Address:,{}", stmt.meta.customer_email_address())?;
    writeln!(w, "Statement Period:,{}", stmt.meta.statement_period())?;
    writeln!(w, "Request Date:,{}", stmt.meta.statement_request_date())?;

    writeln!(w)?;
    writeln!(w)?;

    // Write summary table
    write_summary_table_header(&mut w)?;
    for summary in &stmt.summary {
        writeln!(
            w,
            "{},\"{}\",\"{}\"",
            summary.transaction_type(),
            summary.paid_in(),
            summary.paid_out()
        )?;
    }

    if !stmt.transactions.is_empty() {
        writeln!(w)?;
        writeln!(w)?;

        writeln!(
            w,
            "{},DETAILED STATEMENT",
            ",".repeat(FIRST_DETAILED_STATEMENT_COL)
        )?;
        write_detailed_statement_table_header(&mut w)?;
        for transaction in &stmt.transactions {
            writeln!(
                w,
                "{}, {},{},\"{}\",{},\"{}\",\"{}\",\"{}\"",
                ",".repeat(FIRST_DETAILED_STATEMENT_COL),
                transaction.receipt_no(),
                transaction.completion_time(),
                transaction.details(),
                transaction.transaction_status(),
                transaction.paid_in(),
                transaction.withdrawn(),
                transaction.balance(),
            )?
        }
    }

    Ok(w.into_inner().map_err(|e| e.into_error())?)
}

/// Writes the summary table's header row
fn write_summary_table_header(w: &mut impl Write) -> std::io::Result<()> {
    writeln!(
        w,
        "{}, {}, {}",
        SummaryHeader::TransactionType,
        SummaryHeader::PaidIn,
        SummaryHeader::PaidOut
    )
}

/// Writes the detailed transaction table's header row, indented by
/// [`FIRST_DETAILED_STATEMENT_COL`] columns.
fn write_detailed_statement_table_header(w: &mut impl Write) -> std::io::Result<()> {
    writeln!(
        w,
        "{},{},{},{},{},{},{},{} ",
        ",".repeat(FIRST_DETAILED_STATEMENT_COL),
        TransactionHeader::ReceiptNo,
        TransactionHeader::CompletionTime,
        TransactionHeader::Details,
        TransactionHeader::TransactionStatus,
        TransactionHeader::PaidIn,
        TransactionHeader::Withdrawn,
        TransactionHeader::Balance
    )
}
