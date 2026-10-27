//! The virtualized transaction table and its filtering and sorting.

use std::{cmp::Ordering, sync::Arc};

use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Pixels, SharedString, Styled as _, Window,
    component::{
        ActiveTheme as _, Sizable as _, h_flex,
        input::{Input, InputState},
        table::{Column, ColumnSort, TableDelegate, TableState},
        v_flex,
    },
    div,
    prelude::FluentBuilder as _,
    px,
};
use parser::{Money, Statement};

/// Number of columns in the detailed transaction table.
pub const COLUMN_COUNT: usize = 7;

/// Column keys, matching the parsed transaction fields.
const COLUMN_KEYS: [&str; COLUMN_COUNT] = [
    "receipt_no",
    "completion_time",
    "details",
    "status",
    "paid_in",
    "withdrawn",
    "balance",
];

/// Column headings, matching the source PDF's wording.
const COLUMN_NAMES: [&str; COLUMN_COUNT] = [
    "Receipt No.",
    "Completion Time",
    "Details",
    "Status",
    "Paid In",
    "Withdrawn",
    "Balance",
];

/// The first column holding a right-aligned amount.
const FIRST_AMOUNT_COLUMN: usize = 4;

/// Details absorbs leftover width when the window is wider than the columns.
const DETAILS_COLUMN: usize = 2;

/// Paid In and Withdrawn accept only decimal filter text.
const PAID_IN_COLUMN: usize = 4;
const WITHDRAWN_COLUMN: usize = 5;
const BALANCE_COLUMN: usize = 6;

/// Column widths as multiples of the theme's font size, in the same order as
/// [`COLUMN_NAMES`].
const COLUMN_WIDTHS: [f32; COLUMN_COUNT] = [6.5, 9.5, 22.0, 6.0, 6.0, 6.0, 6.0];

/// Matches DataTable's default trailing gutter (`w_3`) plus left/right borders.
const TABLE_CHROME_WIDTH: f32 = 14.;

/// Upper bound so Details can grow with a maximized window.
const DETAILS_MAX_WIDTH: f32 = 10_000.;

/// Balance has no filter; running balance is not a filterable field.
fn column_has_filter(col_ix: usize) -> bool {
    col_ix != BALANCE_COLUMN
}

/// True for columns whose filter input rejects non-decimal characters.
fn is_decimal_filter_column(col_ix: usize) -> bool {
    matches!(col_ix, PAID_IN_COLUMN | WITHDRAWN_COLUMN)
}

/// Formats a money total, using `0.00` when the amount is zero.
fn format_total(amount: &parser::Money) -> String {
    let text = amount.to_string();
    if text.is_empty() {
        "0.00".to_string()
    } else {
        text
    }
}

/// Allows empty text and intermediate decimal typing (`-`, `1.`, `1,234.5`).
fn is_decimal_filter_input(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }

    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_sign = false;

    for c in value.chars() {
        match c {
            '-' if !seen_digit && !seen_dot && !seen_sign => seen_sign = true,
            '0'..='9' => seen_digit = true,
            ',' if seen_digit && !seen_dot => {}
            '.' if !seen_dot => seen_dot = true,
            _ => return false,
        }
    }

    true
}

/// Drops thousands separators so typed decimals match displayed money.
fn normalize_amount_filter_text(text: &str) -> String {
    text.chars()
        .filter(|&c| c != ',')
        .collect::<String>()
        .to_lowercase()
}

/// The data source for the transactions [`DataTable`](gpui_kit::component::table::DataTable).
///
/// Keeps the filtered and sorted row order in `visible` so the table only ever
/// renders transactions that match every active column filter.
pub struct TransactionsTable {
    statement: Arc<Statement>,
    /// Lowercased text of each column per transaction, built once.
    searchable: Vec<[String; COLUMN_COUNT]>,
    /// Indices into `statement.transactions`, in display order.
    visible: Vec<usize>,
    /// Lowercased query per column; empty means that column is not filtered.
    filters: [String; COLUMN_COUNT],
    /// Search field per filterable column (`None` for Balance).
    filter_inputs: [Option<Entity<InputState>>; COLUMN_COUNT],
    /// The active sort column and direction, or `None` for statement order.
    sort: Option<(usize, ColumnSort)>,
    widths: [Pixels; COLUMN_COUNT],
    /// Floor for Details when the window shrinks; set from the initial width.
    details_min_width: Pixels,
    /// Last table width Details was fitted to; skips redundant layout passes.
    fitted_table_width: Option<Pixels>,
}

