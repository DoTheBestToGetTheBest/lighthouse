use crate::{ChainSpec, Epoch};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use std::convert::TryFrom;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, Decode, Encode, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String")]
#[serde(into = "String")]
#[ssz(enum_behaviour = "tag")]
pub enum ForkName {
    Base,
    Altair,
    Merge,
    Capella,
    Deneb,
}

impl ForkName {
    pub fn list_all() -> Vec<ForkName> {
        vec![
            ForkName::Base,
            ForkName::Altair,
            ForkName::Merge,
            ForkName::Capella,
            ForkName::Deneb,
        ]
    }

    pub fn latest() -> ForkName {
        // This unwrap is safe as long as we have 1+ forks. It is tested below.
        *ForkName::list_all().last().unwrap()
    }

    /// Set the activation slots in the given `ChainSpec` so that the fork named by `self`
    /// is the only fork in effect from genesis.
    pub fn make_genesis_spec(&self, mut spec: ChainSpec) -> ChainSpec {
        // Assumes GENESIS_EPOCH = 0, which is safe because it's a constant.
        match self {
            ForkName::Base => {
                spec.altair_fork_epoch = None;
                spec.bellatrix_fork_epoch = None;
                spec.capella_fork_epoch = None;
                spec.deneb_fork_epoch = None;
                spec
            }
            ForkName::Altair => {
                spec.altair_fork_epoch = Some(Epoch::new(0));
                spec.bellatrix_fork_epoch = None;
                spec.capella_fork_epoch = None;
                spec.deneb_fork_epoch = None;
                spec
            }
            ForkName::Merge => {
                spec.altair_fork_epoch = Some(Epoch::new(0));
                spec.bellatrix_fork_epoch = Some(Epoch::new(0));
                spec.capella_fork_epoch = None;
                spec.deneb_fork_epoch = None;
                spec
            }
            ForkName::Capella => {
                spec.altair_fork_epoch = Some(Epoch::new(0));
                spec.bellatrix_fork_epoch = Some(Epoch::new(0));
                spec.capella_fork_epoch = Some(Epoch::new(0));
                spec.deneb_fork_epoch = None;
                spec
            }
            ForkName::Deneb => {
                spec.altair_fork_epoch = Some(Epoch::new(0));
                spec.bellatrix_fork_epoch = Some(Epoch::new(0));
                spec.capella_fork_epoch = Some(Epoch::new(0));
                spec.deneb_fork_epoch = Some(Epoch::new(0));
                spec
            }
        }
    }

    /// Return the name of the fork immediately prior to the current one.
    ///
    /// If `self` is `ForkName::Base` then `Base` is returned.
    pub fn previous_fork(self) -> Option<ForkName> {
        match self {
            ForkName::Base => None,
            ForkName::Altair => Some(ForkName::Base),
            ForkName::Merge => Some(ForkName::Altair),
            ForkName::Capella => Some(ForkName::Merge),
            ForkName::Deneb => Some(ForkName::Capella),
        }
    }

    /// Return the name of the fork immediately after the current one.
    ///
    /// If `self` is the last known fork and has no successor, `None` is returned.
    pub fn next_fork(self) -> Option<ForkName> {
        match self {
            ForkName::Base => Some(ForkName::Altair),
            ForkName::Altair => Some(ForkName::Merge),
            ForkName::Merge => Some(ForkName::Capella),
            ForkName::Capella => Some(ForkName::Deneb),
            ForkName::Deneb => None,
        }
    }
}

/// Map a fork name into a fork-versioned superstruct type like `BeaconBlock`.
///
/// The `$body` expression is where the magic happens. The macro allows us to achieve polymorphism
/// in the return type, which is not usually possible in Rust without trait objects.
///
/// E.g. you could call `map_fork_name!(fork, BeaconBlock, serde_json::from_str(s))` to decode
/// different `BeaconBlock` variants depending on the value of `fork`. Note how the type of the body
/// will change between `BeaconBlockBase` and `BeaconBlockAltair` depending on which branch is
/// taken, the important thing is that they are re-unified by injecting them back into the
/// `BeaconBlock` parent enum.
///
/// If you would also like to extract additional data alongside the superstruct type, use
/// the more flexible `map_fork_name_with` macro.
#[macro_export]
macro_rules! map_fork_name {
    ($fork_name:expr, $t:tt, $body:expr) => {
        map_fork_name_with!($fork_name, $t, { ($body, ()) }).0
    };
}

