//! Construction choices within TunedSurfaceProbed and the evidence for each attempt.
use serde::{Deserialize, Serialize};

pub const DEFAULT_CLOSURE_DEPTH: usize = 64;
pub const DEFAULT_COMPOUND_DEPTH: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConstructionStrategy {
    SurfaceEnum,
    Factored,
}

impl ConstructionStrategy {
    pub const ALL: [Self; 2] = [Self::SurfaceEnum, Self::Factored];

    pub const fn label(self) -> &'static str {
        match self {
            Self::SurfaceEnum => "surface-enum",
            Self::Factored => "factored",
        }
    }
}

impl std::str::FromStr for ConstructionStrategy {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "surface-enum" => Ok(Self::SurfaceEnum),
            "factored" => Ok(Self::Factored),
            _ => Err(format!("unknown FST construction strategy {value:?}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StrategySelection {
    SurfaceEnum,
    Factored,
    Auto,
}

impl std::str::FromStr for StrategySelection {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "surface-enum" => Ok(Self::SurfaceEnum),
            "factored" => Ok(Self::Factored),
            "auto" => Ok(Self::Auto),
            _ => Err(format!(
                "unknown FST strategy {value:?}; use surface-enum, factored or auto"
            )),
        }
    }
}

/// Containment limits, independent of the grammar's per-rule application bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralClosureLimits {
    pub work_limit: usize,
    pub retained_bytes_limit: usize,
}

impl Default for StructuralClosureLimits {
    fn default() -> Self {
        Self {
            work_limit: 100_000,
            retained_bytes_limit: 16 * 1024 * 1024,
        }
    }
}

impl StructuralClosureLimits {
    pub fn from_env() -> Result<Self, String> {
        Self::from_overrides(None, None)
    }

    /// Explicit per-build values take precedence over the existing environment settings.
    pub fn from_overrides(work: Option<usize>, bytes: Option<usize>) -> Result<Self, String> {
        Self::resolve(work, bytes, |name| std::env::var(name))
    }

    fn resolve(
        work: Option<usize>,
        bytes: Option<usize>,
        mut lookup: impl FnMut(&str) -> Result<String, std::env::VarError>,
    ) -> Result<Self, String> {
        let mut limit = |explicit: Option<usize>, name: &str, default: usize| {
            if let Some(value) = explicit {
                return Ok(value);
            }
            match lookup(name) {
                Ok(value) => value
                    .parse()
                    .map_err(|_| format!("{name} must be a nonnegative integer; got {value:?}")),
                Err(std::env::VarError::NotPresent) => Ok(default),
                Err(error) => Err(format!("cannot read {name}: {error}")),
            }
        };
        let defaults = Self::default();
        Ok(Self {
            work_limit: limit(
                work,
                "HC_STRUCTURAL_CLOSURE_WORK_LIMIT",
                defaults.work_limit,
            )?,
            retained_bytes_limit: limit(
                bytes,
                "HC_STRUCTURAL_CLOSURE_BYTES_LIMIT",
                defaults.retained_bytes_limit,
            )?,
        })
    }
}

/// Operational limits, independent of the grammar's authored language bounds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstructionBounds {
    /// Optional additional logical-work cap. None preserves the surface constructor
    /// without imposing the characterization probe's separate 3,000-step budget.
    pub closure_work: Option<usize>,
    /// None resolves HC_STRUCTURAL_CLOSURE_WORK_LIMIT (default 100,000 syntheses).
    #[serde(default)]
    pub structural_work: Option<usize>,
    /// None resolves HC_STRUCTURAL_CLOSURE_BYTES_LIMIT (default 16 MiB retained charge).
    #[serde(default)]
    pub structural_bytes: Option<usize>,
    pub closure_depth: usize,
    pub compound_depth: usize,
    /// None resolves the legacy HC_DERIV_CHAIN_DEPTH_BUDGET setting once per build.
    pub derivation_depth: Option<usize>,
}

