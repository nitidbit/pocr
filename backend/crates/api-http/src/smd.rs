//! Standardized Mean Difference (SMD) calculator for demographic balance assessment.
//!
//! Computes SMD for continuous and categorical variables across treatment arms.
//! Used by the balance endpoint to flag demographic imbalance.

/// SMD threshold above which a variable is considered imbalanced.
pub const SMD_IMBALANCE_THRESHOLD: f64 = 0.1;

/// Per-arm statistics for a single variable.
#[derive(Debug, Clone)]
pub struct ArmStats {
    /// Mean (continuous) or proportion (categorical).
    pub value: f64,
    /// Standard deviation (continuous) or unused (categorical).
    pub sd: f64,
}

/// Compute the absolute SMD for a continuous variable between two arms.
///
/// Formula: `|mean1 - mean2| / sqrt((sd1^2 + sd2^2) / 2)`
///
/// Returns 0.0 when the pooled standard deviation is zero.
pub fn smd_continuous(mean1: f64, sd1: f64, mean2: f64, sd2: f64) -> f64 {
    let pooled_variance = (sd1 * sd1 + sd2 * sd2) / 2.0;
    if pooled_variance == 0.0 {
        return 0.0;
    }
    (mean1 - mean2).abs() / pooled_variance.sqrt()
}

/// Compute the absolute SMD for a binary categorical variable between two arms.
///
/// Formula: `|p1 - p2| / sqrt((p1*(1-p1) + p2*(1-p2)) / 2)`
///
/// Returns 0.0 when the pooled variance is zero (both proportions are 0 or 1).
pub fn smd_categorical(p1: f64, p2: f64) -> f64 {
    let pooled_variance = (p1 * (1.0 - p1) + p2 * (1.0 - p2)) / 2.0;
    if pooled_variance == 0.0 {
        return 0.0;
    }
    (p1 - p2).abs() / pooled_variance.sqrt()
}

/// Compute the maximum pairwise SMD across all arm pairs.
///
/// For multi-arm studies, the largest pairwise SMD indicates the worst imbalance.
/// Returns 0.0 if fewer than two arms are provided.
pub fn smd_max_pairwise(arms: &[ArmStats], is_continuous: bool) -> f64 {
    if arms.len() < 2 {
        return 0.0;
    }

    let mut max_smd: f64 = 0.0;
    for i in 0..arms.len() {
        for j in (i + 1)..arms.len() {
            let smd = if is_continuous {
                smd_continuous(arms[i].value, arms[i].sd, arms[j].value, arms[j].sd)
            } else {
                smd_categorical(arms[i].value, arms[j].value)
            };
            max_smd = max_smd.max(smd);
        }
    }
    max_smd
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-10;

    #[test]
    fn continuous_identical_arms_returns_zero() {
        let result = smd_continuous(50.0, 10.0, 50.0, 10.0);

        assert_eq!(result, 0.0);
    }

    #[test]
    fn continuous_known_value() {
        // mean1=60, sd=10, mean2=50, sd=10 -> |10| / sqrt((100+100)/2) = 10/10 = 1.0
        let result = smd_continuous(60.0, 10.0, 50.0, 10.0);

        assert!((result - 1.0).abs() < EPSILON);
    }

    #[test]
    fn continuous_different_sds() {
        // mean1=55, sd1=8, mean2=50, sd2=12
        // pooled_sd = sqrt((64 + 144) / 2) = sqrt(104) ~ 10.198
        // SMD = 5 / sqrt(104)
        let result = smd_continuous(55.0, 8.0, 50.0, 12.0);
        let expected = 5.0 / (104.0_f64).sqrt();

        assert!((result - expected).abs() < EPSILON);
    }

    #[test]
    fn continuous_zero_sd_both_arms_returns_zero() {
        let result = smd_continuous(50.0, 0.0, 60.0, 0.0);

        assert_eq!(result, 0.0);
    }

    #[test]
    fn categorical_identical_proportions_returns_zero() {
        let result = smd_categorical(0.5, 0.5);

        assert_eq!(result, 0.0);
    }

    #[test]
    fn categorical_known_value() {
        // p1=0.6, p2=0.4
        // pooled_var = (0.6*0.4 + 0.4*0.6) / 2 = (0.24 + 0.24) / 2 = 0.24
        // SMD = 0.2 / sqrt(0.24)
        let result = smd_categorical(0.6, 0.4);
        let expected = 0.2 / (0.24_f64).sqrt();

        assert!((result - expected).abs() < EPSILON);
    }

    #[test]
    fn categorical_extreme_proportions_returns_zero() {
        // p1=0.0, p2=1.0 -> variance for each is 0, pooled is 0
        let result = smd_categorical(0.0, 1.0);

        assert_eq!(result, 0.0);
    }

    #[test]
    fn max_pairwise_fewer_than_two_arms_returns_zero() {
        let arms = vec![ArmStats {
            value: 50.0,
            sd: 10.0,
        }];
        let result = smd_max_pairwise(&arms, true);

        assert_eq!(result, 0.0);
    }

    #[test]
    fn max_pairwise_two_arms_continuous() {
        let arms = vec![
            ArmStats {
                value: 60.0,
                sd: 10.0,
            },
            ArmStats {
                value: 50.0,
                sd: 10.0,
            },
        ];
        let result = smd_max_pairwise(&arms, true);

        assert!((result - 1.0).abs() < EPSILON);
    }

    #[test]
    fn max_pairwise_three_arms_returns_largest() {
        let arms = vec![
            ArmStats {
                value: 50.0,
                sd: 10.0,
            },
            ArmStats {
                value: 55.0,
                sd: 10.0,
            },
            ArmStats {
                value: 70.0,
                sd: 10.0,
            },
        ];
        // Pairs: (50,55) -> 0.5, (50,70) -> 2.0, (55,70) -> 1.5
        // Max = 2.0
        let result = smd_max_pairwise(&arms, true);

        assert!((result - 2.0).abs() < EPSILON);
    }

    #[test]
    fn max_pairwise_categorical() {
        let arms = vec![
            ArmStats {
                value: 0.5,
                sd: 0.0,
            },
            ArmStats {
                value: 0.7,
                sd: 0.0,
            },
        ];
        // |0.5 - 0.7| / sqrt((0.5*0.5 + 0.7*0.3) / 2) = 0.2 / sqrt((0.25 + 0.21) / 2)
        // = 0.2 / sqrt(0.23)
        let result = smd_max_pairwise(&arms, false);
        let expected = 0.2 / (0.23_f64).sqrt();

        assert!((result - expected).abs() < EPSILON);
    }
}
