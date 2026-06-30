use crate::error::{RandomizationError, RandomizationResult};
use crate::random::Arm;
use crate::random::Strata;
use rand::seq::SliceRandom;
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A permuted block of randomized arm assignments.
///
/// Generate a shuffled sequence of arm indices according to the configured
/// ratios and block size. Assignments are dispensed one at a time via
/// [`get_next`](Self::get_next) until the block is exhausted.
#[derive(Debug, Serialize, Deserialize)]
pub struct RandomPermutedBlock {
    /// Unique identifier for this block instance.
    pub id: Uuid,
    /// Study this block belongs to.
    pub study_id: Uuid,
    /// Serialized stratification key, if the block is stratum-specific.
    pub stratification: Option<String>,
    /// Position of the next unissued assignment within `values`.
    index: usize,
    /// Shuffled arm-index sequence. Each entry is an index into the study's arm list.
    values: Vec<u32>,
}

impl RandomPermutedBlock {
    /// Create a new permuted block for the given study arms.
    ///
    /// # Errors
    ///
    /// Return [`RandomizationError::InvalidBlockSize`] if `block_size` is
    /// provided and is not a positive multiple of the sum of arm ratios.
    pub fn new(
        study_id: &Uuid,
        arms: &[Arm],
        block_size: Option<u32>,
        strata: Option<&Strata>,
        rng: &mut impl Rng,
    ) -> RandomizationResult<Self> {
        Self::try_generate(study_id, arms, block_size, strata, rng)
    }

    /// Return the next arm index from this block and advance the cursor.
    ///
    /// # Errors
    ///
    /// Return [`RandomizationError::BlockExhausted`] if all assignments
    /// in this block have already been consumed.
    pub fn get_next(&mut self) -> RandomizationResult<u32> {
        if self.index >= self.values.len() {
            return Err(RandomizationError::BlockExhausted);
        }
        let value = self.values[self.index];
        self.index += 1;
        Ok(value)
    }

    fn try_generate(
        study_id: &Uuid,
        arms: &[Arm],
        block_size: Option<u32>,
        strata: Option<&Strata>,
        rng: &mut impl Rng,
    ) -> RandomizationResult<Self> {
        let block_size_minima = block_size_minima(arms);

        let block_size = match block_size {
            Some(bs) if bs > 0 && bs % block_size_minima != 0 => {
                return Err(RandomizationError::InvalidBlockSize {
                    block_size: bs,
                    ratio_sum: block_size_minima,
                });
            }
            Some(bs) if bs > 0 => bs,
            // None or Some(0): generate a random block size as a multiple of ratio_sum
            _ => block_size_minima * rng.random_range(2..=4),
        };

        let stratification = strata.map(String::from);

        let mut values = Vec::new();

        for (i, arm) in arms.iter().enumerate() {
            let n_arm_assignments = arm_assignment_count(block_size, block_size_minima, arm.ratio);
            let arm_assignments = vec![i as u32; n_arm_assignments as usize];
            values.extend(arm_assignments);
        }

        values.shuffle(rng);

        Ok(RandomPermutedBlock {
            id: Uuid::now_v7(),
            study_id: *study_id,
            stratification,
            index: 0,
            values,
        })
    }
}

/// Sum of all arm ratios — the minimum valid block size.
fn block_size_minima(arms: &[Arm]) -> u32 {
    arms.iter().map(|arm| arm.ratio).sum::<u32>()
}

