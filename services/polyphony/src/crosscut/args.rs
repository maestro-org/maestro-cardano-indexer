use pallas::network::miniprotocols::{
    Point,
    MAINNET_MAGIC,
    TESTNET_MAGIC,
    // PREVIEW_MAGIC, PRE_PRODUCTION_MAGIC,
};
use serde::{Deserialize, Serialize};
use std::{ops::Deref, str::FromStr};

// TODO: use from pallas once available
pub const PRE_PRODUCTION_MAGIC: u64 = 1;
pub const PREVIEW_MAGIC: u64 = 2;

/// A serialization-friendly chain Point struct using a hex-encoded hash
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PointArg {
    Origin,
    Specific(u64, String),
}

impl TryInto<Point> for PointArg {
    type Error = crate::Error;

    fn try_into(self) -> Result<Point, Self::Error> {
        match self {
            PointArg::Origin => Ok(Point::Origin),
            PointArg::Specific(slot, hash_hex) => {
                let hash = hex::decode(&hash_hex)
                    .map_err(|_| Self::Error::message("can't decode point hash hex value"))?;

                Ok(Point::Specific(slot, hash))
            }
        }
    }
}

impl From<Point> for PointArg {
    fn from(other: Point) -> Self {
        match other {
            Point::Origin => PointArg::Origin,
            Point::Specific(slot, hash) => PointArg::Specific(slot, hex::encode(hash)),
        }
    }
}

impl FromStr for PointArg {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            x if s.contains(',') => {
                let mut parts: Vec<_> = x.split(',').collect();
                let slot = parts
                    .remove(0)
                    .parse()
                    .map_err(|_| Self::Err::message("can't parse slot number"))?;

                let hash = parts.remove(0).to_owned();
                Ok(PointArg::Specific(slot, hash))
            }
            "origin" => Ok(PointArg::Origin),
            _ => Err(Self::Err::message(
                "Can't parse chain point value, expecting `slot,hex-hash` format",
            )),
        }
    }
}

#[allow(clippy::to_string_trait_impl)]
impl ToString for PointArg {
    fn to_string(&self) -> String {
        match self {
            PointArg::Origin => "origin".to_string(),
            PointArg::Specific(slot, hash) => format!("{},{}", slot, hash),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MagicArg(pub u64);

impl Deref for MagicArg {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for MagicArg {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let m = match s {
            "testnet" => MagicArg(TESTNET_MAGIC),
            "mainnet" => MagicArg(MAINNET_MAGIC),
            "preview" => MagicArg(PREVIEW_MAGIC),
            "preprod" => MagicArg(PRE_PRODUCTION_MAGIC),
            _ => MagicArg(u64::from_str(s).map_err(|_| "can't parse magic value")?),
        };

        Ok(m)
    }
}

impl Default for MagicArg {
    fn default() -> Self {
        Self(MAINNET_MAGIC)
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", content = "value")]
pub enum IntersectConfig {
    Tip,
    Origin,
    Point(u64, String),
}

impl IntersectConfig {
    pub fn get_point(&self) -> Option<Point> {
        match self {
            IntersectConfig::Point(slot, hash) => {
                let hash = hex::decode(hash).expect("valid hex hash");
                Some(Point::Specific(*slot, hash))
            }
            _ => None,
        }
    }
}
