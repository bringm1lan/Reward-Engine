use crate::{accounting::*, vault::live, *};
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};

pub(crate) fn initialize_bank(ctx: Context<InitializeTokenBank>) -> Result<()> {
    let b = &mut ctx.accounts.bank;
    b.vault = ctx.accounts.vault.key();
    b.mint = ctx.accounts.mint.key();
    b.bump = ctx.bumps.bank;
    Ok(())
}
pub(crate) fn deposit_tokens(ctx: Context<DepositTokens>, amount: u64, bucket: u8) -> Result<()> {
    require!(amount > 0, RouterError::InvalidAmount);
    // Reward banks fund campaigns/giveaways/tips; purchase and reserve buckets
    // are SOL-only. Reject a destination that has no SPL spending path.
    require!((bucket as usize) <= TIPS, RouterError::InvalidBucket);
    token::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.source.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.bank_tokens.to_account_info(),
                authority: ctx.accounts.contributor.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;
    credit(&mut ctx.accounts.bank.balances, bucket, amount)
}
pub(crate) fn sync_donations(ctx: Context<SyncTokenDonations>, bucket: u8) -> Result<()> {
    require!((bucket as usize) <= TIPS, RouterError::InvalidBucket);
    let amount = sub(
        ctx.accounts.bank_tokens.amount,
        total(&ctx.accounts.bank.balances)?,
    )?;
    require!(amount > 0, RouterError::InvalidAmount);
    credit(&mut ctx.accounts.bank.balances, bucket, amount)
}
pub(crate) fn create(
    ctx: Context<CreatePurchase>,
    id: [u8; 32],
    amount: u64,
    minimum_tokens: u64,
    expires_at: i64,
    bucket: u8,
) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    let now = Clock::get()?.unix_timestamp;
    require!(amount > 0 && minimum_tokens > 0, RouterError::InvalidAmount);
    require!(
        expires_at > now && expires_at <= now.checked_add(86400).ok_or(RouterError::Overflow)?,
        RouterError::InvalidTime
    );
    require!((bucket as usize) <= TIPS, RouterError::InvalidBucket);
    debit(&mut ctx.accounts.vault.balances, PURCHASES as u8, amount)?;
    transfer_lamports(
        &ctx.accounts.vault.to_account_info(),
        &ctx.accounts.order.to_account_info(),
        amount,
    )?;
    let o = &mut ctx.accounts.order;
    o.vault = ctx.accounts.vault.key();
    o.id = id;
    o.reward_mint = ctx.accounts.router.reward_mint;
    o.input_lamports = amount;
    o.minimum_tokens = minimum_tokens;
    o.expires_at = expires_at;
    o.output_bucket = bucket;
    o.bump = ctx.bumps.order;
    Ok(())
}
/// Atomic RFQ settlement: a keeper delivers at least the approved token amount
/// before receiving SOL. The keeper may source tokens through a DEX separately.
/// No caller-supplied DEX bytecode is ever given the vault's signing authority.
pub(crate) fn fill(ctx: Context<FillPurchase>, token_amount: u64) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    let o = &mut ctx.accounts.order;
    require_eq!(o.status, 0, RouterError::Closed);
    require!(
        Clock::get()?.unix_timestamp < o.expires_at,
        RouterError::OutsideWindow
    );
    require!(
        token_amount >= o.minimum_tokens,
        RouterError::SlippageExceeded
    );
    token::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.source.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.bank_tokens.to_account_info(),
                authority: ctx.accounts.executor.to_account_info(),
            },
        ),
        token_amount,
        ctx.accounts.mint.decimals,
    )?;
    credit(
        &mut ctx.accounts.bank.balances,
        o.output_bucket,
        token_amount,
    )?;
    transfer_lamports(
        &o.to_account_info(),
        &ctx.accounts.executor.to_account_info(),
        o.input_lamports,
    )?;
    // Return unsolicited SOL dust to this project's purchase bucket.
    let extra = spendable(&o.to_account_info())?;
    if extra > 0 {
        transfer_lamports(
            &o.to_account_info(),
            &ctx.accounts.vault.to_account_info(),
            extra,
        )?;
        credit(&mut ctx.accounts.vault.balances, PURCHASES as u8, extra)?;
    }
    o.status = 1;
    emit!(PurchaseFilled {
        order: o.key(),
        vault: o.vault,
        executor: ctx.accounts.executor.key(),
        input_lamports: o.input_lamports,
        tokens_received: token_amount
    });
    Ok(())
}
pub(crate) fn cancel(ctx: Context<CancelPurchase>) -> Result<()> {
    let o = &mut ctx.accounts.order;
    require_eq!(o.status, 0, RouterError::Closed);
    require!(
        ctx.accounts.actor.key() == ctx.accounts.vault.manager
            || Clock::get()?.unix_timestamp >= o.expires_at,
        RouterError::InvalidAuthority
    );
    let amount = spendable(&o.to_account_info())?;
    require!(amount >= o.input_lamports, RouterError::InsufficientFunds);
    transfer_lamports(
        &o.to_account_info(),
        &ctx.accounts.vault.to_account_info(),
        amount,
    )?;
    credit(&mut ctx.accounts.vault.balances, PURCHASES as u8, amount)?;
    o.status = 2;
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeTokenBank<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    pub mint: Box<Account<'info, Mint>>,
    #[account(init, payer = payer, space = 8 + TokenBank::INIT_SPACE, seeds = [b"token_bank", vault.key().as_ref(), mint.key().as_ref()], bump)]
    pub bank: Box<Account<'info, TokenBank>>,
    #[account(init, payer = payer, token::mint = mint, token::authority = vault, seeds = [b"bank_tokens", bank.key().as_ref()], bump)]
    pub bank_tokens: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct DepositTokens<'info> {
    pub contributor: Signer<'info>,
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut, seeds = [b"token_bank", bank.vault.as_ref(), mint.key().as_ref()], bump = bank.bump, has_one = mint)]
    pub bank: Box<Account<'info, TokenBank>>,
    #[account(mut, token::mint = mint, token::authority = contributor)]
    pub source: Box<Account<'info, TokenAccount>>,
    #[account(mut, seeds = [b"bank_tokens", bank.key().as_ref()], bump, token::mint = mint, constraint = bank_tokens.owner == bank.vault @ RouterError::InvalidAuthority)]
    pub bank_tokens: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
