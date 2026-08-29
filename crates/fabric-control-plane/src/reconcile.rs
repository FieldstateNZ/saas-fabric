//! Converging the identity provider onto desired state.
//!
//! # There is no schedule here any more
//!
//! This used to spawn a loop that swept every client on an interval, using a
//! service account's credential. That credential is gone (ADR 0012): the
//! platform holds no authority over the identity provider of its own, and acts
//! with an operator's.
//!
//! So a sweep happens when an operator does something — writes a client's
//! identity, or asks for one — and there is nothing that can converge anything
//! at three in the morning. That is a real loss and it is the trade the ADR
//! argues for: the identity provider's console is published on no plane, so
//! changes made outside SaaS Fabric are largely prevented rather than merely
//! noticed afterwards.

mod pass;
#[cfg(test)]
mod pass_tests;

pub(crate) use pass::run;
