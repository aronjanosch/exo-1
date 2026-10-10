//! customers_core: named buyers with taste and relationship (#168), without engine types. A
//! customer orders from the wholesaler by a seeded rhythm; an order is the domain event
//! `OrderPlaced`, which the jobs system turns into an offer (the systems never call each other).
//! The end of the job comes back as `OrderSettled`: what arrived, in what condition, in time.
//! That moves the customer's relationship (0 to 5), and the relationship moves how often they
//! order and how much they pay.
//!
//! - `customer`: the `customer` record, the `price_table` record and the checked loader.
//! - `state`: `Customers`, the seeded orders, satisfaction, relationship, neglect, the save section.
//!
//! No mistake loses a customer for good: neglect only sinks the relationship to a floor, and work
//! brings it back. Prices of the wholesaler, the customers and the courier jobs sit in one table.
pub mod customer;
pub mod state;

pub use customer::{Customer, CustomerContent, CustomerId, PriceTable};
pub use state::{Customers, DECAY_PERIOD_S, DECAY_STEP, NEGLECT_FLOOR, Outcome, pay_factor, rhythm_factor, satisfaction};
