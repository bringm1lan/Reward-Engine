use anchor_lang::prelude::*;
mod accounting;
mod campaigns;
mod fees;
mod policy;
mod purchases;
mod state;
mod vault;
pub use campaigns::*;
pub use fees::*;
pub use purchases::*;
pub use state::*;
pub use vault::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkgMQHGJkY4KT");
pub const ROUTER_SEED: &[u8] = b"distribution_router";
pub const VAULT_SEED: &[u8] = b"distribution_vault";
pub const BUCKETS: usize = 5;
pub const DEFAULT_ACTIVATION_FEE_LAMPORTS: u64 = 200_000_000;

#[program]
pub mod distribution_router {
    use super::*;
    pub fn initialize_router(ctx: Context<InitializeRouter>, reward_mint: Pubkey) -> Result<()> {
        vault::initialize(ctx, reward_mint)
    }
    pub fn configure_router(
        ctx: Context<AdminRouter>,
        treasury: Pubkey,
        fee: u64,
        paused: bool,
    ) -> Result<()> {
        vault::configure_router(ctx, treasury, fee, paused)
    }
    pub fn register_vault(ctx: Context<RegisterVault>, policy: AllocationPolicy) -> Result<()> {
        vault::register(ctx, policy)
    }
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        vault::deposit(ctx, amount)
    }
    pub fn route(ctx: Context<Allocate>, sequence: u64, version: u64, minimum: u64) -> Result<()> {
        vault::allocate(ctx, sequence, version, minimum)
    }
    pub fn configure_vault(ctx: Context<ManageVault>, policy: AllocationPolicy) -> Result<()> {
        policy.validate()?;
        ctx.accounts.vault.policy = policy;
        ctx.accounts.vault.rules_version = accounting::add(ctx.accounts.vault.rules_version, 1)?;
        Ok(())
    }
    pub fn pause_vault(ctx: Context<ManageVault>, paused: bool) -> Result<()> {
        ctx.accounts.vault.paused = paused;
        Ok(())
    }
    pub fn rebalance_sol(ctx: Context<ManageVault>, from: u8, to: u8, amount: u64) -> Result<()> {
        require!(from != to, RouterError::InvalidBucket);
        accounting::debit(&mut ctx.accounts.vault.balances, from, amount)?;
        accounting::credit(&mut ctx.accounts.vault.balances, to, amount)
    }
    pub fn withdraw_reserve(ctx: Context<WithdrawReserve>, amount: u64) -> Result<()> {
        vault::withdraw_reserve(ctx, amount)
    }
    pub fn propose_manager(ctx: Context<ManageVault>, next: Pubkey) -> Result<()> {
        require!(next != Pubkey::default(), RouterError::InvalidAuthority);
        ctx.accounts.vault.pending_manager = next;
        Ok(())
    }
    pub fn accept_manager(ctx: Context<AcceptManager>) -> Result<()> {
        ctx.accounts.vault.manager = ctx.accounts.next_manager.key();
        ctx.accounts.vault.pending_manager = Pubkey::default();
        Ok(())
    }
    pub fn propose_authority(ctx: Context<AdminRouter>, next: Pubkey) -> Result<()> {
        require!(next != Pubkey::default(), RouterError::InvalidAuthority);
        ctx.accounts.router.pending_authority = next;
        Ok(())
    }
    pub fn accept_authority(ctx: Context<AcceptAuthority>) -> Result<()> {
        ctx.accounts.router.authority = ctx.accounts.next_authority.key();
        ctx.accounts.router.pending_authority = Pubkey::default();
        Ok(())
    }
    pub fn initialize_token_bank(ctx: Context<InitializeTokenBank>) -> Result<()> {
        purchases::initialize_bank(ctx)
    }
    pub fn deposit_tokens(ctx: Context<DepositTokens>, amount: u64, bucket: u8) -> Result<()> {
        purchases::deposit_tokens(ctx, amount, bucket)
    }
    pub fn sync_token_donations(ctx: Context<SyncTokenDonations>, bucket: u8) -> Result<()> {
        purchases::sync_donations(ctx, bucket)
    }
    pub fn create_purchase(
        ctx: Context<CreatePurchase>,
        id: [u8; 32],
        amount: u64,
        minimum_tokens: u64,
        expires_at: i64,
        bucket: u8,
    ) -> Result<()> {
        purchases::create(ctx, id, amount, minimum_tokens, expires_at, bucket)
    }
    pub fn fill_purchase(ctx: Context<FillPurchase>, token_amount: u64) -> Result<()> {
        purchases::fill(ctx, token_amount)
    }
    pub fn cancel_purchase(ctx: Context<CancelPurchase>) -> Result<()> {
        purchases::cancel(ctx)
    }
    pub fn create_campaign(
        ctx: Context<CreateCampaign>,
        id: [u8; 32],
        rules: CampaignRules,
    ) -> Result<()> {
        campaigns::create(ctx, id, rules)
    }
    pub fn initialize_campaign_tokens(_ctx: Context<InitializeCampaignTokens>) -> Result<()> {
        Ok(())
    }
    pub fn fund_campaign_sol(ctx: Context<FundCampaignSol>, amount: u64) -> Result<()> {
        campaigns::fund_sol(ctx, amount, false)
    }
    pub fn refill_campaign_sol(ctx: Context<FundCampaignSol>) -> Result<()> {
        campaigns::fund_sol(ctx, 0, true)
    }
    pub fn fund_campaign_tokens(ctx: Context<FundCampaignTokens>, amount: u64) -> Result<()> {
        campaigns::fund_tokens(ctx, amount, false)
    }
    pub fn refill_campaign_tokens(ctx: Context<FundCampaignTokens>) -> Result<()> {
        campaigns::fund_tokens(ctx, 0, true)
    }
    pub fn pause_campaign(ctx: Context<ManageCampaign>, paused: bool) -> Result<()> {
        require!(ctx.accounts.campaign.status != 2, RouterError::Closed);
        ctx.accounts.campaign.status = if paused { 1 } else { 0 };
        Ok(())
    }
    pub fn advance_campaign_epoch(ctx: Context<AdvanceEpoch>) -> Result<()> {
        campaigns::advance_epoch(ctx)
    }
    pub fn award_sol(ctx: Context<AwardSol>, award: Award) -> Result<()> {
        campaigns::award_sol(ctx, award)
    }
    pub fn award_tokens(ctx: Context<AwardTokens>, award: Award) -> Result<()> {
        campaigns::award_tokens(ctx, award)
    }
    pub fn close_campaign_sol(ctx: Context<CloseCampaignSol>) -> Result<()> {
        campaigns::close_sol(ctx)
    }
    pub fn close_campaign_tokens(ctx: Context<CloseCampaignTokens>) -> Result<()> {
        campaigns::close_tokens(ctx)
    }
    pub fn create_pump_sharing<'info>(
        ctx: Context<'_, '_, '_, 'info, FeeSetup<'info>>,
    ) -> Result<()> {
        fees::create_pump(ctx)
    }
    pub fn finalize_pump_sharing<'info>(
        ctx: Context<'_, '_, '_, 'info, FeeSetup<'info>>,
        share_bps: u16,
    ) -> Result<()> {
        fees::finalize_pump(ctx, share_bps)
    }
    pub fn connect_pump(ctx: Context<ConnectPump>) -> Result<()> {
        fees::connect_pump(ctx)
    }
    pub fn sweep_pump_amm<'info>(
        ctx: Context<'_, '_, '_, 'info, CollectPump<'info>>,
    ) -> Result<()> {
        fees::sweep_pump(ctx)
    }
    pub fn collect_pump_fees<'info>(
        ctx: Context<'_, '_, '_, 'info, CollectPump<'info>>,
        minimum_received: u64,
    ) -> Result<()> {
        fees::collect_pump(ctx, minimum_received)
    }
    pub fn sweep_pump_buckets<'info>(
        ctx: Context<'_, '_, '_, 'info, SweepPumpBuckets<'info>>,
        amm: bool,
    ) -> Result<()> {
        fees::sweep_buckets(ctx, amm)
    }
    pub fn connect_launchlab(ctx: Context<ConnectLaunchLab>, share_bps: u16) -> Result<()> {
        fees::connect_launchlab(ctx, share_bps)
    }
    pub fn relay_launchlab_fees(ctx: Context<RelayLaunchLab>, amount: u64) -> Result<()> {
        fees::relay_launchlab(ctx, amount)
    }
    pub fn claim_launchlab_fees<'info>(
        ctx: Context<'_, '_, '_, 'info, ClaimLaunchLab<'info>>,
        minimum_received: u64,
    ) -> Result<()> {
        fees::claim_launchlab(ctx, minimum_received)
    }
}