impl TransactionsTable {
    /// Builds one cleanable filter input per filterable column.
    ///
    /// Paid In and Withdrawn reject non-decimal keystrokes and paste. Balance
    /// has no filter.
    pub fn new_filter_inputs(
        window: &mut Window,
        cx: &mut App,
    ) -> [Option<Entity<InputState>>; COLUMN_COUNT] {
        std::array::from_fn(|col_ix| {
            if !column_has_filter(col_ix) {
                return None;
            }
            Some(cx.new(|cx| {
                let state = InputState::new(window, cx)
                    .placeholder("Filter")
                    .clean_on_escape();
                if is_decimal_filter_column(col_ix) {
                    state.validate(|value, _| is_decimal_filter_input(value))
                } else {
                    state
                }
            }))
        })
    }

    /// Builds the table for `statement`, sizing columns from `font_size` so the
    /// layout follows the window's text scale.
    pub fn new(
        statement: Arc<Statement>,
        font_size: Pixels,
        filter_inputs: [Option<Entity<InputState>>; COLUMN_COUNT],
    ) -> Self {
        let searchable = statement
            .transactions
            .iter()
            .map(|transaction| {
                [
                    transaction.receipt_no().to_lowercase(),
                    transaction.completion_time().to_string().to_lowercase(),
                    transaction.details().to_lowercase(),
                    transaction.transaction_status().to_string().to_lowercase(),
                    normalize_amount_filter_text(&transaction.paid_in().to_string()),
                    normalize_amount_filter_text(&transaction.withdrawn().to_string()),
                    String::new(),
                ]
            })
            .collect();

        let widths = COLUMN_WIDTHS.map(|factor| font_size * factor);
        let mut table = Self {
            statement,
            searchable,
            visible: Vec::new(),
            filters: std::array::from_fn(|_| String::new()),
            filter_inputs,
            sort: None,
            details_min_width: widths[DETAILS_COLUMN] * 0.6,
            fitted_table_width: None,
            widths,
        };
        table.rebuild_visible();
        table
    }

    /// The filter input for column `col_ix`, if that column is filterable.
    pub fn filter_input(&self, col_ix: usize) -> Option<&Entity<InputState>> {
        self.filter_inputs[col_ix].as_ref()
    }

    /// Applies a search query to a single column.
    pub fn set_column_filter(&mut self, col_ix: usize, query: &str) {
        if !column_has_filter(col_ix) {
            return;
        }
        self.filters[col_ix] = if is_decimal_filter_column(col_ix) {
            normalize_amount_filter_text(query.trim())
        } else {
            query.trim().to_lowercase()
        };
        self.rebuild_visible();
    }

    /// The number of transactions that match the current column filters.
    pub fn visible_count(&self) -> usize {
        self.visible.len()
    }

    /// Copies runtime column widths back after the user resizes a column.
    pub fn sync_widths(&mut self, widths: &[Pixels]) {
        for (slot, width) in self.widths.iter_mut().zip(widths.iter()) {
            *slot = *width;
        }
        // User-changed widths invalidate the last fit; recompute on next layout.
        self.fitted_table_width = None;
    }

    /// True when Details still needs to be sized for `table_width`.
    pub fn needs_details_fit(&self, table_width: Pixels) -> bool {
        match self.fitted_table_width {
            Some(fitted) => (fitted - table_width).abs() >= px(1.),
            None => table_width > px(0.),
        }
    }

    /// Grows or shrinks Details so the columns fill `table_width`.
    ///
    /// Returns whether any width changed. Other columns keep their current
    /// sizes; only Details absorbs leftover space or yields when the window
    /// is too narrow.
    pub fn fit_details_to(&mut self, table_width: Pixels) -> bool {
        let available = table_width - px(TABLE_CHROME_WIDTH);
        if available <= px(0.) {
            return false;
        }

        let others: Pixels = self
            .widths
            .iter()
            .enumerate()
            .filter(|(col_ix, _)| *col_ix != DETAILS_COLUMN)
            .map(|(_, width)| *width)
            .sum();
        let target = (available - others)
            .max(self.details_min_width)
            .min(px(DETAILS_MAX_WIDTH));
        self.fitted_table_width = Some(table_width);
        if (target - self.widths[DETAILS_COLUMN]).abs() < px(1.) {
            return false;
        }
        self.widths[DETAILS_COLUMN] = target;
        true
    }

    /// Paid In and Withdrawn totals for the currently visible rows.
    pub fn visible_amount_totals(&self) -> (String, String) {
        let mut paid_in = Money::zero();
        let mut withdrawn = Money::zero();
        for &transaction_ix in &self.visible {
            let transaction = &self.statement.transactions[transaction_ix];
            paid_in += transaction.paid_in();
            withdrawn += transaction.withdrawn();
        }
        (format_total(&paid_in), format_total(&withdrawn))
    }

