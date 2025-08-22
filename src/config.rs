use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::{error::Result, utils::Unit};

/// Rate limit configuration for a domain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub domain: String,
    pub descriptors: Vec<RateLimitDescriptor>,
}

/// A rate limit descriptor that can match requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitDescriptor {
    pub key: String,
    pub value: Option<String>,
    pub rate_limit: Option<RateLimit>,
    pub shadow_mode: Option<bool>,
    pub descriptors: Option<Vec<RateLimitDescriptor>>,
}

/// Rate limit specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub requests_per_unit: u32,
    pub unit: RateLimitUnit,
    pub unlimited: Option<bool>,
    pub name: Option<String>,
}

/// Time units for rate limits
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitUnit {
    Second,
    Minute,
    Hour,
    Day,
}

impl From<RateLimitUnit> for Unit {
    fn from(unit: RateLimitUnit) -> Self {
        match unit {
            RateLimitUnit::Second => Unit::Second,
            RateLimitUnit::Minute => Unit::Minute,
            RateLimitUnit::Hour => Unit::Hour,
            RateLimitUnit::Day => Unit::Day,
        }
    }
}

impl From<Unit> for RateLimitUnit {
    fn from(unit: Unit) -> Self {
        match unit {
            Unit::Second => RateLimitUnit::Second,
            Unit::Minute => RateLimitUnit::Minute,
            Unit::Hour => RateLimitUnit::Hour,
            Unit::Day => RateLimitUnit::Day,
        }
    }
}

/// Compiled rate limit configuration for fast lookups
#[derive(Debug)]
pub struct CompiledRateLimitConfig {
    domain: String,
    // Map from descriptor path to rate limit
    limits: HashMap<String, CompiledRateLimit>,
}

#[derive(Debug, Clone)]
pub struct CompiledRateLimit {
    pub requests_per_unit: u32,
    pub unit: Unit,
    pub unlimited: bool,
    pub shadow_mode: bool,
    pub name: Option<String>,
}

impl CompiledRateLimitConfig {
    /// Compile a configuration for efficient runtime lookups
    pub fn compile(config: RateLimitConfig) -> Result<Self> {
        let mut limits = HashMap::new();
        
        for descriptor in &config.descriptors {
            Self::compile_descriptor(descriptor, &mut vec![], &mut limits)?;
        }

        Ok(Self {
            domain: config.domain,
            limits,
        })
    }

    fn compile_descriptor(
        descriptor: &RateLimitDescriptor,
        path: &mut Vec<String>,
        limits: &mut HashMap<String, CompiledRateLimit>,
    ) -> Result<()> {
        // Add current descriptor to path
        let key_value = if let Some(value) = &descriptor.value {
            format!("{}_{}", descriptor.key, value)
        } else {
            descriptor.key.clone()
        };
        path.push(key_value);

        // If this descriptor has a rate limit, store it
        if let Some(rate_limit) = &descriptor.rate_limit {
            let path_key = path.join(":");
            limits.insert(
                path_key,
                CompiledRateLimit {
                    requests_per_unit: rate_limit.requests_per_unit,
                    unit: rate_limit.unit.clone().into(),
                    unlimited: rate_limit.unlimited.unwrap_or(false),
                    shadow_mode: descriptor.shadow_mode.unwrap_or(false),
                    name: rate_limit.name.clone(),
                },
            );
        }

        // Recursively compile nested descriptors
        if let Some(nested_descriptors) = &descriptor.descriptors {
            for nested in nested_descriptors {
                Self::compile_descriptor(nested, path, limits)?;
            }
        }

        path.pop();
        Ok(())
    }

    /// Get the domain for this configuration
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Find a rate limit for the given descriptor path
    pub fn find_limit(&self, descriptors: &[(&str, &str)]) -> Option<&CompiledRateLimit> {
        // Try different combinations, from most specific to least specific
        for i in (1..=descriptors.len()).rev() {
            let path_parts: Vec<String> = descriptors[..i]
                .iter()
                .map(|(key, value)| {
                    if value.is_empty() {
                        key.to_string()
                    } else {
                        format!("{}_{}", key, value)
                    }
                })
                .collect();
            
            let path = path_parts.join(":");
            if let Some(limit) = self.limits.get(&path) {
                return Some(limit);
            }
        }
        
        None
    }
}

/// Load configuration from YAML string
pub fn load_config_from_yaml(yaml: &str) -> Result<RateLimitConfig> {
    serde_yaml::from_str(yaml).map_err(|e| {
        crate::error::RateLimitError::Config(format!("Failed to parse YAML: {}", e))
    })
}

/// Load configuration from YAML file
pub fn load_config_from_file(path: &str) -> Result<RateLimitConfig> {
    let content = std::fs::read_to_string(path)?;
    load_config_from_yaml(&content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config_from_yaml() {
        let yaml = r#"
domain: test
descriptors:
  - key: database
    value: users
    rate_limit:
      requests_per_unit: 100
      unit: second
  - key: database
    rate_limit:
      requests_per_unit: 1000
      unit: minute
"#;

        let config = load_config_from_yaml(yaml).unwrap();
        assert_eq!(config.domain, "test");
        assert_eq!(config.descriptors.len(), 2);
    }

    #[test]
    fn test_compile_config() {
        let config = RateLimitConfig {
            domain: "test".to_string(),
            descriptors: vec![
                RateLimitDescriptor {
                    key: "database".to_string(),
                    value: Some("users".to_string()),
                    rate_limit: Some(RateLimit {
                        requests_per_unit: 100,
                        unit: RateLimitUnit::Second,
                        unlimited: None,
                        name: None,
                    }),
                    shadow_mode: None,
                    descriptors: None,
                },
            ],
        };

        let compiled = CompiledRateLimitConfig::compile(config).unwrap();
        let limit = compiled.find_limit(&[("database", "users")]);
        assert!(limit.is_some());
        assert_eq!(limit.unwrap().requests_per_unit, 100);
    }
}