use serde::{Deserialize, Deserializer, Serialize};
use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueErrorCode {
    NonFinite,
    Negative,
}

impl ValueErrorCode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NonFinite => "value.non_finite",
            Self::Negative => "value.negative",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueError {
    code: ValueErrorCode,
    value_type: &'static str,
}

impl ValueError {
    #[must_use]
    pub const fn code(self) -> ValueErrorCode {
        self.code
    }

    #[must_use]
    pub const fn value_type(self) -> &'static str {
        self.value_type
    }
}

impl fmt::Display for ValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} for {}", self.code.as_str(), self.value_type)
    }
}

impl Error for ValueError {}

macro_rules! finite_scalar {
    ($name:ident, $non_negative:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(f64);

        impl $name {
            pub fn try_new(value: f64) -> Result<Self, ValueError> {
                if !value.is_finite() {
                    return Err(ValueError {
                        code: ValueErrorCode::NonFinite,
                        value_type: stringify!($name),
                    });
                }
                let value = if value == 0.0 { 0.0 } else { value };
                if $non_negative && value < 0.0 {
                    return Err(ValueError {
                        code: ValueErrorCode::Negative,
                        value_type: stringify!($name),
                    });
                }
                Ok(Self(value))
            }

            #[must_use]
            pub const fn get(self) -> f64 {
                self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = f64::deserialize(deserializer)?;
                Self::try_new(value).map_err(serde::de::Error::custom)
            }
        }
    };
}

finite_scalar!(LengthMeters, true);
finite_scalar!(CoordinateMeters, false);
finite_scalar!(SpeedMetersPerSecond, true);
finite_scalar!(DurationSeconds, true);
finite_scalar!(FlowRatePerHour, true);
finite_scalar!(AngleRadians, false);
finite_scalar!(ToleranceMeters, true);
finite_scalar!(CurvaturePerMeter, false);
finite_scalar!(CurvatureTolerancePerMeter, true);

/// A canonical planar heading in radians in the half-open range `[0, 2π)`.
///
/// Unlike [`AngleRadians`], a heading describes an orientation rather than a
/// signed rotation. Canonicalization ensures equivalent full-turn orientations
/// have one serialized representation for future semantic hashing.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct HeadingRadians(f64);

impl HeadingRadians {
    pub fn try_new(value: f64) -> Result<Self, ValueError> {
        if !value.is_finite() {
            return Err(ValueError {
                code: ValueErrorCode::NonFinite,
                value_type: stringify!(HeadingRadians),
            });
        }
        let value = value.rem_euclid(std::f64::consts::TAU);
        // `rem_euclid` can round a tiny negative input up to exactly `TAU`.
        // Preserve the documented half-open range and one full-turn encoding.
        Ok(Self(if value == 0.0 || value >= std::f64::consts::TAU {
            0.0
        } else {
            value
        }))
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for HeadingRadians {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::try_new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct SimulationTick(u64);

impl SimulationTick {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn checked_add(self, ticks: u64) -> Option<Self> {
        match self.0.checked_add(ticks) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Explicit root for deterministic simulation random streams.
///
/// Substream derivation is defined by the backend contract in `E09-T03`;
/// this value never falls back to wall-clock time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RootSeed(u64);

impl RootSeed {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}
