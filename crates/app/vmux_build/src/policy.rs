use crate::Result;

pub const GIB: u64 = 1024 * 1024 * 1024;

pub struct Policy {
    pub minimum: u64,
    pub warning: u64,
    pub cache_limit: u64,
    pub local_keep: usize,
}

impl Policy {
    pub fn check(&self, free: u64) -> Result<()> {
        if free < self.minimum {
            return Err(format!("Insufficient build space: need {} GiB free; stop unused apps/builds and run make clean-build-cache with VMUX_BUILD_STORAGE_APPLY=1", self.minimum as f64 / GIB as f64).into());
        }
        Ok(())
    }

    pub fn environment() -> Result<Self> {
        let minimum = std::env::var("VMUX_BUILD_STORAGE_MIN_GIB").unwrap_or_else(|_| "20".into());
        let warning = std::env::var("VMUX_BUILD_STORAGE_WARN_GIB").unwrap_or_else(|_| "50".into());
        let cache = std::env::var("VMUX_BUILD_STORAGE_CACHE_GIB").unwrap_or_else(|_| "50".into());
        let keep = std::env::var("VMUX_BUILD_STORAGE_LOCAL_KEEP").unwrap_or_else(|_| "3".into());
        Self::parse(&minimum, &warning, &cache, &keep)
    }

    fn parse(minimum: &str, warning: &str, cache: &str, keep: &str) -> Result<Self> {
        let minimum = Self::bytes(minimum)?;
        let warning = Self::bytes(warning)?;
        let cache_limit = Self::bytes(cache)?;
        let local_keep = keep.parse()?;
        if local_keep < 1 || warning < minimum {
            return Err("Keep at least one local app and set warning >= minimum".into());
        }
        Ok(Self {
            minimum,
            warning,
            cache_limit,
            local_keep,
        })
    }

    fn bytes(value: &str) -> Result<u64> {
        let value: f64 = value.parse()?;
        let bytes = value * GIB as f64;
        if !value.is_finite() || value < 0.0 || bytes >= u64::MAX as f64 {
            return Err("Storage thresholds must be finite nonnegative numbers".into());
        }
        Ok(bytes as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::Policy;

    #[test]
    fn rejects_invalid_storage_policies() {
        for values in [
            ["nan", "50", "50", "3"],
            ["20", "50", "-1", "3"],
            ["60", "50", "50", "3"],
            ["20", "50", "50", "0"],
            ["inf", "inf", "50", "3"],
        ] {
            assert!(Policy::parse(values[0], values[1], values[2], values[3]).is_err());
        }
    }

    #[test]
    fn warning_threshold_does_not_refuse_a_build() {
        let policy = Policy::parse("20", "50", "50", "3").unwrap();
        assert!(policy.check(10 * super::GIB).is_err());
        assert!(policy.check(30 * super::GIB).is_ok());
    }
}
