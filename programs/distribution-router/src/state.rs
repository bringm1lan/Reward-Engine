use crate::{RouterError, BUCKETS};
use anchor_lang::prelude::*;

pub const CAMPAIGNS: usize = 0;
pub const GIVEAWAYS: usize = 1;
pub const TIPS: usize = 2;
pub const PURCHASES: usize = 3;
pub const RESERVE: usize = 4;
pub const ROLLING_CAPACITY: usize = 64;

#[account]
#[derive(InitSpace)]
pub struct RouterConfig {
    pub authority: Pubkey,
    pub pending_authority: Pubkey,
    pub activation_treasury: Pubkey,
    pub reward_mint: Pubkey,
    pub activation_fee_lamports: u64,
    pub paused: bool,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct DistributionVault {
    pub mint: Pubkey,
    pub manager: Pubkey,
    pub pending_manager: Pubkey,
    pub policy: AllocationPolicy,
    /// Allocated SOL remains in this account until spent into a campaign/order.
    pub balances: [u64; BUCKETS],
    pub rules_version: u64,
    pub route_sequence: u64,
    pub total_allocated_lamports: u64,
    pub verified_creator_fees: u64,
    pub fee_kind: u8,
    pub fee_source: Pubkey,
    pub fee_creator: Pubkey,
    pub fee_share_bps: u16,
    pub paused: bool,
    pub bump: u8,
}

/// Website order: campaigns, giveaways, tips, TIPANSEM purchases, reserve.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace)]
pub struct AllocationPolicy {
    pub share_bps: [u16; BUCKETS],
    pub minimum_route_lamports: u64,
}
impl AllocationPolicy {
    pub fn validate(&self) -> Result<()> {
        require!(self.minimum_route_lamports > 0, RouterError::InvalidAmount);
        require_eq!(
            self.share_bps.iter().map(|&n| u32::from(n)).sum::<u32>(),
            10_000,
            RouterError::InvalidShares
        );
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct TokenBank {
    pub vault: Pubkey,
    pub mint: Pubkey,
    pub balances: [u64; BUCKETS],
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace)]
pub struct CampaignRules {
    pub verifier: Pubkey,
    /// Default pubkey means native SOL; otherwise a legacy SPL mint.
    pub reward_mint: Pubkey,
    pub bucket: u8,
    pub starts_at: i64,
    pub ends_at: i64,
    pub min_reward: u64,
    pub max_reward: u64,
    pub rolling_limit: u16,
    pub window_seconds: u32,
    pub daily_limit: u32,
    /// Zero disables only this additional lifetime cap.
    pub lifetime_limit: u32,
    pub max_epoch_budget: u64,
    pub refill_amount: u64,
    pub refill_threshold: u64,
    pub auto_refill: bool,
    pub recurring: bool,
    /// Commits to X/content rules checked by the off-chain verifier.
    pub rules_hash: [u8; 32],
}
impl CampaignRules {
    pub fn validate(&self) -> Result<()> {
        require!(
            self.verifier != Pubkey::default(),
            RouterError::InvalidAuthority
        );
        require!((self.bucket as usize) <= TIPS, RouterError::InvalidBucket);
        require!(
            self.ends_at > self.starts_at && self.starts_at >= 0,
            RouterError::InvalidTime
        );
        require!(
            self.ends_at
                .checked_sub(self.starts_at)
                .ok_or(RouterError::Overflow)?
                <= 366 * 86400,
            RouterError::InvalidTime
        );
        require!(
            self.min_reward > 0 && self.max_reward >= self.min_reward,
            RouterError::InvalidAmount
        );
        require!(
            self.rolling_limit > 0 && usize::from(self.rolling_limit) <= ROLLING_CAPACITY,
            RouterError::InvalidLimit
        );
        require!(
            self.window_seconds > 0 && self.window_seconds <= 7 * 86400 && self.daily_limit > 0,
            RouterError::InvalidLimit
        );
        require!(
            self.max_epoch_budget >= self.max_reward,
            RouterError::BudgetExceeded
        );
        if self.auto_refill {
            require!(
                self.refill_amount > 0
                    && self.refill_amount <= self.max_epoch_budget
                    && self.refill_threshold < self.max_epoch_budget,
                RouterError::InvalidLimit
            );
        }
        require!(self.rules_hash != [0; 32], RouterError::InvalidEvidence);
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct Campaign {
    pub vault: Pubkey,
    pub id: [u8; 32],
    pub rules: CampaignRules,
    /// 0 active, 1 paused, 2 permanently closed.
    pub status: u8,
    pub epoch: u64,
    pub epoch_funded: u64,
    pub remaining: u64,
    pub total_funded: u64,
    pub total_paid: u64,
    pub reward_count: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Participant {
    pub campaign: Pubkey,
    pub user_id: [u8; 32],
    pub recipient: Pubkey,
    pub lifetime_count: u64,
    pub utc_day: i64,
    pub daily_count: u32,
    pub cursor: u8,
    pub timestamps: [i64; ROLLING_CAPACITY],
}

#[account]
#[derive(InitSpace)]
pub struct RewardReceipt {
    pub campaign: Pubkey,
    pub post_id: [u8; 32],
    pub user_id: [u8; 32],
    pub recipient: Pubkey,
    pub amount: u64,
    pub epoch: u64,
    pub paid_at: i64,
    pub rules_hash: [u8; 32],
}
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct Award {
    pub post_id: [u8; 32],
    pub user_id: [u8; 32],
    pub amount: u64,
    pub expires_at: i64,
    pub rules_hash: [u8; 32],
}

#[account]
#[derive(InitSpace)]
pub struct PurchaseOrder {
    pub vault: Pubkey,
    pub id: [u8; 32],
    pub reward_mint: Pubkey,
    pub input_lamports: u64,
    pub minimum_tokens: u64,
    pub expires_at: i64,
    pub output_bucket: u8,
    /// 0 open, 1 filled, 2 refunded. Keep receipts to prevent replay.
    pub status: u8,
    pub bump: u8,
}

#[event]
pub struct VaultRegistered {
    pub vault: Pubkey,
    pub mint: Pubkey,
    pub manager: Pubkey,
    pub activation_fee_lamports: u64,
}
#[event]
pub struct VaultDeposit {
    pub vault: Pubkey,
    pub contributor: Pubkey,
    pub amount: u64,
}
#[event]
pub struct FundsAllocated {
    pub vault: Pubkey,
    pub sequence: u64,
    pub rules_version: u64,
    pub amount: u64,
    pub allocations: [u64; BUCKETS],
}
#[event]
pub struct CreatorFeesCollected {
    pub vault: Pubkey,
    pub source: Pubkey,
    pub kind: u8,
    pub amount: u64,
}
#[event]
pub struct CampaignFunded {
    pub campaign: Pubkey,
    pub vault: Pubkey,
    pub epoch: u64,
    pub amount: u64,
    pub remaining: u64,
}
#[event]
pub struct RewardPaid {
    pub campaign: Pubkey,
    pub post_id: [u8; 32],
    pub user_id: [u8; 32],
    pub recipient: Pubkey,
    pub reward_mint: Pubkey,
    pub amount: u64,
    pub epoch: u64,
}
#[event]
pub struct CampaignRefunded {
    pub campaign: Pubkey,
    pub amount: u64,
}
#[event]
pub struct PurchaseFilled {
    pub order: Pubkey,
    pub vault: Pubkey,
    pub executor: Pubkey,
    pub input_lamports: u64,
    pub tokens_received: u64,
}
