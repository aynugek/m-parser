use export::FileFormat;

#[test]
fn statement_can_be_exported_to_every_format_repeatedly() {
    let bytes = std::fs::read("../parser/statements/statement_1_123456.pdf")
        .expect("sample statement should be readable");
    let stmt =
        parser::Statement::new(&bytes, Some("123456")).expect("sample statement should parse");

    for format in FileFormat::ALL {
        let output = export::run(&stmt, format)
            .unwrap_or_else(|e| panic!("{} export failed: {e}", format.label()));
        assert!(!output.is_empty(), "{} produced no bytes", format.label());
    }

    assert!(
        !stmt.transactions.is_empty(),
        "exporting must not consume the statement"
    );
}