#[error_code]
pub enum RouterError {
    #[msg("Required manager, creator, registrar or verifier authorization is missing")]
    InvalidAuthority,
    #[msg("The router, vault or campaign is paused")]
    Paused,
    #[msg("Shares must total 10000 basis points")]
    InvalidShares,
    #[msg("Invalid or substituted recipient")]
    InvalidRecipient,
    #[msg("Amount must be positive and within the configured bounds")]
    InvalidAmount,
    #[msg("Not enough available funds; allocated and escrowed balances cannot be spent twice")]
    InsufficientFunds,
    #[msg("The allocation/refill threshold has not been reached")]
    BelowThreshold,
    #[msg("A batch with this sequence has already executed")]
    StaleSequence,
    #[msg("Allocation policy changed; refresh the version")]
    StalePolicy,
    #[msg("Integer arithmetic overflow")]
    Overflow,
    #[msg("Invalid allocation bucket")]
    InvalidBucket,
    #[msg("Invalid start, end or expiration timestamp")]
    InvalidTime,
    #[msg("Campaign is outside its participation window")]
    OutsideWindow,
    #[msg("Campaign epoch funding cap exceeded")]
    BudgetExceeded,
    #[msg("Reward limit exceeded")]
    InvalidLimit,
    #[msg("Post/user identity or content-rule commitment is invalid")]
    InvalidEvidence,
    #[msg("Campaign or purchase is already terminal")]
    Closed,
    #[msg("Wrong reward mint or token program")]
    InvalidMint,
    #[msg("Creator-fee accounts do not belong to this mint or official launchpad")]
    InvalidFeeSource,
    #[msg("Fee configuration is immutable, shared or not controlled by this creator")]
    FeeAuthorityLocked,
    #[msg("Native SOL quote assets only")]
    UnsupportedQuote,
    #[msg("This launchpad cannot automatically redirect this creator's fees")]
    RelayRequired,
    #[msg("Purchase returned fewer reward tokens than the manager approved")]
    SlippageExceeded,
}
