use crate::{RouterError, BUCKETS};
use anchor_lang::prelude::*;

/// Use u128 intermediates. Assign rounding dust to the final nonzero bucket,
/// so the allocations conserve every lamport without funding a disabled bucket.
pub fn split_lamports(amount: u64, shares: [u16; BUCKETS]) -> Result<[u64; BUCKETS]> {
    require_eq!(
        shares.iter().map(|&share| u32::from(share)).sum::<u32>(),
        10_000,
        RouterError::InvalidShares
    );
    let last = shares
        .iter()
        .rposition(|&share| share > 0)
        .ok_or(RouterError::InvalidShares)?;
    let mut allocations = [0u64; BUCKETS];
    let mut distributed = 0u64;
    for (index, share) in shares.into_iter().enumerate() {
        allocations[index] = if index == last {
            amount
                .checked_sub(distributed)
                .ok_or(RouterError::Overflow)?
        } else {
            u64::try_from(u128::from(amount) * u128::from(share) / 10_000)
                .map_err(|_| RouterError::Overflow)?
        };
        distributed = distributed
            .checked_add(allocations[index])
            .ok_or(RouterError::Overflow)?;
    }
    Ok(allocations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conserves_funds_for_small_large_and_fractional_splits() {
        for shares in [
            [5000, 2000, 1000, 1000, 1000],
            [3333, 3333, 3334, 0, 0],
            [0, 10000, 0, 0, 0],
        ] {
            for amount in [1, 3, 9999, 10_001, 1_000_000_000, u64::MAX] {
                let result = split_lamports(amount, shares).unwrap();
                assert_eq!(
                    result.iter().map(|&part| u128::from(part)).sum::<u128>(),
                    u128::from(amount)
                );
                for (share, part) in shares.into_iter().zip(result) {
                    if share == 0 {
                        assert_eq!(part, 0);
                    }
                }
            }
        }
    }

    #[test]
    fn rejects_over_and_under_allocated_policies() {
        for shares in [
            [0; BUCKETS],
            [2500, 2500, 2500, 2499, 0],
            [u16::MAX; BUCKETS],
        ] {
            assert!(split_lamports(1_000_000, shares).is_err());
        }
    }
}
