//! Godot version identifiers (see `docs/specs/version-management.md`).
//!
//! A [`Version`] is a concrete release such as `4.7.1` or `4.8-beta2`; a
//! [`VersionSpec`] is what a user types (`4.7`, `latest`, `4.7.1-mono`) and is
//! resolved against a list of available versions. Godot 3.x is not supported.

use std::fmt;
use std::str::FromStr;

/// Errors from parsing a version or a version spec.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VersionError {
    #[error("empty version")]
    Empty,
    #[error("`{0}` is not a valid Godot version")]
    Malformed(String),
    #[error("Godot {0}.x is not supported, only 4.x and newer")]
    UnsupportedMajor(u32),
}

/// The release channel. Variants are ordered from least to most mature, so a
/// derived ordering puts `dev` before `beta` before `rc` before `stable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    Dev(u32),
    Beta(u32),
    Rc(u32),
    Stable,
}

impl Channel {
    fn parse(s: &str) -> Option<Self> {
        if s == "stable" {
            return Some(Self::Stable);
        }
        let numbered = |prefix: &str| s.strip_prefix(prefix)?.parse::<u32>().ok();
        numbered("dev")
            .map(Self::Dev)
            .or_else(|| numbered("beta").map(Self::Beta))
            .or_else(|| numbered("rc").map(Self::Rc))
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Dev(n) => write!(f, "dev{n}"),
            Self::Beta(n) => write!(f, "beta{n}"),
            Self::Rc(n) => write!(f, "rc{n}"),
            Self::Stable => f.write_str("stable"),
        }
    }
}

/// A concrete Godot release. Ordering is by version, then by channel, so
/// `4.8-beta2 < 4.8-rc1 < 4.8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub channel: Channel,
}

impl Version {
    /// Parses a GitHub release tag such as `4.7.1-stable`, `4.7-stable` or `4.8-beta2`.
    /// Godot omits the patch number when it is zero.
    pub fn from_tag(tag: &str) -> Result<Self, VersionError> {
        let malformed = || VersionError::Malformed(tag.to_string());
        let (numbers, channel) = tag.split_once('-').ok_or_else(malformed)?;
        let channel = Channel::parse(channel).ok_or_else(malformed)?;
        let nums = parse_numbers(numbers).ok_or_else(malformed)?;
        let major = nums[0];
        if major < 4 {
            return Err(VersionError::UnsupportedMajor(major));
        }
        Ok(Self {
            major,
            minor: nums[1],
            patch: nums.get(2).copied().unwrap_or(0),
            channel,
        })
    }

    /// The GitHub release tag, for example `4.7.1-stable`.
    pub fn tag(&self) -> String {
        format!("{}-{}", self.numbers(), self.channel)
    }

    fn numbers(&self) -> String {
        if self.patch == 0 {
            format!("{}.{}", self.major, self.minor)
        } else {
            format!("{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

/// Shown the way people write it: `4.7.1`, `4.7`, `4.8-beta2` (stable has no suffix).
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.numbers())?;
        if self.channel != Channel::Stable {
            write!(f, "-{}", self.channel)?;
        }
        Ok(())
    }
}

/// The standard build or the .NET ("mono") build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Flavor {
    #[default]
    Standard,
    Dotnet,
}

/// An installable build: a version and a flavor. Its [`Display`](fmt::Display)
/// form (`4.7.1`, `4.7.1-mono`) names its folder under `versions/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Build {
    pub version: Version,
    pub flavor: Flavor,
}

impl fmt::Display for Build {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.version)?;
        if self.flavor == Flavor::Dotnet {
            f.write_str("-mono")?;
        }
        Ok(())
    }
}

/// What a version spec points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The newest stable release.
    Latest,
    /// The newest stable release of one `MAJOR.MINOR` line.
    Line { major: u32, minor: u32 },
    /// One exact release, including pre-releases.
    Exact(Version),
}

/// A version as typed by a user: `latest`, `4.7`, `4.7.1`, `4.8-rc1`, each with an
/// optional `-mono` suffix. Pre-releases are only ever chosen when named exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionSpec {
    pub target: Target,
    pub flavor: Flavor,
}

impl VersionSpec {
    /// Picks the version this spec stands for from `available`, if any.
    pub fn pick<'a>(&self, available: impl IntoIterator<Item = &'a Version>) -> Option<Version> {
        let stable = |v: &&Version| v.channel == Channel::Stable;
        match self.target {
            Target::Latest => available.into_iter().filter(stable).max().copied(),
            Target::Line { major, minor } => available
                .into_iter()
                .filter(stable)
                .filter(|v| v.major == major && v.minor == minor)
                .max()
                .copied(),
            Target::Exact(wanted) => available.into_iter().find(|v| **v == wanted).copied(),
        }
    }
}

impl FromStr for VersionSpec {
    type Err = VersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        if text.is_empty() {
            return Err(VersionError::Empty);
        }
        let (text, flavor) = match text.strip_suffix("-mono") {
            Some(rest) => (rest, Flavor::Dotnet),
            None => (text, Flavor::Standard),
        };
        let malformed = || VersionError::Malformed(text.to_string());