pub struct SyncTokenDonations<'info> {
    pub manager: Signer<'info>,
    #[account(seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump, has_one = manager @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"token_bank", vault.key().as_ref(), bank.mint.as_ref()], bump = bank.bump, has_one = vault)]
    pub bank: Box<Account<'info, TokenBank>>,
    #[account(seeds = [b"bank_tokens", bank.key().as_ref()], bump, constraint = bank_tokens.mint == bank.mint && bank_tokens.owner == vault.key() @ RouterError::InvalidMint)]
    pub bank_tokens: Box<Account<'info, TokenAccount>>,
}
#[derive(Accounts)]
#[instruction(id: [u8;32])]
pub struct CreatePurchase<'info> {
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(mut)]
    pub manager: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump, has_one = manager @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(init, payer = manager, space = 8 + PurchaseOrder::INIT_SPACE, seeds = [b"purchase", vault.key().as_ref(), id.as_ref()], bump)]
    pub order: Box<Account<'info, PurchaseOrder>>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct FillPurchase<'info> {
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(mut)]
    pub executor: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"purchase", vault.key().as_ref(), order.id.as_ref()], bump = order.bump, has_one = vault)]
    pub order: Box<Account<'info, PurchaseOrder>>,
    #[account(address = order.reward_mint @ RouterError::InvalidMint)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut, seeds = [b"token_bank", vault.key().as_ref(), mint.key().as_ref()], bump = bank.bump, has_one = vault, has_one = mint)]
    pub bank: Box<Account<'info, TokenBank>>,
    #[account(mut, token::mint = mint, token::authority = executor)]
    pub source: Box<Account<'info, TokenAccount>>,
    #[account(mut, seeds = [b"bank_tokens", bank.key().as_ref()], bump, token::mint = mint, token::authority = vault)]
    pub bank_tokens: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
pub struct CancelPurchase<'info> {
    pub actor: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"purchase", vault.key().as_ref(), order.id.as_ref()], bump = order.bump, has_one = vault)]
    pub order: Box<Account<'info, PurchaseOrder>>,
}
