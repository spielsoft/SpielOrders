//! Order-local semantic validation and calculations.

use crate::metadata::{DateOrdered, OrderDocument, normalize_optional_single_line};

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedOrder {
    order: OrderDocument,
    item_totals: Vec<f64>,
    order_total: f64,
}

impl ValidatedOrder {
    pub fn order(&self) -> &OrderDocument {
        &self.order
    }

    pub fn item_totals(&self) -> &[f64] {
        &self.item_totals
    }

    pub fn order_total(&self) -> f64 {
        self.order_total
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderValidationFailure {
    InvalidDateOrdered,
}

impl OrderDocument {
    pub fn validate(&self) -> Result<ValidatedOrder, Vec<OrderValidationFailure>> {
        if !self.date_ordered.is_real_gregorian() {
            return Err(vec![OrderValidationFailure::InvalidDateOrdered]);
        }

        let mut order = self.clone();
        normalize_single_line_facts(&mut order);
        let item_totals = order
            .items
            .iter()
            .map(|item| item.quantity.get() * item.unit_price.get())
            .collect::<Vec<_>>();
        let order_total = item_totals.iter().sum();

        Ok(ValidatedOrder {
            order,
            item_totals,
            order_total,
        })
    }
}

fn normalize_single_line_facts(order: &mut OrderDocument) {
    order.title.normalize_single_line();
    normalize_optional_single_line(&mut order.orderer);
    normalize_optional_single_line(&mut order.ordering_institution);
    normalize_optional_single_line(&mut order.project);
    normalize_optional_single_line(&mut order.order_group);
    for item in &mut order.items {
        item.description.normalize_single_line();
        normalize_optional_single_line(&mut item.catalog_number);
    }
}

impl DateOrdered {
    fn is_real_gregorian(&self) -> bool {
        let bytes = self.as_str().as_bytes();
        let year = u16::from(bytes[0] - b'0') * 1_000
            + u16::from(bytes[1] - b'0') * 100
            + u16::from(bytes[2] - b'0') * 10
            + u16::from(bytes[3] - b'0');
        let month = (bytes[5] - b'0') * 10 + (bytes[6] - b'0');
        let day = (bytes[8] - b'0') * 10 + (bytes[9] - b'0');
        let leap_year =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days_in_month = match month {
            2 if leap_year => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };

        year != 0 && day <= days_in_month
    }
}