/// Map a fork name into a tuple of `(t, extra)` where `t` is a superstruct type.
#[macro_export]
macro_rules! map_fork_name_with {
    ($fork_name:expr, $t:tt, $body:block) => {
        match $fork_name {
            ForkName::Base => {
                let (value, extra_data) = $body;
                ($t::Base(value), extra_data)
            }
            ForkName::Altair => {
                let (value, extra_data) = $body;
                ($t::Altair(value), extra_data)
            }
            ForkName::Merge => {
                let (value, extra_data) = $body;
                ($t::Merge(value), extra_data)
            }
            ForkName::Capella => {
                let (value, extra_data) = $body;
                ($t::Capella(value), extra_data)
            }
            ForkName::Deneb => {
                let (value, extra_data) = $body;
                ($t::Deneb(value), extra_data)
            }
        }
    };
}

impl FromStr for ForkName {
    type Err = String;

    fn from_str(fork_name: &str) -> Result<Self, String> {
        Ok(match fork_name.to_lowercase().as_ref() {
            "phase0" | "base" => ForkName::Base,
            "altair" => ForkName::Altair,
            "bellatrix" | "merge" => ForkName::Merge,
            "capella" => ForkName::Capella,
            "deneb" => ForkName::Deneb,
            _ => return Err(format!("unknown fork name: {}", fork_name)),
        })
    }
}

impl Display for ForkName {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        match self {
            ForkName::Base => "phase0".fmt(f),
            ForkName::Altair => "altair".fmt(f),
            ForkName::Merge => "bellatrix".fmt(f),
            ForkName::Capella => "capella".fmt(f),
            ForkName::Deneb => "deneb".fmt(f),
        }
    }
}

impl From<ForkName> for String {
    fn from(fork: ForkName) -> String {
        fork.to_string()
    }
}

impl TryFrom<String> for ForkName {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::from_str(&s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InconsistentFork {
    pub fork_at_slot: ForkName,
    pub object_fork: ForkName,
}

#[cfg(test)]
mod test {
    use super::*;
    use itertools::Itertools;

    #[test]
    fn previous_and_next_fork_consistent() {
        assert_eq!(ForkName::latest().next_fork(), None);
        assert_eq!(ForkName::Base.previous_fork(), None);

        for (prev_fork, fork) in ForkName::list_all().into_iter().tuple_windows() {
            assert_eq!(prev_fork.next_fork(), Some(fork));
            assert_eq!(fork.previous_fork(), Some(prev_fork));
        }
    }

    #[test]
    fn fork_name_case_insensitive_match() {
        assert_eq!(ForkName::from_str("BASE"), Ok(ForkName::Base));
        assert_eq!(ForkName::from_str("BaSe"), Ok(ForkName::Base));
        assert_eq!(ForkName::from_str("base"), Ok(ForkName::Base));

        assert_eq!(ForkName::from_str("PHASE0"), Ok(ForkName::Base));
        assert_eq!(ForkName::from_str("PhAsE0"), Ok(ForkName::Base));
        assert_eq!(ForkName::from_str("phase0"), Ok(ForkName::Base));

        assert_eq!(ForkName::from_str("ALTAIR"), Ok(ForkName::Altair));
        assert_eq!(ForkName::from_str("AlTaIr"), Ok(ForkName::Altair));
        assert_eq!(ForkName::from_str("altair"), Ok(ForkName::Altair));

        assert!(ForkName::from_str("NO_NAME").is_err());
        assert!(ForkName::from_str("no_name").is_err());
    }

    #[test]
    fn fork_name_bellatrix_or_merge() {
        assert_eq!(ForkName::from_str("bellatrix"), Ok(ForkName::Merge));
        assert_eq!(ForkName::from_str("merge"), Ok(ForkName::Merge));
        assert_eq!(ForkName::Merge.to_string(), "bellatrix");
    }

    #[test]
    fn fork_name_latest() {
        assert_eq!(ForkName::latest(), *ForkName::list_all().last().unwrap());

        let mut fork = ForkName::Base;
        while let Some(next_fork) = fork.next_fork() {
            fork = next_fork;
        }
        assert_eq!(ForkName::latest(), fork);
    }
}
