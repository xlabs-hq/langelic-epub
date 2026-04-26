use crate::types::ErrorKind;
use rustler::{Encoder, Env, Term};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid zip archive: {0}")]
    InvalidZip(String),

    #[error("invalid mimetype: {0}")]
    InvalidMimetype(String),

    #[error("missing META-INF/container.xml")]
    MissingContainer,

    #[error("OPF file not found at {0}")]
    MissingOpf(String),

    #[error("malformed OPF: {0}")]
    MalformedOpf(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("invalid chapter {0}: {1}")]
    InvalidChapter(String, String),

    #[error("missing required field: {0}")]
    MissingRequiredField(&'static str),

    #[error("duplicate id: {0}")]
    DuplicateId(String),

    #[error("rust panic: {0}")]
    Panic(String),
}

impl AppError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            AppError::InvalidZip(_) => ErrorKind::InvalidZip,
            AppError::InvalidMimetype(_) => ErrorKind::InvalidMimetype,
            AppError::MissingContainer => ErrorKind::MissingContainer,
            AppError::MissingOpf(_) => ErrorKind::MissingOpf,
            AppError::MalformedOpf(_) => ErrorKind::MalformedOpf,
            AppError::Io(_) => ErrorKind::Io,
            AppError::InvalidChapter(_, _) => ErrorKind::InvalidChapter,
            AppError::MissingRequiredField(_) => ErrorKind::MissingRequiredField,
            AppError::DuplicateId(_) => ErrorKind::DuplicateId,
            AppError::Panic(_) => ErrorKind::Panic,
        }
    }
}

/// Encode an `AppError` as `{:error, %LangelicEpub.Error{kind: atom, message: binary}}`.
pub fn encode_error<'a>(env: Env<'a>, err: &AppError) -> Term<'a> {
    let kind = err.kind();
    let message = err.to_string();
    let error_struct = ErrorStruct { kind, message };
    (rustler::types::atom::error(), error_struct).encode(env)
}

#[derive(rustler::NifStruct)]
#[module = "LangelicEpub.Error"]
struct ErrorStruct {
    kind: ErrorKind,
    message: String,
}
