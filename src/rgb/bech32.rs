// LNP/BP Core Library implementing LNPBP specifications & standards
// Written in 2020 by
//     Dr. Maxim Orlovsky <orlovsky@pandoracore.com>
//
// To the extent possible under law, the author(s) have dedicated all
// copyright and related and neighboring rights to this software to
// the public domain worldwide. This software is distributed without
// any warranty.
//
// You should have received a copy of the MIT License
// along with this software.
// If not, see <https://opensource.org/licenses/MIT>.

use bech32::{self, FromBase32, ToBase32};
use core::fmt::{Display, Formatter};
use core::str::FromStr;
use deflate::{write::DeflateEncoder, Compression};
use std::convert::{TryFrom, TryInto};

use crate::rgb::{
    seal, Anchor, ContractId, Disclosure, Extension, Genesis, Schema, SchemaId,
    Transition,
};
use crate::strict_encoding::{
    self, strict_decode, strict_encode, StrictDecode, StrictEncode,
};

/// Bech32 representation of generic RGB data, that can be generated from
/// some string basing on Bech32 HRP value.
#[derive(Clone, Debug, From)]
pub enum Bech32 {
    /// Blinded UTXO for assigning RGB state to.
    ///
    /// HRP: `utxob`
    #[from]
    // TODO: (new) Remove it once invoice implementation will be completed
    BlindedUtxo(seal::Confidential),

    /// RGB Schema ID (hash of the schema data).
    ///
    /// HRP: `sch`
    #[from]
    SchemaId(SchemaId),

    /// RGB Schema raw data (hash of the genesis).
    ///
    /// HRP: `schema`
    #[from]
    Schema(Schema),

    /// RGB Contract ID (hash of the genesis).
    ///
    /// HRP: `rgb`
    #[from]
    ContractId(ContractId),

    /// RGB Contract genesis raw data
    ///
    /// HRP: `genesis`
    #[from]
    Genesis(Genesis),

    /// Raw data of state transition under some RGB contract
    ///
    /// HRP: `transition`
    #[from]
    Transition(Transition),

    /// Raw data of state extension under some RGB contract
    ///
    /// HRP: `statex`
    #[from]
    Extension(Extension),

    /// Anchor data for some dterministic bitcoin commitment
    ///
    /// HRP: `anchor`
    #[from]
    Anchor(Anchor),

    /// Disclosure data revealing some specific confidential information about
    /// RGB contract
    ///
    /// HRP: `disclosure`
    #[from]
    Disclosure(Disclosure),

    /// Binary data for unknown Bech32 HRPs
    Other(String, Vec<u8>),
}

impl Bech32 {
    /// HRP for a Bech32-encoded blinded UTXO data
    pub const HRP_OUTPOINT: &'static str = "utxob";

    /// Bech32 HRP for RGB schema ID encoding
    pub const HRP_SCHEMA_ID: &'static str = "sch";
    /// Bech32 HRP for RGB contract ID encoding
    pub const HRP_CONTRACT_ID: &'static str = "rgb";

    /// HRP for a Bech32-encoded raw RGB schema data
    pub const HRP_SCHEMA: &'static str = "schema";
    /// HRP for a Bech32-encoded raw RGB contract genesis data
    pub const HRP_GENESIS: &'static str = "genesis";
    /// HRP for a Bech32-encoded raw RGB state transition data
    pub const HRP_TRANSITION: &'static str = "transition";
    /// HRP for a Bech32-encoded raw RGB state extension data
    pub const HRP_EXTENSION: &'static str = "statex";
    /// HRP for a Bech32-encoded deterministic bitcoin commitments anchor data
    pub const HRP_ANCHOR: &'static str = "anchor";
    /// HRP for a Bech32-encoded RGB disclosure data
    pub const HRP_DISCLOSURE: &'static str = "disclosure";

    pub(self) const RAW_DATA_ENCODING_PLAIN: u8 = 0u8;
    pub(self) const RAW_DATA_ENCODING_DEFLATE: u8 = 1u8;

