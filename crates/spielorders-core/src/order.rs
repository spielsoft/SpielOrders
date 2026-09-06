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

/// Which calculated total is not a finite number.
///
/// **Two answers rather than one**, because a product that overflows and a sum of finite
/// products that overflows are different arithmetic in the same Order: an Item Total is
/// one Item's Quantity times its Unit Price, and the Order Total is the sum of them. A
/// caller correcting one is not looking at the other.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CalculatedTotal {
    ItemTotal,
    OrderTotal,
}

impl CalculatedTotal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ItemTotal => "item_total",
            Self::OrderTotal => "order_total",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrderValidationFailure {
    InvalidDateOrdered,
    /// A calculated total is not a finite number.
    ///
    /// **Overflow is the only route to one, so no stored value is at fault.** A
    /// non-finite Quantity or Unit Price never loads at all — `FiniteNumber`'s
    /// deserializer refuses it, and JSON cannot spell one — so an Order whose record
    /// parsed carries finite numbers, and what is not finite is the product or the sum
    /// this validation computed from them. The failure therefore names the total rather
    /// than a field of the record: there is no field to name.
    NonFiniteTotal {
        total: CalculatedTotal,
    },
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
        // A total this validation could not calculate is not a total, and an Order whose
        // money cannot be added up is not one this application will report a figure for.
        // Refusing here is what makes every `ValidatedOrder` carry finite calculations,
        // which is what every caller that displays or sums them relies on.
        if item_totals.iter().any(|total| !total.is_finite()) {
            return Err(vec![OrderValidationFailure::NonFiniteTotal {
                total: CalculatedTotal::ItemTotal,
            }]);
        }
        let order_total: f64 = item_totals.iter().sum();
        if !order_total.is_finite() {
            return Err(vec![OrderValidationFailure::NonFiniteTotal {
                total: CalculatedTotal::OrderTotal,
            }]);
        }

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
    pub(crate) fn is_real_gregorian(&self) -> bool {
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
