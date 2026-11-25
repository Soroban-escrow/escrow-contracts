#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    reason = "tests may panic on unexpected results and use plain arithmetic to compute expected values"
)]

mod admin;
mod bounties;
mod budgets;
mod dispute;
mod errors;
mod fund;
mod invariants;
mod quotes;
mod refund;
mod release;
mod setup;
mod token_fee;
mod ttl;