    /// Encoder for v0 of raw data encoding algorithm. Uses plain strict encoded
    /// data
    pub(self) fn plain_encode(
        obj: &impl StrictEncode<Error = strict_encoding::Error>,
    ) -> Result<Vec<u8>, Error> {
        // We initialize writer with a version byte, indicating plain
        // algorithm used
        let mut writer = vec![Self::RAW_DATA_ENCODING_PLAIN];
        obj.strict_encode(&mut writer)?;
        Ok(writer)
    }

    /// Encoder for v1 of raw data encoding algorithm. Uses deflate
    pub(self) fn deflate_encode(
        obj: &impl StrictEncode<Error = strict_encoding::Error>,
    ) -> Result<Vec<u8>, Error> {
        // We initialize writer with a version byte, indicating deflation
        // algorithm used
        let writer = vec![Self::RAW_DATA_ENCODING_DEFLATE];
        let mut encoder = DeflateEncoder::new(writer, Compression::Best);
        obj.strict_encode(&mut encoder)?;
        Ok(encoder.finish().map_err(|_| Error::DeflateEncoding)?)
    }

    pub(self) fn raw_decode<T>(data: &impl AsRef<[u8]>) -> Result<T, Error>
    where
        T: StrictDecode<Error = strict_encoding::Error>,
    {
        let mut reader = data.as_ref();
        Ok(match u8::strict_decode(&mut reader)? {
            Self::RAW_DATA_ENCODING_PLAIN => T::strict_decode(&mut reader)?,
            Self::RAW_DATA_ENCODING_DEFLATE => {
                println!("{:#x?}", reader);
                let decoded = inflate::inflate_bytes(&mut reader)
                    .map_err(|e| Error::InflateError(e))?;
                T::strict_decode(&decoded[..])?
            }
            unknown_ver => Err(Error::UnknownRawDataEncoding(unknown_ver))?,
        })
    }
}

/// Trait for types which data can be represented in form of Bech32 string
pub trait ToBech32 {
    /// Returns [`Bech32`] enum variant for this specific type
    fn to_bech32(&self) -> Bech32;

    /// Converts type to it's Bech32-encoded representation. Default
    /// implementation constructs [`Bech32`] object and converts it to string.
    fn to_bech32_string(&self) -> String {
        self.to_bech32().to_string()
    }
}

/// Trait for types that can be reconstructed from Bech32-encoded data tagged
/// with specific HRP
pub trait FromBech32
where
    Self: Sized,
{
    /// Unwraps [`Bech32`] enum data into a concrete type, if any, or fails with
    /// [`Error::WrongType`] otherwise
    fn from_bech32(bech32: Bech32) -> Result<Self, Error>;

    /// Tries to read Bech32-encoded data from `s` argument, checks it's type
    /// and constructs object if HRP corresponds to the type implementing this
    /// trait. Fails with [`Error`] type
    fn from_bech32_str(s: &str) -> Result<Self, Error> {
        Self::from_bech32(s.parse()?)
    }
}

impl<T> ToBech32 for T
where
    T: Into<Bech32> + Clone,
{
    fn to_bech32(&self) -> Bech32 {
        self.clone().into()
    }
}

impl<T> FromBech32 for T
where
    T: TryFrom<Bech32, Error = Error>,
{
    fn from_bech32(bech32: Bech32) -> Result<Self, Error> {
        Self::try_from(bech32)
    }
}

/// Errors generated by Bech32 conversion functions (both parsing and
/// type-specific conversion errors)
#[derive(Clone, PartialEq, Debug, Display, From, Error)]
#[display(doc_comments)]
pub enum Error {
    /// Bech32 string parse error: {_0}
    #[from]
    Bech32Error(::bech32::Error),

    /// Payload data parse error: {_0}
    #[from]
    WrongData(strict_encoding::Error),

    /// Requested object type does not match used Bech32 HRP
    WrongType,

    /// Provided raw data use unknown encoding version {_0}
    UnknownRawDataEncoding(u8),

