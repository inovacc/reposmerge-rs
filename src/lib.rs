//! reposmerge — faithful 1:1 Rust port of github.com/inovacc/reposmerge.
//!
//! Modules are declared here as each is ported, in dependency order:
//! model → gitx → fingerprint → group → discover → report → safety →
//! strategy → consolidate → app → (cmd = src/main.rs).

pub mod model;
pub mod gitx;
pub mod fingerprint;
// pub mod group;
// pub mod discover;
// pub mod report;
// pub mod safety;
// pub mod strategy;
// pub mod consolidate;
// pub mod app;
