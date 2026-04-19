//! Rustler NIF for EPUB read/write.
//!
//! Both `parse` and `build` run on the `DirtyCpu` scheduler:
//!   * a 5MB EPUB takes 50–200ms to parse, well past the 1ms NIF guideline;
//!   * both operations are pure CPU on bytes already in memory.
//!
//! `catch_panic` wraps every invocation in `std::panic::catch_unwind` so that
//! a buggy EPUB cannot crash the BEAM node — panics are converted to a
//! `%LangelicEpub.Error{kind: :panic}` error.

mod error;
mod opf;
mod reader;
mod types;
mod writer;

use error::{encode_error, AppError};
use rustler::{Binary, Encoder, Env, NifResult, OwnedBinary, Term};
use types::Document;

#[rustler::nif(schedule = "DirtyCpu")]
fn parse<'a>(env: Env<'a>, epub_bytes: Binary<'a>) -> NifResult<Term<'a>> {
    let bytes = epub_bytes.as_slice().to_vec();
    let result = catch_panic(move || reader::parse(&bytes));
    match result {
        Ok(doc) => Ok((rustler::types::atom::ok(), doc).encode(env)),
        Err(err) => Ok(encode_error(env, &err)),
    }
}

#[rustler::nif(schedule = "DirtyCpu")]
fn build<'a>(env: Env<'a>, doc: Document) -> NifResult<Term<'a>> {
    let result: Result<Vec<u8>, AppError> = catch_panic(move || writer::build(&doc));
    match result {
        Ok(bytes) => {
            let mut bin = OwnedBinary::new(bytes.len()).ok_or(rustler::Error::Term(Box::new(
                "failed to allocate binary",
            )))?;
            bin.as_mut_slice().copy_from_slice(&bytes);
            Ok((rustler::types::atom::ok(), Binary::from_owned(bin, env)).encode(env))
        }
        Err(err) => Ok(encode_error(env, &err)),
    }
}

fn catch_panic<T, F>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError>,
{
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(payload) => {
            let msg = if let Some(s) = payload.downcast_ref::<&str>() {
                format!("rust panic: {}", s)
            } else if let Some(s) = payload.downcast_ref::<String>() {
                format!("rust panic: {}", s)
            } else {
                "rust panic with non-string payload".to_string()
            };
            Err(AppError::Panic(msg))
        }
    }
}

rustler::init!("Elixir.LangelicEpub.Native");