/// Calculate how many slots in a block belong to a single arm.
pub(super) fn arm_assignment_count(block_size: u32, ratio_sum: u32, arm_ratio: u32) -> u32 {
    ((block_size as u64 * arm_ratio as u64) / ratio_sum as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::SmallRng;

    fn seeded_rng() -> SmallRng {
        SmallRng::seed_from_u64(42)
    }

    #[test]
    fn generate_block_with_specified_size() {
        let arms = vec![
            Arm { ratio: 1, ..Default::default() },
            Arm { ratio: 1, ..Default::default() },
        ];

        let block = RandomPermutedBlock::try_generate(
            &Default::default(), &arms, Some(6), None, &mut seeded_rng(),
        ).expect("should generate block with valid size");

        assert_eq!(block.values.len(), 6);
    }

    #[test]
    fn generate_block_respects_arm_ratios() {
        let arms = vec![
            Arm { ratio: 3, ..Default::default() },
            Arm { ratio: 1, ..Default::default() },
        ];

        let block = RandomPermutedBlock::try_generate(
            &Default::default(), &arms, Some(8), None, &mut seeded_rng(),
        ).expect("should generate block with valid size");

        assert_eq!(block.values.len(), 8);
        assert_eq!(block.values.iter().filter(|&&x| x == 0).count(), 6);
        assert_eq!(block.values.iter().filter(|&&x| x == 1).count(), 2);
    }

    #[test]
    fn generate_dynamic_block_is_multiple_of_ratio_sum() {
        let arms = vec![
            Arm { ratio: 3, ..Default::default() },
            Arm { ratio: 1, ..Default::default() },
        ];

        let block = RandomPermutedBlock::try_generate(
            &Default::default(), &arms, None, None, &mut seeded_rng(),
        ).expect("should generate block with dynamic size");

        let ratio_sum: usize = arms.iter().map(|a| a.ratio as usize).sum();
        assert_eq!(
            block.values.len() % ratio_sum, 0,
            "block size must be a multiple of ratio_sum"
        );

        let count_0 = block.values.iter().filter(|&&x| x == 0).count();
        let count_1 = block.values.iter().filter(|&&x| x == 1).count();
        assert_eq!(count_0, 3 * count_1, "3:1 ratio not maintained");
    }

    #[test]
    fn get_next_returns_values_in_sequence() {
        let mut block = RandomPermutedBlock {
            id: Uuid::nil(),
            study_id: Uuid::nil(),
            stratification: None,
            index: 0,
            values: vec![0, 1, 0, 1, 0, 1],
        };

        assert_eq!(block.get_next().unwrap(), 0);
        assert_eq!(block.get_next().unwrap(), 1);
        assert_eq!(block.get_next().unwrap(), 0);
    }

    #[test]
    fn get_next_returns_block_exhausted_when_index_at_end() {
        let mut block = RandomPermutedBlock {
            id: Uuid::nil(),
            study_id: Uuid::nil(),
            stratification: None,
            index: 2,
            values: vec![0, 1],
        };

        let err = block.get_next().unwrap_err();
        assert!(matches!(err, RandomizationError::BlockExhausted));
    }

    #[test]
    fn try_generate_returns_invalid_block_size_when_not_multiple_of_ratio_sum() {
        let arms = vec![
            Arm { ratio: 2, ..Default::default() },
            Arm { ratio: 1, ..Default::default() },
        ];

        let result = RandomPermutedBlock::try_generate(
            &Default::default(), &arms, Some(5), None, &mut seeded_rng(),
        );

        let err = result.unwrap_err();
        assert!(matches!(
            err,
            RandomizationError::InvalidBlockSize { block_size: 5, .. }
        ));
    }

    #[test]
    fn block_generation_is_deterministic_with_seeded_rng() {
        let arms = vec![
            Arm { ratio: 1, ..Default::default() },
            Arm { ratio: 1, ..Default::default() },
        ];

        let block_a = RandomPermutedBlock::try_generate(
            &Default::default(), &arms, Some(6), None, &mut seeded_rng(),
        ).unwrap();

        let block_b = RandomPermutedBlock::try_generate(
            &Default::default(), &arms, Some(6), None, &mut seeded_rng(),
        ).unwrap();

        assert_eq!(block_a.values, block_b.values, "same seed must produce same sequence");
    }

    #[test]
    fn block_shuffles_assignments_with_rng() {
        let arms = vec![
            Arm { ratio: 1, ..Default::default() },
            Arm { ratio: 1, ..Default::default() },
        ];

        let mut rng = seeded_rng();
        let block = RandomPermutedBlock::try_generate(
            &Default::default(), &arms, Some(6), None, &mut rng,
        ).unwrap();

        // With equal 1:1 ratios and size 6: 3 zeros, 3 ones
        assert_eq!(block.values.iter().filter(|&&x| x == 0).count(), 3);
        assert_eq!(block.values.iter().filter(|&&x| x == 1).count(), 3);

        // The sequence should not be sorted (shuffled)
        let sorted = {
            let mut v = block.values.clone();
            v.sort();
            v
        };
        assert_ne!(block.values, sorted, "block should be shuffled, not sorted");
    }

    #[test]
    fn dynamic_block_size_is_deterministic_with_seeded_rng() {
        let arms = vec![
            Arm { ratio: 2, ..Default::default() },
            Arm { ratio: 1, ..Default::default() },
        ];

        let block = RandomPermutedBlock::try_generate(
            &Default::default(), &arms, None, None, &mut seeded_rng(),
        ).unwrap();

        // ratio_sum = 3, multiplier from rng in 2..=4
        assert_eq!(
            block.values.len() % 3, 0,
            "block size must be multiple of ratio_sum"
        );
        assert!(
            block.values.len() >= 6 && block.values.len() <= 12,
            "block size should be ratio_sum * (2..=4), got {}",
            block.values.len()
        );
    }
}
