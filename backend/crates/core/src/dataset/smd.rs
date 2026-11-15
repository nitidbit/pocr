//! Standardized mean difference (SMD) of one variable across arms.
//!
//! Every function returns `None` rather than a value when the SMD is
//! undefined, so a degenerate comparison never reads as perfect balance.

use super::summary::{Moments, count_as_f64};

/// SMD above which a variable is flagged as imbalanced.
///
/// The conventional 0.1 cutoff; the datasets README documents it as part of
/// the report contract.
pub(super) const IMBALANCE_THRESHOLD: f64 = 0.1;

/// Absolute SMD of a continuous variable between two arms:
/// `|m1 - m2| / sqrt((s1^2 + s2^2) / 2)`; `None` when the pooled variance is 0.
fn continuous_pair(m1: f64, s1: f64, m2: f64, s2: f64) -> Option<f64> {
    let pooled_variance = f64::midpoint(s1 * s1, s2 * s2);
    (pooled_variance != 0.0).then(|| (m1 - m2).abs() / pooled_variance.sqrt())
}

/// Absolute SMD of one category's proportion between two arms:
/// `|p1 - p2| / sqrt((p1(1-p1) + p2(1-p2)) / 2)`; `None` when the pooled
/// variance is 0.
fn proportion_pair(p1: f64, p2: f64) -> Option<f64> {
    let pooled_variance = f64::midpoint(p1 * (1.0 - p1), p2 * (1.0 - p2));
    (pooled_variance != 0.0).then(|| (p1 - p2).abs() / pooled_variance.sqrt())
}

/// Largest value `pair` yields over every unordered pair of `items`.
fn max_defined<T>(items: &[T], pair: impl Fn(&T, &T) -> Option<f64>) -> Option<f64> {
    items
        .iter()
        .enumerate()
        .flat_map(|(i, a)| items[i + 1..].iter().map(move |b| (a, b)))
        .filter_map(|(a, b)| pair(a, b))
        .max_by(f64::total_cmp)
}

/// SMD of a continuous variable across arms.
///
/// Arms without values are excluded. `None` when fewer than two arms remain,
/// when any remaining arm has no standard deviation, or when no pair is defined.
pub(super) fn continuous(arms: &[Moments]) -> Option<f64> {
    let stats: Vec<(f64, f64)> = arms
        .iter()
        .filter(|arm| arm.count > 0)
        .map(|arm| arm.mean.zip(arm.standard_deviation))
        .collect::<Option<_>>()?;
    max_defined(&stats, |&(m1, s1), &(m2, s2)| {
        continuous_pair(m1, s1, m2, s2)
    })
}