    /// Can not encode raw data with DEFLATE algorithm
    DeflateEncoding,

    /// Error inflating compressed data from payload: {_0}
    InflateError(String),
}

impl From<Error> for ::core::fmt::Error {
    fn from(_: Error) -> Self {
        ::core::fmt::Error
    }
}

impl TryFrom<Bech32> for seal::Confidential {
    type Error = Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        match bech32 {
            Bech32::BlindedUtxo(obj) => Ok(obj),
            _ => Err(Error::WrongType),
        }
    }
}

impl TryFrom<Bech32> for ContractId {
    type Error = Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        match bech32 {
            Bech32::ContractId(obj) => Ok(obj),
            _ => Err(Error::WrongType),
        }
    }
}

impl TryFrom<Bech32> for SchemaId {
    type Error = Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        match bech32 {
            Bech32::SchemaId(obj) => Ok(obj),
            _ => Err(Error::WrongType),
        }
    }
}

impl TryFrom<Bech32> for Schema {
    type Error = Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        match bech32 {
            Bech32::Schema(obj) => Ok(obj),
            _ => Err(Error::WrongType),
        }
    }
}

impl TryFrom<Bech32> for Genesis {
    type Error = Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        match bech32 {
            Bech32::Genesis(obj) => Ok(obj),
            _ => Err(Error::WrongType),
        }
    }
}

impl TryFrom<Bech32> for Extension {
    type Error = Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        match bech32 {
            Bech32::Extension(obj) => Ok(obj),
            _ => Err(Error::WrongType),
        }
    }
}

impl TryFrom<Bech32> for Transition {
    type Error = Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        match bech32 {
            Bech32::Transition(obj) => Ok(obj),
            _ => Err(Error::WrongType),
        }
    }
}

impl TryFrom<Bech32> for Anchor {
    type Error = Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        match bech32 {
            Bech32::Anchor(obj) => Ok(obj),
            _ => Err(Error::WrongType),
        }
    }
}

impl TryFrom<Bech32> for Disclosure {
    type Error = Error;

    fn try_from(bech32: Bech32) -> Result<Self, Self::Error> {
        match bech32 {
            Bech32::Disclosure(obj) => Ok(obj),
            _ => Err(Error::WrongType),
        }
    }
}

impl FromStr for Bech32 {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (hrp, data) = bech32::decode(&s)?;
        let data = Vec::<u8>::from_base32(&data)?;

        use bitcoin::hashes::hex::ToHex;
        println!("{}", data.to_hex());

        Ok(match hrp {
            x if x == Self::HRP_OUTPOINT => {
                Self::BlindedUtxo(strict_decode(&data)?)
            }
            x if x == Self::HRP_SCHEMA_ID => {
                Self::SchemaId(strict_decode(&data)?)
            }
            x if x == Self::HRP_CONTRACT_ID => {
                Self::ContractId(strict_decode(&data)?)
            }
            x if x == Self::HRP_SCHEMA => {
                Self::Schema(Bech32::raw_decode(&data)?)
            }
            x if x == Self::HRP_GENESIS => {
                Self::Genesis(Bech32::raw_decode(&data)?)
            }
            x if x == Self::HRP_EXTENSION => {
                Self::Extension(Bech32::raw_decode(&data)?)
            }
            x if x == Self::HRP_TRANSITION => {
                Self::Transition(Bech32::raw_decode(&data)?)
            }
            x if x == Self::HRP_ANCHOR => {
                Self::Anchor(Bech32::raw_decode(&data)?)
            }
            x if x == Self::HRP_DISCLOSURE => {
                Self::Disclosure(Bech32::raw_decode(&data)?)
            }
            other => Self::Other(other, data),
        })
    }
}

