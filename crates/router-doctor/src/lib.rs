//! Read-only, explicitly scoped diagnostics and redacted audit export.
#![deny(unsafe_code)]

mod audit;
mod diagnose;
#[allow(unsafe_code)]
mod files;

pub use audit::{AuditInput, build_audit_bundle, verify_audit_bundle};
pub use diagnose::diagnose;

pub const MAX_ARTIFACTS: usize = 128;
pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum DoctorError {
    #[error("doctor.input.invalid")]
    Invalid,
    #[error("doctor.path.rejected")]
    Path,
    #[error("doctor.file.unavailable")]
    Read,
    #[error("doctor.input.limit")]
    Limit,
    #[error("doctor.schema.invalid")]
    Schema,
    #[error("doctor.audit.integrity")]
    Integrity,
    #[error("doctor.platform.unsupported")]
    Unsupported,
}
pub type Result<T> = std::result::Result<T, DoctorError>;
