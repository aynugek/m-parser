//! The statement window: opening a PDF, showing its data, and exporting it.

use std::{
    cell::Cell,
    path::{Path, PathBuf},
    rc::Rc,
};

use export::FileFormat;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, Focusable as _, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Window, WindowAppearance, actions,
    assets::IconName,
    base::ElementExt as _,
    component::{
        ActiveTheme as _, Disableable as _, Icon, Sizable as _, Theme, ThemeMode, WindowExt,
        alert::Alert,
        button::{Button, ButtonVariants as _},
        dialog::DialogFooter,
        h_flex,
        input::{Input, InputEvent, InputState},
        menu::{DropdownMenu as _, PopupMenuItem},
        notification::Notification,
        scroll::ScrollableElement as _,
        spinner::Spinner,
        tab::{Tab, TabBar},
        table::{
            DataTable, Table, TableBody, TableCell, TableEvent, TableHead, TableHeader, TableRow,
            TableState,
        },
        v_flex,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};
use parser::Statement;

use crate::{
    errors::{GuiError, PASSWORD_INCORRECT_MESSAGE, PASSWORD_REQUIRED_MESSAGE},
    loaded::{LoadedStatement, file_name_for},
    transactions_table::{COLUMN_COUNT, TransactionsTable},
};

actions!(
    statement_app,
    [OpenStatement, FocusSearch, ShowTransactions, ShowSummary]
);

/// The two views of an open statement.
#[derive(Clone, Copy, PartialEq, Eq)]
enum StatementTab {
    Summary,
    Transactions,
}

impl StatementTab {
    fn index(self) -> usize {
        match self {
            Self::Summary => 0,
            Self::Transactions => 1,
        }
    }

    fn from_index(index: usize) -> Self {
        if index == 1 {
            Self::Transactions
        } else {
            Self::Summary
        }
    }
}

/// An open statement and the state that displays it.
struct LoadedView {
    loaded: LoadedStatement,
    table: Entity<TableState<TransactionsTable>>,
    _filter_subscriptions: Vec<Subscription>,
}

/// What the window is currently showing.
enum DocState {
    Empty,
    Loading {
        path: PathBuf,
    },
    Failed {
        path: PathBuf,
        password: Option<String>,
        error: GuiError,
    },
    Loaded(LoadedView),
}

/// The stages of the export workflow, which also disable the export command.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExportState {
    Idle,
    ChoosingPath,
    Writing(FileFormat),
}

impl ExportState {
    fn is_busy(self) -> bool {
        !matches!(self, Self::Idle)
    }
}

/// The application's single window.
pub struct StatementApp {
    focus_handle: gpui_kit::FocusHandle,
    doc: DocState,
    selected_tab: StatementTab,
    export_state: ExportState,
    /// Rejects the result of a parse that a newer request superseded.
    parse_generation: u64,
    _appearance_subscription: Subscription,
}

impl StatementApp {
    /// Builds the window and starts with no statement open.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);
        cx.defer_in(window, |this, window, cx| {
            window.focus(&this.focus_handle, cx);
        });

        cx.defer_in(window, |this, window, cx| this.toggle_theme(window, cx));

        let appearance_subscription = cx.observe_window_appearance(window, |_this, window, cx| {
            Theme::change(window.appearance(), Some(window), cx);
        });