impl Display for Bech32 {
    fn fmt(&self, f: &mut Formatter<'_>) -> ::core::fmt::Result {
        let (hrp, data) = match self {
            Self::BlindedUtxo(obj) => (Self::HRP_OUTPOINT, strict_encode(obj)?),
            Self::SchemaId(obj) => (Self::HRP_SCHEMA_ID, strict_encode(obj)?),
            Self::ContractId(obj) => {
                (Self::HRP_CONTRACT_ID, strict_encode(obj)?)
            }
            Self::Schema(obj) => {
                (Self::HRP_SCHEMA, Bech32::deflate_encode(obj)?)
            }
            Self::Genesis(obj) => {
                (Self::HRP_GENESIS, Bech32::deflate_encode(obj)?)
            }
            Self::Extension(obj) => {
                (Self::HRP_EXTENSION, Bech32::deflate_encode(obj)?)
            }
            Self::Transition(obj) => {
                (Self::HRP_TRANSITION, Bech32::deflate_encode(obj)?)
            }
            Self::Anchor(obj) => (Self::HRP_ANCHOR, Bech32::plain_encode(obj)?),
            Self::Disclosure(obj) => {
                (Self::HRP_DISCLOSURE, Bech32::deflate_encode(obj)?)
            }
            Self::Other(hrp, obj) => (hrp.as_ref(), obj.clone()),
        };
        let b = ::bech32::encode(hrp, data.to_base32())
            .map_err(|_| ::core::fmt::Error)?;
        b.fmt(f)
    }
}

impl FromStr for Schema {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Bech32::from_str(s)?.try_into()
    }
}

impl FromStr for Genesis {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Bech32::from_str(s)?.try_into()
    }
}

impl FromStr for Extension {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Bech32::from_str(s)?.try_into()
    }
}

impl FromStr for Transition {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Bech32::from_str(s)?.try_into()
    }
}

impl FromStr for Anchor {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Bech32::from_str(s)?.try_into()
    }
}

impl FromStr for Disclosure {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Bech32::from_str(s)?.try_into()
    }
}

// TODO: Enable after removal of the default `Display` and `FromStr`
//       implementations for hash-derived types
/*
impl FromStr for seal::Confidential {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Bech32::from_str(s).try_into()
    }
}

impl FromStr for ContractId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Bech32::from_str(s).try_into()
    }
}

impl FromStr for SchemaId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Bech32::from_str(s).try_into()
    }
}

impl Display for seal::Confidential {
    fn fmt(&self, f: &mut Formatter<'_>) -> ::core::fmt::Result {
        Bech32::Outpoint(self.clone()).fmt(f)
    }
}

impl Display for ContractId {
    fn fmt(&self, f: &mut Formatter<'_>) -> ::core::fmt::Result {
        Bech32::ContractId(self.clone()).fmt(f)
    }
}

impl Display for SchemaId {
    fn fmt(&self, f: &mut Formatter<'_>) -> ::core::fmt::Result {
        Bech32::SchemaId(self.clone()).fmt(f)
    }
}
 */

impl Display for Schema {
    fn fmt(&self, f: &mut Formatter<'_>) -> ::core::fmt::Result {
        Bech32::Schema(self.clone()).fmt(f)
    }
}

impl Display for Genesis {
    fn fmt(&self, f: &mut Formatter<'_>) -> ::core::fmt::Result {
        Bech32::Genesis(self.clone()).fmt(f)
    }
}

impl Display for Transition {
    fn fmt(&self, f: &mut Formatter<'_>) -> ::core::fmt::Result {
        Bech32::Transition(self.clone()).fmt(f)
    }
}

impl Display for Anchor {
    fn fmt(&self, f: &mut Formatter<'_>) -> ::core::fmt::Result {
        Bech32::Anchor(self.clone()).fmt(f)
    }
}

impl Display for Disclosure {
    fn fmt(&self, f: &mut Formatter<'_>) -> ::core::fmt::Result {
        Bech32::Disclosure(self.clone()).fmt(f)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bech32() {
        let obj = Transition::default();
        let bech32 = format!("{}", obj);
        assert_eq!(bech32, "transition1q935qqsqpr0f9t");
        let decoded = Transition::from_bech32_str(&bech32).unwrap();
        assert_eq!(obj, decoded);
    }
}
