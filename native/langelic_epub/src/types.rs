use rustler::{
    Binary, Decoder, Encoder, Env, NifResult, NifStruct, NifUnitEnum, OwnedBinary, Term,
};
use std::collections::HashMap;

/// Wraps `Vec<u8>` so it encodes as an Erlang binary instead of the default
/// list encoding. Chapter and asset bodies can be large — a list of integer
/// terms would be ~8× the memory and force decoders to rebuild a binary.
#[derive(Debug, Clone, Default)]
pub struct Bytes(pub Vec<u8>);

impl Encoder for Bytes {
    fn encode<'a>(&self, env: Env<'a>) -> Term<'a> {
        let mut owned = OwnedBinary::new(self.0.len()).expect("OwnedBinary allocation failed");
        owned.as_mut_slice().copy_from_slice(&self.0);
        Binary::from_owned(owned, env).to_term(env)
    }
}

impl<'a> Decoder<'a> for Bytes {
    fn decode(term: Term<'a>) -> NifResult<Self> {
        let bin: Binary = term.decode()?;
        Ok(Bytes(bin.as_slice().to_vec()))
    }
}

#[derive(NifStruct, Debug, Clone)]
#[module = "LangelicEpub.Chapter"]
pub struct Chapter {
    pub id: String,
    pub file_name: String,
    pub title: Option<String>,
    pub media_type: String,
    pub data: Bytes,
}

#[derive(NifStruct, Debug, Clone)]
#[module = "LangelicEpub.Asset"]
pub struct Asset {
    pub id: String,
    pub file_name: String,
    pub media_type: String,
    pub data: Bytes,
}

#[derive(NifStruct, Debug, Clone)]
#[module = "LangelicEpub.NavItem"]
pub struct NavItem {
    pub title: String,
    pub href: String,
    pub children: Vec<NavItem>,
}

#[derive(NifStruct, Debug, Clone)]
#[module = "LangelicEpub.Document"]
pub struct Document {
    pub title: String,
    pub creators: Vec<String>,
    pub language: Option<String>,
    pub identifier: String,
    pub publisher: Option<String>,
    pub date: Option<String>,
    pub description: Option<String>,
    pub rights: Option<String>,
    pub metadata: HashMap<String, Vec<String>>,
    pub spine: Vec<Chapter>,
    pub assets: Vec<Asset>,
    pub toc: Vec<NavItem>,
    pub cover_asset_id: Option<String>,
    pub version: String,
}

#[derive(NifUnitEnum, Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum ErrorKind {
    InvalidZip,
    InvalidMimetype,
    MissingContainer,
    MissingOpf,
    MalformedOpf,
    Io,
    InvalidChapter,
    MissingRequiredField,
    DuplicateId,
    Panic,
}