    fn has_active_filter(&self) -> bool {
        self.filters.iter().any(|query| !query.is_empty())
    }

    fn rebuild_visible(&mut self) {
        let Self {
            statement,
            searchable,
            visible,
            filters,
            sort,
            ..
        } = self;

        visible.clear();
        visible.extend(
            searchable
                .iter()
                .enumerate()
                .filter(|(_, cells)| {
                    filters
                        .iter()
                        .zip(cells.iter())
                        .all(|(query, cell)| query.is_empty() || cell.contains(query.as_str()))
                })
                .map(|(index, _)| index),
        );

        let Some((column, direction)) = *sort else {
            return;
        };

        let transactions = &statement.transactions;
        visible.sort_by(|&left_ix, &right_ix| {
            let left = &transactions[left_ix];
            let right = &transactions[right_ix];
            let ordering = match column {
                0 => left.receipt_no().cmp(right.receipt_no()),
                1 => left
                    .completion_time()
                    .to_string()
                    .cmp(&right.completion_time().to_string()),
                2 => left.details().cmp(right.details()),
                3 => left
                    .transaction_status()
                    .to_string()
                    .cmp(&right.transaction_status().to_string()),
                4 => left.paid_in().amount().cmp(&right.paid_in().amount()),
                5 => left.withdrawn().amount().cmp(&right.withdrawn().amount()),
                6 => left.balance().amount().cmp(&right.balance().amount()),
                _ => Ordering::Equal,
            };
            match direction {
                ColumnSort::Ascending => ordering,
                ColumnSort::Descending => ordering.reverse(),
                ColumnSort::Default => Ordering::Equal,
            }
        });
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize) -> SharedString {
        let Some(transaction_ix) = self.visible.get(row_ix) else {
            return SharedString::from("");
        };
        let transaction = &self.statement.transactions[*transaction_ix];

        match col_ix {
            0 => transaction.receipt_no().into(),
            1 => transaction.completion_time().to_string().into(),
            2 => transaction.details().into(),
            3 => transaction.transaction_status().to_string().into(),
            4 => transaction.paid_in().to_string().into(),
            5 => transaction.withdrawn().to_string().into(),
            6 => transaction.balance().to_string().into(),
            _ => SharedString::from(""),
        }
    }
}

impl TableDelegate for TransactionsTable {
    fn columns_count(&self, _: &App) -> usize {
        COLUMN_COUNT
    }

    fn rows_count(&self, _: &App) -> usize {
        self.visible.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        let min_width = if col_ix == DETAILS_COLUMN {
            self.details_min_width
        } else {
            self.widths[col_ix] * 0.6
        };
        let column = Column::new(COLUMN_KEYS[col_ix], COLUMN_NAMES[col_ix])
            .width(self.widths[col_ix])
            .min_width(min_width)
            .resizable(true)
            .movable(false)
            .sortable();

        let column = if col_ix == DETAILS_COLUMN {
            column.max_width(px(DETAILS_MAX_WIDTH))
        } else {
            column
        };

        if col_ix >= FIRST_AMOUNT_COLUMN {
            column.text_right()
        } else {
            column
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        self.sort = match sort {
            ColumnSort::Default => None,
            direction => Some((col_ix, direction)),
        };
        self.rebuild_visible();
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let filter = self.filter_inputs[col_ix].clone();
        v_flex()
            .w_full()
            .gap_1()
            .justify_center()
            .child(
                h_flex()
                    .w_full()
                    .when(col_ix >= FIRST_AMOUNT_COLUMN, |this| this.justify_end())
                    .child(COLUMN_NAMES[col_ix]),
            )
            .when_some(filter, |this, filter| {
                this.child(
                    div()
                        .w_full()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(Input::new(&filter).xsmall().cleanable(true)),
                )
            })
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let cell = div()
            .w_full()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .when(col_ix == PAID_IN_COLUMN, |this| {
                this.text_color(cx.theme().success)
            })
            .when(col_ix == WITHDRAWN_COLUMN, |this| {
                this.text_color(cx.theme().danger)
            })
            .child(self.cell_text(row_ix, col_ix));

        if col_ix >= FIRST_AMOUNT_COLUMN {
            cell.text_right()
        } else {
            cell
        }
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let message = if self.has_active_filter() {
            "No transactions match these filters."
        } else {
            "This statement has no transactions."
        };

        h_flex()
            .size_full()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(message)
    }
}
