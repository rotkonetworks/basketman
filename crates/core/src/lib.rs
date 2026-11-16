//! Deterministic basketball management sim core.
//!
//! No I/O, no clock, no platform RNG: everything is driven by a seed, so the
//! same save produces the same results natively and in wasm. Keep it that way:
//! avoid `powf`/`exp`/`ln` and friends in sim code (libm differs per target).

pub mod arena;
pub mod assistant;
pub mod board;
pub mod books;
pub mod budget;
pub mod calendar;
pub mod career;
pub mod club;
pub mod command;
pub mod court;
pub mod days;
pub mod debt;
pub mod divisions;
pub mod dressing;
pub mod engine;
pub mod facility;
pub mod fame;
pub mod fans;
pub mod film;
pub mod finance;
pub mod fit;
pub mod health;
pub mod inbox;
pub mod income;
pub mod instructions;
pub mod league;
pub mod legal;
pub mod media;
pub mod ops;
pub mod player;
pub mod pricing;
pub mod projection;
pub mod replay;
pub mod reserves;
pub mod retire;
pub mod rng;
pub mod save;
pub mod scouting;
pub mod scrimmage;
pub mod season;
pub mod signature;
pub mod sponsor;
pub mod staff;
pub mod tactics;
pub mod tax;
pub mod training;
pub mod view;
pub mod view_more;

#[cfg(test)]
mod books_tests;
#[cfg(test)]
mod club_tests;
#[cfg(test)]
mod fame_tests;
#[cfg(test)]
mod signature_tests;