        Self {
            focus_handle,
            doc: DocState::Empty,
            selected_tab: StatementTab::Transactions,
            export_state: ExportState::Idle,
            parse_generation: 0,
            _appearance_subscription: appearance_subscription,
        }
    }

    fn loaded(&self) -> Option<&LoadedView> {
        match &self.doc {
            DocState::Loaded(view) => Some(view),
            _ => None,
        }
    }

    /// Shows the file picker, then the password prompt for the chosen PDF.
    fn pick_statement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .set_title("Open statement")
                .add_filter("PDF", &["pdf"])
                .pick_file()
                .await;

            let Some(file) = file else {
                return;
            };
            let path = file.path().to_path_buf();
            this.update_in(cx, |this, window, cx| {
                this.show_password_dialog(path, None, window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Opens the prompt shown before every parse, optionally with an error.
    fn show_password_dialog(
        &mut self,
        path: PathBuf,
        error: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Statement password")
        });

        let entity = cx.entity();
        let submit = {
            let entity = entity.clone();
            let input = input.clone();
            let path = path.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| {
                let value = input.read(cx).value().to_string();
                let password = (!value.is_empty()).then_some(value);
                entity.update(cx, |this, cx| {
                    this.begin_parse(path.clone(), password.clone(), window, cx);
                });
            })
        };

        let focused = Cell::new(false);
        window.open_dialog(cx, move |dialog, window, cx| {
            if !focused.replace(true) {
                let handle = input.read(cx).focus_handle(cx);
                window.focus(&handle, cx);
            }

            let mut content = v_flex()
                .gap_3()
                .child(div().text_sm().text_color(cx.theme().muted_foreground))
                .child(Input::new(&input).w_full());

            if let Some(error) = error {
                content = content.child(Alert::error("password-error", error));
            }

            dialog
                .title("Password")
                .child(content)
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("password-cancel")
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child({
                            let submit = submit.clone();
                            Button::new("password-open")
                                .primary()
                                .label("Open")
                                .on_click(move |_, window, cx| {
                                    submit(window, cx);
                                    window.close_dialog(cx);
                                })
                        }),
                )
                .on_ok({
                    let submit = submit.clone();
                    move |_, window, cx| {
                        submit(window, cx);
                        true
                    }
                })
        });
    }

    /// Parses `path` in the background, keeping the UI responsive.
    fn begin_parse(
        &mut self,
        path: PathBuf,
        password: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.parse_generation = self.parse_generation.wrapping_add(1);
        let generation = self.parse_generation;
        self.doc = DocState::Loading { path: path.clone() };
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let read_path = path.clone();
            let parse_password = password.clone();
            let result = cx
                .background_spawn(async move {
                    let bytes = std::fs::read(&read_path).map_err(|source| GuiError::Read {
                        path: read_path.clone(),
                        source,
                    })?;
                    Statement::new(&bytes, parse_password.as_deref()).map_err(GuiError::from)
                })
                .await;

            this.update_in(cx, |this, window, cx| {
                if this.parse_generation == generation {
                    this.finish_parse(result, path, password, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn finish_parse(
        &mut self,
        result: Result<Statement, GuiError>,
        path: PathBuf,
        password: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(statement) => {
                self.show_statement(LoadedStatement::new(statement, path), window, cx);
            }
            Err(error) if error.is_password_error() => {
                let message = if password.is_some() {
                    PASSWORD_INCORRECT_MESSAGE
                } else {
                    PASSWORD_REQUIRED_MESSAGE
                };
                self.doc = DocState::Empty;
                cx.notify();
                self.show_password_dialog(path, Some(message), window, cx);
            }
            Err(error) => {
                self.doc = DocState::Failed {
                    path,
                    password,
                    error,
                };
                cx.notify();
            }
        }
    }

    fn show_statement(
        &mut self,
        loaded: LoadedStatement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let font_size = cx.theme().font_size;
        let statement = loaded.shared();
        let filter_inputs = TransactionsTable::new_filter_inputs(window, cx);
        let table = cx.new(|cx| {
            TableState::new(
                TransactionsTable::new(statement, font_size, filter_inputs.clone()),
                window,
                cx,
            )
            .sortable(true)
            .col_movable(false)
        });

        let mut filter_subscriptions = Vec::with_capacity(COLUMN_COUNT + 1);
        for (col_ix, filter) in filter_inputs.iter().enumerate() {
            let Some(filter) = filter else {
                continue;
            };
            let table = table.clone();
            filter_subscriptions.push(cx.subscribe_in(
                filter,
                window,
                move |_, state, event, _, cx| {
                    if !matches!(event, InputEvent::Change) {
                        return;
                    }
                    let query = state.read(cx).value().to_string();
                    table.update(cx, |table, cx| {
                        table.delegate_mut().set_column_filter(col_ix, &query);
                        table.refresh(cx);
                        cx.notify();
                    });
                    cx.notify();
                },
            ));
        }
        filter_subscriptions.push(cx.subscribe_in(
            &table,
            window,
            move |_, table, event, _, cx| {
                let TableEvent::ColumnWidthsChanged(widths) = event else {
                    return;
                };
                table.update(cx, |table, _| {
                    table.delegate_mut().sync_widths(widths);
                });
            },
        ));

        let file_name = loaded.file_name();
        window.set_window_title(&format!("{file_name} — M-Pesa Statements"));
        self.selected_tab = StatementTab::Transactions;
        self.doc = DocState::Loaded(LoadedView {
            loaded,
            table,
            _filter_subscriptions: filter_subscriptions,
        });
        cx.notify();
    }

    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(filter) = ({
            let Some(view) = self.loaded() else {
                return;
            };
            view.table.read(cx).delegate().filter_input(0).cloned()
        }) else {
            return;
        };
        if self.selected_tab != StatementTab::Transactions {
            self.selected_tab = StatementTab::Transactions;
            cx.notify();
        }

        let handle = filter.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    fn select_tab(&mut self, tab: StatementTab, cx: &mut Context<Self>) {
        if self.loaded().is_none() || self.selected_tab == tab {
            return;
        }
        self.selected_tab = tab;
        cx.notify();
    }

    fn retry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (path, password) = match &self.doc {
            DocState::Failed { path, password, .. } => (path.clone(), password.clone()),
            _ => return,
        };
        self.begin_parse(path, password, window, cx);
    }

    /// Switches the application between the light and dark themes.
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if cx.theme().is_dark() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        Theme::change(mode, Some(window), cx);
        let appearance = if mode.is_dark() {
            WindowAppearance::Dark
        } else {
            WindowAppearance::Light
        };
        cx.set_window_appearance(Some(appearance));
    }

    /// Asks for a destination, then exports and writes the open statement.
    fn begin_export(&mut self, format: FileFormat, window: &mut Window, cx: &mut Context<Self>) {
        if self.export_state.is_busy() {
            return;
        }
        let view = match self.loaded() {
            Some(view) => view,
            None => return,
        };

        let (statement, file_name) = (view.loaded.shared(), view.loaded.file_name());

        self.export_state = ExportState::ChoosingPath;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .set_title(format!("Export as {}", format.label()))
                .set_file_name(format!("{}.{}", file_name, format.extension()))
                .add_filter(format.label(), &[format.extension()])
                .save_file()
                .await;

            let Some(file) = file else {
                this.update_in(cx, |this, _, cx| {
                    this.export_state = ExportState::Idle;
                    cx.notify();
                })
                .ok();
                return;
            };

            let path = file.path().to_path_buf();
            this.update_in(cx, |this, _, cx| {
                this.export_state = ExportState::Writing(format);
                cx.notify();
            })
            .ok();

            let write_path = path.clone();
            let result = cx
                .background_spawn(async move {
                    let bytes = export::run(&statement, format)?;
                    std::fs::write(&write_path, bytes).map_err(|source| GuiError::Write {
                        path: write_path.clone(),
                        source,
                    })?;
                    Ok::<PathBuf, GuiError>(write_path)
                })
                .await;

            this.update_in(cx, |this, window, cx| {
                this.finish_export(result, window, cx)
            })
            .ok();
        })
        .detach();
    }

    fn finish_export(
        &mut self,
        result: Result<PathBuf, GuiError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.export_state = ExportState::Idle;
        match result {
            Ok(path) => window.push_notification(
                Notification::success(format!("Saved to {}", path.display())),
                cx,
            ),
            Err(error) => window.push_notification(
                Notification::error(error.to_string()).title("Couldn't export"),
                cx,
            ),
        }
        cx.notify();
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let document_name = match &self.doc {
            DocState::Empty => None,
            DocState::Loading { path, .. } | DocState::Failed { path, .. } => {
                Some(file_name_for(path))
            }
            DocState::Loaded(view) => Some(view.loaded.file_name()),
        };
        let has_statement = self.loaded().is_some();
        let export_busy = self.export_state.is_busy();
        let export_writing = matches!(self.export_state, ExportState::Writing(_));
        let export_label: SharedString = match self.export_state {
            ExportState::Writing(format) => format!("Exporting {}…", format.label()).into(),
            _ => "Export…".into(),
        };
        let entity = cx.entity();
        let is_dark = cx.theme().is_dark();
        let (theme_icon, theme_tooltip) = if is_dark {
            (IconName::Sun, "Switch to light mode")
        } else {
            (IconName::Moon, "Switch to dark mode")
        };

        h_flex()
            .h_10()
            .px_3()
            .gap_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(IconName::FileText)
                            .size_5()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("M-Pesa Statements"),
                    )
                    .when_some(document_name, |this, name| {
                        this.child(
                            div()
                                .min_w_0()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .text_ellipsis()
                                .child(name),
                        )
                    }),
            )
            .child(
                Button::new("toggle-theme")
                    .ghost()
                    .icon(theme_icon)
                    .tooltip(theme_tooltip)
                    .accessibility_label(theme_tooltip)
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_theme(window, cx))),
            )
            .child(
                Button::new("open-statement")
                    .outline()
                    .icon(IconName::FolderOpen)
                    .label("Open…")
                    .on_click(cx.listener(|this, _, window, cx| this.pick_statement(window, cx))),
            )
            .child(
                Button::new("export-statement")
                    .outline()
                    .icon(IconName::Download)
                    .label(export_label)
                    .loading(export_writing)
                    .disabled(!has_statement || export_busy)
                    .dropdown_menu(move |menu, _, _| {
                        FileFormat::ALL.into_iter().fold(menu, |menu, format| {
                            let entity = entity.clone();
                            menu.item(PopupMenuItem::new(format.label()).on_click(
                                move |_, window, cx| {
                                    entity.update(cx, |this, cx| {
                                        this.begin_export(format, window, cx)
                                    });
                                },
                            ))
                        })
                    }),
            )
            .into_any_element()
    }

    fn render_document(&self, cx: &mut Context<Self>) -> AnyElement {
        match &self.doc {
            DocState::Empty => self.render_empty(cx),
            DocState::Loading { path, .. } => self.render_loading(path),
            DocState::Failed { path, error, .. } => self.render_failed(path, error, cx),
            DocState::Loaded(view) => self.render_loaded(view, cx),
        }
    }

    fn render_loaded(&self, view: &LoadedView, cx: &mut Context<Self>) -> AnyElement {
        let statement = view.loaded.statement();
        let tab_bar = TabBar::new("statement-tabs")
            .underline()
            .selected_index(self.selected_tab.index())
            .child(Tab::new().label("Summary"))
            .child(Tab::new().label("Transactions"))
            .on_click(cx.listener(|this, index: &usize, _, cx| {
                this.select_tab(StatementTab::from_index(*index), cx);
            }));

        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_meta(statement, cx))
            .child(div().px_4().child(tab_bar))
            .child(match self.selected_tab {
                StatementTab::Transactions => self.render_transactions(view, cx),
                StatementTab::Summary => self.render_summary(statement, cx),
            })
            .into_any_element()
    }

    fn render_meta(&self, statement: &Statement, cx: &mut Context<Self>) -> AnyElement {
        let meta = &statement.meta;

        h_flex()
            .flex_wrap()
            .gap_4()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(meta_item("Customer", meta.customer_name(), cx))
            .child(meta_item("Mobile", meta.customer_mobile_number(), cx))
            .child(meta_item("Email", meta.customer_email_address(), cx))
            .child(meta_item(
                "Statement period",
                meta.statement_period().to_string(),
                cx,
            ))
            .child(meta_item(
                "Request date",
                meta.statement_request_date().to_string(),
                cx,
            ))
            .into_any_element()
    }

    fn render_transactions(&self, view: &LoadedView, cx: &mut Context<Self>) -> AnyElement {
        let delegate = view.table.read(cx).delegate();
        let total = view.loaded.statement().transactions.len();
        let visible = delegate.visible_count();
        let (paid_in_total, withdrawn_total) = delegate.visible_amount_totals();
        let count = if visible == total {
            format!("{total} transactions")
        } else {
            format!("{visible} of {total} transactions")
        };
        let header_row_height = cx.theme().font_size * 3.75;

        v_flex()
            .flex_1()
            .min_h_0()
            .gap_3()
            .p_3()
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(count)
                    .child(
                        h_flex()
                            .gap_4()
                            .child(amount_total("Paid in", &paid_in_total, cx.theme().success))
                            .child(amount_total(
                                "Withdrawn",
                                &withdrawn_total,
                                cx.theme().danger,
                            )),
                    ),
            )
            .child({
                let table = view.table.clone();
                div()
                    .id("transactions-table")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .on_prepaint(move |bounds, window, cx| {
                        let width = bounds.size.width;
                        if width <= px(0.) || !table.read(cx).delegate().needs_details_fit(width) {
                            return;
                        }
                        let table = table.clone();
                        window.on_next_frame(move |_, cx| {
                            table.update(cx, |table, cx| {
                                if table.delegate_mut().fit_details_to(width) {
                                    table.refresh(cx);
                                    cx.notify();
                                }
                            });
                        });
                    })
                    .child(DataTable::new(&view.table).header_row_height(header_row_height))
            })
            .into_any_element()
    }

    fn render_summary(&self, statement: &Statement, cx: &mut Context<Self>) -> AnyElement {
        let content = if statement.summary.is_empty() {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("This statement has no summary rows.")
                .into_any_element()
        } else {
            Table::new()
                .small()
                .child(
                    TableHeader::new().child(
                        TableRow::new()
                            .child(TableHead::new().child("Transaction type"))
                            .child(TableHead::new().text_right().child("Paid in"))
                            .child(TableHead::new().text_right().child("Paid out")),
                    ),
                )
                .child(
                    TableBody::new().children(statement.summary.iter().map(|summary| {
                        TableRow::new()
                            .child(TableCell::new().child(summary.transaction_type().to_string()))
                            .child(
                                TableCell::new()
                                    .text_right()
                                    .text_color(cx.theme().success)
                                    .child(summary.paid_in().to_string()),
                            )
                            .child(
                                TableCell::new()
                                    .text_right()
                                    .text_color(cx.theme().danger)
                                    .child(summary.paid_out().to_string()),
                            )
                    })),
                )
                .into_any_element()
        };

        div()
            .flex_1()
            .min_h_0()
            .p_4()
            .overflow_y_scrollbar()
            .child(content)
            .into_any_element()
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_3()
            .p_8()
            .child(
                Icon::new(IconName::FileText)
                    .size_8()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("No statement open"),
            )
            .child(
                div()
                    .max_w_96()
                    .text_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Open an M-Pesa PDF statement to review its summary and transactions."),
            )
            .child(
                Button::new("empty-open")
                    .primary()
                    .icon(IconName::FolderOpen)
                    .label("Open statement…")
                    .on_click(cx.listener(|this, _, window, cx| this.pick_statement(window, cx))),
            )
            .into_any_element()
    }

    fn render_loading(&self, path: &Path) -> AnyElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_3()
            .p_8()
            .child(Spinner::new().large())
            .child(
                div()
                    .text_sm()
                    .child(format!("Reading {}…", file_name_for(path))),
            )
            .into_any_element()
    }

    fn render_failed(&self, path: &Path, error: &GuiError, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .p_8()
            .child(
                v_flex()
                    .w_full()
                    .max_w_128()
                    .gap_4()
                    .child(
                        Alert::error("open-error", error.to_string())
                            .title("Couldn't open statement"),
                    )
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(path.display().to_string()),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("retry-open")
                                    .primary()
                                    .label("Try again")
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.retry(window, cx)),
                                    ),
                            )
                            .child(
                                Button::new("choose-another")
                                    .outline()
                                    .label("Choose another file…")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.pick_statement(window, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let (left, right) = match &self.doc {
            DocState::Empty => (String::new(), String::new()),
            DocState::Loading { path, .. } => {
                (path.display().to_string(), "Reading statement…".to_owned())
            }
            DocState::Failed { path, .. } => {
                (path.display().to_string(), "Couldn't open".to_owned())
            }
            DocState::Loaded(view) => (
                view.loaded.path().display().to_string(),
                format!(
                    "{} transactions",
                    view.loaded.statement().transactions.len()
                ),
            ),
        };

        h_flex()
            .h_8()
            .px_3()
            .gap_4()
            .items_center()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(left),
            )
            .when(!right.is_empty(), |this| {
                this.child(div().whitespace_nowrap().child(right))
            })
            .into_any_element()
    }
}

impl Render for StatementApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .key_context("StatementApp")
            .on_action(
                cx.listener(|this, _: &OpenStatement, window, cx| this.pick_statement(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &FocusSearch, window, cx| this.focus_search(window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowTransactions, _, cx| {
                this.select_tab(StatementTab::Transactions, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowSummary, _, cx| {
                this.select_tab(StatementTab::Summary, cx)
            }))
            .child(self.render_toolbar(cx))
            .child(self.render_document(cx))
            .child(self.render_status_bar(cx))
    }
}

/// A labeled Paid In / Withdrawn total beside the transaction count.
fn amount_total(label: &'static str, value: &str, value_color: Hsla) -> AnyElement {
    h_flex()
        .gap_1()
        .child(div().child(format!("{label}:")))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(value_color)
                .child(value.to_string()),
        )
        .into_any_element()
}

/// One label/value pair in the statement's metadata strip.
fn meta_item(label: &'static str, value: impl Into<SharedString>, cx: &App) -> AnyElement {
    v_flex()
        .flex_1()
        .min_w_32()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            div()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_sm()
                .child(value.into()),
        )
        .into_any_element()
}
