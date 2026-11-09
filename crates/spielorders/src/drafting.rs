//! The rules an editor's draft applies as the user types, before the draft is planned.
//! Planning the draft applies every rule again; these only keep what an editor shows
//! the answer the Package would give.
//!
//! Decision sources: Prototype the SpielOrders desktop experience; Define the Rust CLI
//! capability contract.

use crate::{
    Coded,
    creation::OrderCreationDefaults,
    metadata::OrderStatus,
    order::{item_total, order_total},
    order_edits::{date_ordered_after_status_change, item_number, parse_status},
    ports::LocalDate,
};

/// The Date Ordered and Status a new Order's draft starts with.
pub fn new_order_defaults(today: LocalDate) -> (String, &'static str) {
    let defaults = OrderCreationDefaults::new(&|| today);
    (defaults.date_ordered, defaults.status.code())
}

/// The Date Ordered a draft holds once its Status changes from `previous` to `next`:
/// entering Ordered takes today's date, which the user may then change.
pub fn draft_date_ordered(previous: &str, next: &str, current: &str, today: LocalDate) -> String {
    match parse_status(next) {
        Some(next) => {
            let previous = parse_status(previous).unwrap_or(OrderStatus::InPreparation);
            date_ordered_after_status_change(previous, next, current, &|| today)
        }
        None => current.to_owned(),
    }
}

/// One draft row's Item Total, or `None` where its numbers do not parse or their
/// product is not finite.
pub fn draft_item_total(quantity: &str, unit_price: &str) -> Option<f64> {
    Some(item_total(item_number(quantity)?, item_number(unit_price)?))
        .filter(|total| total.is_finite())
}

/// A draft's Order Total, or `None` where any row has no Item Total.
pub fn draft_order_total(item_totals: &[Option<f64>]) -> Option<f64> {
    let totals: Option<Vec<f64>> = item_totals.iter().copied().collect();
    Some(order_total(totals?.into_iter())).filter(|total| total.is_finite())
}
