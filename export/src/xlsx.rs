use parser::{MParserError, Statement, SummaryHeader, TransactionHeader};
use rust_xlsxwriter::{Format, Table, TableColumn, Workbook};

// Exports a [`Statement`] as an XLSX workbook
pub fn export(stmt: Statement) -> Result<Vec<u8>, MParserError> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    let mut row: u32 = 0;
    // Column at which the detailed transaction table starts, chosen to
    // leave enough room for the metadata/summary table's widest column.
    let table_start_col = 4;

    let date_time_format = Format::new().set_num_format("dd-mm-yyyy hh:mm");
    let date_format = Format::new().set_num_format("dd-mm-yyyy");

    worksheet.set_column_width(0, 30)?;
    worksheet.set_column_width(1, 20)?;

    worksheet.write_string(row, 0, "MPESA STATEMENT")?;
    row += 1;
    worksheet.write_string(row, 0, "Customer Name")?;
    worksheet.write_string(row, 1, stmt.meta.customer_name())?;
    row += 1;
    worksheet.write_string(row, 0, "Mobile Number")?;
    worksheet.write_string(row, 1, stmt.meta.customer_mobile_number())?;
    row += 1;
    worksheet.write_string(row, 0, "Email Address")?;
    worksheet.write_string(row, 1, stmt.meta.customer_email_address())?;
    row += 1;
    worksheet.write_string(row, 0, "Statement Period")?;
    worksheet.write_string(row, 1, stmt.meta.statement_period().to_string())?;
    row += 1;
    worksheet.write_string(row, 0, "Request Date")?;
    worksheet.write_with_format(
        row,
        1,
        stmt.meta.statement_request_date().to_string(),
        &date_format,
    )?;
    row += 2;

    let summary_header_row = row;
    worksheet.set_column_width(2, 12)?;
    for summary in &stmt.summary {
        row += 1;
        worksheet.write_string(row, 0, summary.transaction_type().to_string())?;
        worksheet.write(row, 1, summary.paid_in())?;
        worksheet.write(row, 2, summary.paid_out())?;
    }

    let summary_table = Table::new().set_columns(&[
        TableColumn::new().set_header(SummaryHeader::TransactionType.to_string()),
        TableColumn::new().set_header(SummaryHeader::PaidIn.to_string()),
        TableColumn::new().set_header(SummaryHeader::PaidOut.to_string()),
    ]);
    worksheet.add_table(summary_header_row, 0, row, 2, &summary_table)?;

    // DETAILED STATEMENT TABLE
    worksheet.set_column_width(table_start_col, 16)?; // Receipt No
    worksheet.set_column_width(table_start_col + 1, 20)?; // Completion Time
    worksheet.set_column_width(table_start_col + 2, 64)?; // Details
    worksheet.set_column_width(table_start_col + 3, 16)?; // Transaction Status
    worksheet.set_column_width(table_start_col + 3, 16)?; // Paid In
    worksheet.set_column_width(table_start_col + 5, 16)?; // Withdrawn
    worksheet.set_column_width(table_start_col + 6, 16)?; // Balance

    row += 2;
    worksheet.write_string(row, table_start_col + 1, "DETAILED STATEMENT")?;
    row += 1;

    let detail_header_row = row;
    for t in &stmt.transactions {
        row += 1;
        worksheet.write_string(row, table_start_col, t.receipt_no())?;
        worksheet.write_with_format(
            row,
            table_start_col + 1,
            t.completion_time().to_string(),
            &date_time_format,
        )?;
        worksheet.write_string(row, table_start_col + 2, t.details())?;
        worksheet.write_string(row, table_start_col + 3, t.transaction_status().to_string())?;
        worksheet.write(row, table_start_col + 4, t.paid_in())?;
        worksheet.write(row, table_start_col + 5, t.withdrawn())?;
        worksheet.write(row, table_start_col + 6, t.balance())?;
    }

    let detail_table = Table::new().set_columns(&[
        TableColumn::new().set_header(TransactionHeader::ReceiptNo.to_string()),
        TableColumn::new().set_header(TransactionHeader::CompletionTime.to_string()),
        TableColumn::new().set_header(TransactionHeader::Details.to_string()),
        TableColumn::new().set_header(TransactionHeader::TransactionStatus.to_string()),
        TableColumn::new().set_header(TransactionHeader::PaidIn.to_string()),
        TableColumn::new().set_header(TransactionHeader::Withdrawn.to_string()),
        TableColumn::new().set_header(TransactionHeader::Balance.to_string()),
    ]);
    worksheet.add_table(
        detail_header_row,
        table_start_col,
        row,
        table_start_col + 6,
        &detail_table,
    )?;

    Ok(workbook.save_to_buffer()?)
}