impl Default for ConstructionBounds {
    fn default() -> Self {
        Self {
            closure_work: None,
            structural_work: None,
            structural_bytes: None,
            closure_depth: DEFAULT_CLOSURE_DEPTH,
            compound_depth: DEFAULT_COMPOUND_DEPTH,
            derivation_depth: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FstCompileConfig {
    pub strategy: StrategySelection,
    pub auto_order: Vec<ConstructionStrategy>,
    pub bounds: ConstructionBounds,
}

impl Default for FstCompileConfig {
    fn default() -> Self {
        Self {
            strategy: StrategySelection::SurfaceEnum,
            auto_order: ConstructionStrategy::ALL.to_vec(),
            bounds: ConstructionBounds::default(),
        }
    }
}

impl FstCompileConfig {
    pub fn for_strategy(strategy: ConstructionStrategy) -> Self {
        Self {
            strategy: match strategy {
                ConstructionStrategy::SurfaceEnum => StrategySelection::SurfaceEnum,
                ConstructionStrategy::Factored => StrategySelection::Factored,
            },
            ..Self::default()
        }
    }

    pub fn strategies(&self) -> Result<Vec<ConstructionStrategy>, String> {
        match self.strategy {
            StrategySelection::SurfaceEnum => Ok(vec![ConstructionStrategy::SurfaceEnum]),
            StrategySelection::Factored => Ok(vec![ConstructionStrategy::Factored]),
            StrategySelection::Auto => {
                if self.auto_order.is_empty() {
                    return Err("FST auto order must contain at least one strategy".to_string());
                }
                for (index, strategy) in self.auto_order.iter().enumerate() {
                    if self.auto_order[..index].contains(strategy) {
                        return Err(format!(
                            "duplicate strategy {} in FST auto order",
                            strategy.label()
                        ));
                    }
                }
                Ok(self.auto_order.clone())
            }
        }
    }

    /// Consume shared CLI options; returns false for an option belonging to the caller.
    pub fn parse_option(&mut self, args: &[String], index: &mut usize) -> Result<bool, String> {
        let arg = &args[*index];
        let (name, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(n, v)| (n, Some(v)));
        if !matches!(
            name,
            "--fst-strategy"
                | "--fst-auto-order"
                | "--fst-closure-work"
                | "--fst-structural-work"
                | "--fst-structural-bytes"
                | "--fst-closure-depth"
                | "--fst-derivation-depth"
                | "--fst-compound-depth"
        ) {
            return Ok(false);
        }
        let value = match inline {
            Some(value) => value,
            None => {
                *index += 1;
                args.get(*index)
                    .ok_or_else(|| format!("{name} requires a value"))?
            }
        };
        match name {
            "--fst-strategy" => self.strategy = value.parse()?,
            "--fst-auto-order" => {
                self.auto_order = value.split(',').map(str::parse).collect::<Result<_, _>>()?
            }
            _ => {
                let limit = value
                    .parse::<usize>()
                    .map_err(|_| format!("{name} requires a non-negative integer"))?;
                match name {
                    "--fst-closure-work" => self.bounds.closure_work = Some(limit),
                    "--fst-structural-work" => self.bounds.structural_work = Some(limit),
                    "--fst-structural-bytes" => self.bounds.structural_bytes = Some(limit),
                    "--fst-closure-depth" => self.bounds.closure_depth = limit,
                    "--fst-compound-depth" => self.bounds.compound_depth = limit,
                    "--fst-derivation-depth" => self.bounds.derivation_depth = Some(limit),
                    _ => unreachable!(),
                }
            }
        }
        *index += 1;
        Ok(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StrategyRefusal {
    NotImplemented,
    Capability,
    Incomplete,
    LimitExceeded,
    LexcCompileFailed,
    InvalidConfiguration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructionLimit {
    pub measure: String,
    pub value: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructionAttempt {
    pub strategy: ConstructionStrategy,
    pub elapsed_millis: u64,
    /// Process high-water RSS at attempt end, including earlier attempts; unavailable off Linux.
    pub process_peak_rss_kib: Option<u64>,
    pub states: Option<i64>,
    pub arcs: Option<i64>,
    pub refusal: Option<StrategyRefusal>,
    pub detail: Option<String>,
    pub closure: Option<crate::emit_report::CharacterizationResult>,
    pub bound_refusal: Option<ConstructionLimit>,
    pub compile_phase: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConstructionReport {
    pub requested_config: Option<FstCompileConfig>,
    pub config: Option<FstCompileConfig>,
    pub selected: Option<ConstructionStrategy>,
    pub attempts: Vec<ConstructionAttempt>,
    pub configuration_error: Option<String>,
}

pub fn process_peak_rss_kib() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        status.lines().find_map(|line| {
            line.strip_prefix("VmHWM:")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structural_limits_keep_defaults_and_resolve_each_override_once() {
        let defaults =
            StructuralClosureLimits::resolve(None, None, |_| Err(std::env::VarError::NotPresent))
                .unwrap();
        assert_eq!(defaults.work_limit, 100_000);
        assert_eq!(defaults.retained_bytes_limit, 16 * 1024 * 1024);
        let mut names = Vec::new();
        let limits = StructuralClosureLimits::resolve(None, None, |name| {
            names.push(name.to_string());
            Ok(if name == "HC_STRUCTURAL_CLOSURE_WORK_LIMIT" {
                "7"
            } else {
                "11"
            }
            .to_string())
        })
        .unwrap();
        assert_eq!((limits.work_limit, limits.retained_bytes_limit), (7, 11));
        assert_eq!(
            names,
            [
                "HC_STRUCTURAL_CLOSURE_WORK_LIMIT",
                "HC_STRUCTURAL_CLOSURE_BYTES_LIMIT"
            ]
        );
        let explicit = StructuralClosureLimits::resolve(Some(0), Some(19), |_| {
            panic!("explicit values must not read environment")
        })
        .unwrap();
        assert_eq!(
            (explicit.work_limit, explicit.retained_bytes_limit),
            (0, 19)
        );
        let mixed = StructuralClosureLimits::resolve(Some(23), None, |name| {
            assert_eq!(name, "HC_STRUCTURAL_CLOSURE_BYTES_LIMIT");
            Ok("29".to_string())
        })
        .unwrap();
        assert_eq!((mixed.work_limit, mixed.retained_bytes_limit), (23, 29));
    }

    #[test]
    fn malformed_structural_environment_is_refused_and_names_its_control() {
        for control in [
            "HC_STRUCTURAL_CLOSURE_WORK_LIMIT",
            "HC_STRUCTURAL_CLOSURE_BYTES_LIMIT",
        ] {
            let error = StructuralClosureLimits::resolve(None, None, |name| {
                if name == control {
                    Ok("bad".to_string())
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            })
            .unwrap_err();
            assert!(error.contains(control));
        }
    }
}
