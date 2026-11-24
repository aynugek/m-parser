// SPDX-License-Identifier: GPL-3.0-or-later

use std::{cell::RefCell, fs::File, io, rc::Rc};

use parser::Statement;
use slint::{ComponentHandle, SharedString};
use slint_kit::{
    FileFilter, FileHandle, FileLocation, FilePickerBinding, FilePickerPorts, FileRequest,
    FileSelectionMode, event,
};

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = MainWindow::new()?;
    slint_kit::install(&app);

    let selected_file = Rc::new(RefCell::new(None::<FileHandle>));
    let pdf_filter = FileFilter::new("M-Pesa statement".into(), vec!["pdf".into()])
        .map_err(|error| io::Error::other(format!("invalid built-in PDF filter: {error:?}")))?;
    let picker = FilePickerBinding::new(
        &app,
        FilePickerPorts {
            entries: MainWindow::set_entries,
            state: MainWindow::set_request_state,
            error: MainWindow::set_file_error,
        },
    );
    picker.set_mode(FileSelectionMode::Single);
    picker.set_request(FileRequest {
        selection: FileSelectionMode::Single,
        filters: vec![pdf_filter],
        title: Some("Choose a M-Pesa statement".into()),
        ..Default::default()
    });

    let selected_file_for_picker = Rc::clone(&selected_file);
    picker.on_selection(event(&app, move |app, files: Vec<FileHandle>| {
        let file = files.into_iter().next();
        app.set_has_selection(file.is_some());
        *selected_file_for_picker.borrow_mut() = file;
    }));

    app.on_browse(picker.browse_handler());
    app.on_remove(picker.remove_handler());

    let selected_file_for_parse = Rc::clone(&selected_file);
    app.on_parse_requested(event(&app, move |app, password: SharedString| {
        let Some(file) = selected_file_for_parse.borrow().as_ref().cloned() else {
            return;
        };
        let path = match file.location {
            FileLocation::Path(path) => path,
            FileLocation::Uri(_) => {
                app.set_status("This file provider does not expose a local path.".into());
                return;
            }
        };

        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) => {
                app.set_status(format!("Could not open statement: {error}").into());
                return;
            }
        };
        let password = (!password.is_empty()).then_some(password.as_str());
        match Statement::new(file, password) {
            Ok(_statement) => {
                todo!("display statement");
            }
            Err(error) => app.set_status(format!("Could not parse statement: {error}").into()),
        }
    }));

    app.run()?;
    Ok(())
}
