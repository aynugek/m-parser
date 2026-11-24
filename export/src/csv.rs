use std::io::Write;

use parser::{Statement, SummaryHeader, TransactionHeader};

use crate::ExportError;

/// Three leading empty columns indent the detailed transaction table.
const TRANSACTIONS_INDENT: &str = ",,,";

// Exports a [`Statement`] as CSV.
pub fn export(stmt: Statement) -> Result<Vec<u8>, ExportError> {
    let mut w: Vec<u8> = Vec::with_capacity(1024 + stmt.transactions.len() * 128);

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

        writeln!(w, "{TRANSACTIONS_INDENT},DETAILED STATEMENT")?;
        write_detailed_statement_table_header(&mut w)?;
        for transaction in &stmt.transactions {
            writeln!(
                w,
                "{TRANSACTIONS_INDENT}, {},{},\"{}\",{},\"{}\",\"{}\",\"{}\"",
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

    Ok(w)
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
/// [`TRANSACTIONS_INDENT`].
fn write_detailed_statement_table_header(w: &mut impl Write) -> std::io::Result<()> {
    writeln!(
        w,
        "{TRANSACTIONS_INDENT},{},{},{},{},{},{},{} ",
        TransactionHeader::ReceiptNo,
        TransactionHeader::CompletionTime,
        TransactionHeader::Details,
        TransactionHeader::TransactionStatus,
        TransactionHeader::PaidIn,
        TransactionHeader::Withdrawn,
        TransactionHeader::Balance
    )
}
