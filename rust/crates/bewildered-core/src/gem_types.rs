//! Gem kinds used throughout the game.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

/// All supported gem types in Bewildered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GemKind {
    Circle,
    Triangle,
    Square,
    Diamond,
    Star,
    Cross,
}

impl fmt::Display for GemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GemKind::Circle => write!(f, "circle"),
            GemKind::Triangle => write!(f, "triangle"),
            GemKind::Square => write!(f, "square"),
            GemKind::Diamond => write!(f, "diamond"),
            GemKind::Star => write!(f, "star"),
            GemKind::Cross => write!(f, "cross"),
        }
    }
}

impl FromStr for GemKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "circle" => Ok(GemKind::Circle),
            "triangle" => Ok(GemKind::Triangle),
            "square" => Ok(GemKind::Square),
            "diamond" => Ok(GemKind::Diamond),
            "star" => Ok(GemKind::Star),
            "cross" => Ok(GemKind::Cross),
            _ => Err(format!("Unknown gem kind: {}", s)),
        }
    }
}

impl Serialize for GemKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for GemKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        GemKind::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<u8> for GemKind {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(GemKind::Circle),
            1 => Ok(GemKind::Triangle),
            2 => Ok(GemKind::Square),
            3 => Ok(GemKind::Diamond),
            4 => Ok(GemKind::Star),
            5 => Ok(GemKind::Cross),
            _ => Err(()),
        }
    }
}