        let target = if text == "latest" {
            Target::Latest
        } else if text.contains('-') {
            Target::Exact(Version::from_tag(text)?)
        } else {
            let nums = parse_numbers(text).ok_or_else(malformed)?;
            if nums[0] < 4 {
                return Err(VersionError::UnsupportedMajor(nums[0]));
            }
            match nums.get(2) {
                None => Target::Line {
                    major: nums[0],
                    minor: nums[1],
                },
                Some(&patch) => Target::Exact(Version {
                    major: nums[0],
                    minor: nums[1],
                    patch,
                    channel: Channel::Stable,
                }),
            }
        };
        Ok(Self { target, flavor })
    }
}

/// `MAJOR.MINOR` or `MAJOR.MINOR.PATCH` as numbers; `None` for anything else.
fn parse_numbers(s: &str) -> Option<Vec<u32>> {
    let nums: Option<Vec<u32>> = s.split('.').map(|p| p.parse().ok()).collect();
    nums.filter(|n| (2..=3).contains(&n.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(tag: &str) -> Version {
        Version::from_tag(tag).unwrap()
    }

    fn spec(text: &str) -> VersionSpec {
        text.parse().unwrap()
    }

    #[test]
    fn tags_round_trip() {
        for tag in [
            "4.7.1-stable",
            "4.7-stable",
            "4.8-beta2",
            "4.8-rc1",
            "4.8-dev7",
        ] {
            assert_eq!(v(tag).tag(), tag);
        }
    }

    #[test]
    fn missing_patch_means_zero() {
        assert_eq!(v("4.7-stable").patch, 0);
    }

    #[test]
    fn display_is_the_short_form() {
        assert_eq!(v("4.7.1-stable").to_string(), "4.7.1");
        assert_eq!(v("4.7-stable").to_string(), "4.7");
        assert_eq!(v("4.8-beta2").to_string(), "4.8-beta2");
    }

    #[test]
    fn channels_order_by_maturity() {
        assert!(v("4.8-dev7") < v("4.8-beta1"));
        assert!(v("4.8-beta2") < v("4.8-rc1"));
        assert!(v("4.8-rc1") < v("4.8-stable"));
        assert!(v("4.7.2-stable") < v("4.8-dev1"));
        assert!(v("4.7-stable") < v("4.7.1-stable"));
    }

    #[test]
    fn godot_3_is_rejected() {
        assert_eq!(
            Version::from_tag("3.6.3-stable"),
            Err(VersionError::UnsupportedMajor(3))
        );
        assert_eq!(
            "3.5".parse::<VersionSpec>(),
            Err(VersionError::UnsupportedMajor(3))
        );
    }

    #[test]
    fn bad_tags_are_rejected() {
        for tag in [
            "",
            "4.7",
            "4.7-nightly",
            "x.y-stable",
            "4-stable",
            "4.7.1.2-stable",
            "4.8-beta",
        ] {
            assert!(Version::from_tag(tag).is_err(), "{tag} should be rejected");
        }
    }

    #[test]
    fn build_names_the_folder() {
        let b = Build {
            version: v("4.7.1-stable"),
            flavor: Flavor::Dotnet,
        };
        assert_eq!(b.to_string(), "4.7.1-mono");
        let b = Build {
            version: v("4.8-beta2"),
            flavor: Flavor::Standard,
        };
        assert_eq!(b.to_string(), "4.8-beta2");
    }

    #[test]
    fn specs_parse() {
        assert_eq!(spec("latest").target, Target::Latest);
        assert_eq!(spec("4.7").target, Target::Line { major: 4, minor: 7 });
        assert_eq!(spec("4.7.1").target, Target::Exact(v("4.7.1-stable")));
        assert_eq!(spec("4.7.0").target, Target::Exact(v("4.7-stable")));
        assert_eq!(spec("4.8-rc1").target, Target::Exact(v("4.8-rc1")));
        assert_eq!(spec("4.7.1-mono").flavor, Flavor::Dotnet);
        assert_eq!(spec("latest-mono").flavor, Flavor::Dotnet);
        assert_eq!(
            spec(" 4.8-beta2-mono ").target,
            Target::Exact(v("4.8-beta2"))
        );
        assert_eq!("".parse::<VersionSpec>(), Err(VersionError::Empty));
        assert!("banana".parse::<VersionSpec>().is_err());
    }

    #[test]
    fn pick_chooses_the_newest_matching_stable() {
        let all = [
            v("4.6.3-stable"),
            v("4.7-stable"),
            v("4.7.1-stable"),
            v("4.7.2-stable"),
            v("4.8-beta2"),
        ];
        assert_eq!(spec("latest").pick(&all), Some(v("4.7.2-stable")));
        assert_eq!(spec("4.7").pick(&all), Some(v("4.7.2-stable")));
        assert_eq!(spec("4.6").pick(&all), Some(v("4.6.3-stable")));
        assert_eq!(spec("4.7.1").pick(&all), Some(v("4.7.1-stable")));
        assert_eq!(spec("4.5").pick(&all), None);
    }

    #[test]
    fn pre_releases_are_only_picked_by_exact_name() {
        let all = [v("4.7.2-stable"), v("4.8-beta2")];
        assert_eq!(spec("latest").pick(&all), Some(v("4.7.2-stable")));
        assert_eq!(spec("4.8").pick(&all), None);
        assert_eq!(spec("4.8-beta2").pick(&all), Some(v("4.8-beta2")));
        assert_eq!(spec("4.8-beta1").pick(&all), None);
    }
}
