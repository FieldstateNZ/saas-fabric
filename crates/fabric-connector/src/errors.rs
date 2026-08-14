//! Failures at the connector boundary.
//!
//! | Module | Responsibility |
//! |---|---|
//! | `connector_error` | The variants themselves |
//! | `operation_effect` | Whether a failed operation took effect |
//! | `rejection_status` | What a rejection's status code says about that |
//! | `error_classification` | Whose fault it was, and what may be said |
//!
//! `operation_effect` is the one to read first if you are mapping these to
//! HTTP: it is where the difference between "the write did not happen" and
//! "the write may have happened" is argued, and getting that wrong is how a
//! platform instructs a client to repeat a write it already committed.

mod connector_error;
mod error_classification;
mod operation_effect;
#[cfg(test)]
mod operation_effect_tests;
mod refusal_detail;
mod rejection_status;
#[cfg(test)]
mod rejection_status_tests;
mod unsupported_feature;
#[cfg(test)]
mod unsupported_feature_tests;

pub use connector_error::ConnectorError;
pub use operation_effect::OperationEffect;
pub use refusal_detail::RefusalDetail;
pub use rejection_status::rejection_effect;
pub use unsupported_feature::UnsupportedFeature;