/// SMD of a categorical variable across arms.
///
/// `arm_rows` holds the rows per arm and `categories` the count per arm of
/// each category, aligned with `arm_rows`. Arms without rows are excluded.
/// `None` when fewer than two arms remain or no pair is defined.
pub(super) fn categorical(arm_rows: &[usize], categories: &[Vec<usize>]) -> Option<f64> {
    let kept: Vec<usize> = (0..arm_rows.len())
        .filter(|&arm| arm_rows[arm] > 0)
        .collect();
    categories
        .iter()
        .filter_map(|counts| {
            let proportions: Vec<f64> = kept
                .iter()
                .map(|&arm| count_as_f64(counts[arm]) / count_as_f64(arm_rows[arm]))
                .collect();
            max_defined(&proportions, |&p1, &p2| proportion_pair(p1, p2))
        })
        .max_by(f64::total_cmp)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-10;

    fn moments(mean: f64, standard_deviation: Option<f64>, count: usize) -> Moments {
        Moments {
            mean: Some(mean),
            standard_deviation,
            count,
        }
    }

    fn empty_arm() -> Moments {
        Moments {
            mean: None,
            standard_deviation: None,
            count: 0,
        }
    }

    #[test]
    fn continuous_pair_of_identical_arms_is_zero() {
        assert_eq!(continuous_pair(50.0, 10.0, 50.0, 10.0), Some(0.0));
    }

    #[test]
    fn continuous_pair_known_value() {
        // |60 - 50| / sqrt((100 + 100) / 2) = 1
        let result = continuous_pair(60.0, 10.0, 50.0, 10.0).expect("defined");

        assert!((result - 1.0).abs() < EPSILON);
    }

    #[test]
    fn continuous_pair_pools_different_sds() {
        let result = continuous_pair(55.0, 8.0, 50.0, 12.0).expect("defined");

        assert!((result - 5.0 / 104.0_f64.sqrt()).abs() < EPSILON);
    }

    #[test]
    fn continuous_pair_is_undefined_when_both_sds_are_zero() {
        assert_eq!(continuous_pair(50.0, 0.0, 60.0, 0.0), None);
    }

    #[test]
    fn proportion_pair_of_identical_proportions_is_zero() {
        assert_eq!(proportion_pair(0.5, 0.5), Some(0.0));
    }

    #[test]
    fn proportion_pair_known_value() {
        // 0.2 / sqrt((0.24 + 0.24) / 2)
        let result = proportion_pair(0.6, 0.4).expect("defined");

        assert!((result - 0.2 / 0.24_f64.sqrt()).abs() < EPSILON);
    }

    #[test]
    fn proportion_pair_is_undefined_for_extreme_proportions() {
        assert_eq!(proportion_pair(0.0, 1.0), None);
    }

    #[test]
    fn continuous_is_undefined_for_one_arm() {
        assert_eq!(continuous(&[moments(50.0, Some(10.0), 9)]), None);
    }

    #[test]
    fn continuous_compares_two_arms() {
        let arms = [moments(60.0, Some(10.0), 9), moments(50.0, Some(10.0), 9)];

        assert!((continuous(&arms).expect("defined") - 1.0).abs() < EPSILON);
    }

    #[test]
    fn continuous_takes_the_largest_of_three_arms() {
        let arms = [
            moments(50.0, Some(10.0), 9),
            moments(55.0, Some(10.0), 9),
            moments(70.0, Some(10.0), 9),
        ];

        assert!((continuous(&arms).expect("defined") - 2.0).abs() < EPSILON);
    }

    #[test]
    fn continuous_excludes_arm_without_values() {
        let arms = [
            moments(60.0, Some(10.0), 9),
            empty_arm(),
            moments(50.0, Some(10.0), 9),
        ];

        assert!((continuous(&arms).expect("defined") - 1.0).abs() < EPSILON);
    }

    #[test]
    fn continuous_is_undefined_when_only_one_arm_has_values() {
        let arms = [moments(60.0, Some(10.0), 9), empty_arm()];

        assert_eq!(continuous(&arms), None);
    }

    #[test]
    fn continuous_is_undefined_when_an_arm_has_no_sd() {
        let arms = [
            moments(60.0, Some(10.0), 9),
            moments(50.0, None, 1),
            moments(40.0, Some(10.0), 9),
        ];

        assert_eq!(continuous(&arms), None);
    }

    #[test]
    fn continuous_is_undefined_when_every_pair_has_zero_variance() {
        let arms = [moments(60.0, Some(0.0), 9), moments(50.0, Some(0.0), 9)];

        assert_eq!(continuous(&arms), None);
    }

    #[test]
    fn categorical_takes_the_largest_category() {
        // Category 0: 0.5 vs 0.7; category 1: 0.5 vs 0.3.
        let result = categorical(&[10, 10], &[vec![5, 7], vec![5, 3]]).expect("defined");

        assert!((result - 0.2 / 0.23_f64.sqrt()).abs() < EPSILON);
    }

    #[test]
    fn categorical_excludes_arm_without_rows() {
        let with_empty = categorical(&[10, 0, 10], &[vec![5, 0, 7], vec![5, 0, 3]]);

        assert_eq!(
            with_empty,
            categorical(&[10, 10], &[vec![5, 7], vec![5, 3]])
        );
    }

    #[test]
    fn categorical_is_undefined_for_one_arm_with_rows() {
        assert_eq!(categorical(&[10, 0], &[vec![10, 0]]), None);
    }

    #[test]
    fn categorical_is_undefined_when_every_category_has_zero_variance() {
        assert_eq!(categorical(&[10, 10], &[vec![10, 10]]), None);
    }

    #[test]
    fn categorical_takes_the_largest_of_three_arms() {
        let result = categorical(&[10, 10, 10], &[vec![5, 6, 9]]).expect("defined");
        let expected = proportion_pair(0.5, 0.9).expect("defined");

        assert!((result - expected).abs() < EPSILON);
    }
}
