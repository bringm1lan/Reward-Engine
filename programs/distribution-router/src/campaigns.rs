use crate::{accounting::*, vault::live, *};
use anchor_spl::token::{self, Mint, Token, TokenAccount, TransferChecked};

pub(crate) fn create(
    ctx: Context<CreateCampaign>,
    id: [u8; 32],
    rules: CampaignRules,
) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    rules.validate()?;
    require!(
        id != [0; 32] && rules.ends_at > Clock::get()?.unix_timestamp,
        RouterError::InvalidTime
    );
    let c = &mut ctx.accounts.campaign;
    c.vault = ctx.accounts.vault.key();
    c.id = id;
    c.rules = rules;
    c.bump = ctx.bumps.campaign;
    Ok(())
}

fn funding_amount(
    c: &Campaign,
    manager: Pubkey,
    actor: Pubkey,
    requested: u64,
    automatic: bool,
    available: u64,
) -> Result<u64> {
    require_eq!(c.status, 0, RouterError::Paused);
    let now = Clock::get()?.unix_timestamp;
    require!(now < c.rules.ends_at, RouterError::OutsideWindow);
    let cap = sub(c.rules.max_epoch_budget, c.epoch_funded)?;
    let amount = if automatic {
        require!(
            c.rules.auto_refill
                && now >= c.rules.starts_at
                && c.remaining <= c.rules.refill_threshold,
            RouterError::BelowThreshold
        );
        c.rules.refill_amount.min(cap).min(available)
    } else {
        require_keys_eq!(manager, actor, RouterError::InvalidAuthority);
        requested
    };
    require!(amount > 0, RouterError::InvalidAmount);
    require!(amount <= cap, RouterError::BudgetExceeded);
    require!(amount <= available, RouterError::InsufficientFunds);
    Ok(amount)
}
fn funded(c: &mut Account<Campaign>, amount: u64) -> Result<()> {
    c.epoch_funded = add(c.epoch_funded, amount)?;
    c.remaining = add(c.remaining, amount)?;
    c.total_funded = add(c.total_funded, amount)?;
    emit!(CampaignFunded {
        campaign: c.key(),
        vault: c.vault,
        epoch: c.epoch,
        amount,
        remaining: c.remaining
    });
    Ok(())
}
pub(crate) fn fund_sol(
    ctx: Context<FundCampaignSol>,
    requested: u64,
    automatic: bool,
) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    require!(
        ctx.accounts.campaign.rules.reward_mint == Pubkey::default(),
        RouterError::InvalidMint
    );
    let bucket = ctx.accounts.campaign.rules.bucket;
    let amount = funding_amount(
        &ctx.accounts.campaign,
        ctx.accounts.vault.manager,
        ctx.accounts.actor.key(),
        requested,
        automatic,
        ctx.accounts.vault.balances[bucket as usize],
    )?;
    debit(&mut ctx.accounts.vault.balances, bucket, amount)?;
    transfer_lamports(
        &ctx.accounts.vault.to_account_info(),
        &ctx.accounts.campaign.to_account_info(),
        amount,
    )?;
    funded(&mut ctx.accounts.campaign, amount)
}
pub(crate) fn fund_tokens(
    ctx: Context<FundCampaignTokens>,
    requested: u64,
    automatic: bool,
) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    let bucket = ctx.accounts.campaign.rules.bucket;
    let amount = funding_amount(
        &ctx.accounts.campaign,
        ctx.accounts.vault.manager,
        ctx.accounts.actor.key(),
        requested,
        automatic,
        ctx.accounts.bank.balances[bucket as usize],
    )?;
    debit(&mut ctx.accounts.bank.balances, bucket, amount)?;
    let mint = ctx.accounts.vault.mint;
    let bump = [ctx.accounts.vault.bump];
    let seeds: &[&[u8]] = &[VAULT_SEED, mint.as_ref(), &bump];
    token::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.bank_tokens.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.escrow.to_account_info(),
                authority: ctx.accounts.vault.to_account_info(),
            },
            &[seeds],
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;
    funded(&mut ctx.accounts.campaign, amount)
}
pub(crate) fn advance_epoch(ctx: Context<AdvanceEpoch>) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    let c = &mut ctx.accounts.campaign;
    require!(c.status == 0 && c.rules.recurring, RouterError::Closed);
    let now = Clock::get()?.unix_timestamp;
    require!(now >= c.rules.ends_at, RouterError::OutsideWindow);
    let duration = c
        .rules
        .ends_at
        .checked_sub(c.rules.starts_at)
        .ok_or(RouterError::Overflow)?;
    c.rules.starts_at = now;
    c.rules.ends_at = now.checked_add(duration).ok_or(RouterError::Overflow)?;
    c.epoch = add(c.epoch, 1)?;
    c.epoch_funded = c.remaining;
    Ok(())
}

fn approve(
    c: &mut Account<Campaign>,
    p: &mut Account<Participant>,
    receipt: &mut Account<RewardReceipt>,
    recipient: Pubkey,
    award: Award,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require_eq!(c.status, 0, RouterError::Paused);
    require!(
        now >= c.rules.starts_at && now < c.rules.ends_at,
        RouterError::OutsideWindow
    );
    require!(
        award.expires_at >= now
            && award.expires_at <= now.checked_add(3600).ok_or(RouterError::Overflow)?,
        RouterError::InvalidTime
    );
    require!(
        award.post_id != [0; 32]
            && award.user_id != [0; 32]
            && award.rules_hash == c.rules.rules_hash,
        RouterError::InvalidEvidence
    );
    require!(
        award.amount >= c.rules.min_reward && award.amount <= c.rules.max_reward,
        RouterError::InvalidAmount
    );
    require!(
        recipient != Pubkey::default(),
        RouterError::InvalidRecipient
    );
    if p.campaign == Pubkey::default() {
        p.campaign = c.key();
        p.user_id = award.user_id;
        p.recipient = recipient;
    }
    require!(
        p.campaign == c.key() && p.user_id == award.user_id && p.recipient == recipient,
        RouterError::InvalidEvidence
    );
    let day = now.div_euclid(86400);
    if p.utc_day != day {
        p.utc_day = day;
        p.daily_count = 0;
    }
    require!(
        p.daily_count < c.rules.daily_limit,
        RouterError::InvalidLimit
    );
    require!(
        c.rules.lifetime_limit == 0 || p.lifetime_count < u64::from(c.rules.lifetime_limit),
        RouterError::InvalidLimit
    );
    let cutoff = now
        .checked_sub(i64::from(c.rules.window_seconds))
        .ok_or(RouterError::Overflow)?;
    require!(
        p.timestamps
            .iter()
            .filter(|&&time| time > cutoff && time <= now)
            .count()
            < usize::from(c.rules.rolling_limit),
        RouterError::InvalidLimit
    );
    c.remaining = sub(c.remaining, award.amount)?;
    c.total_paid = add(c.total_paid, award.amount)?;
    c.reward_count = add(c.reward_count, 1)?;
    p.daily_count = p.daily_count.checked_add(1).ok_or(RouterError::Overflow)?;
    p.lifetime_count = add(p.lifetime_count, 1)?;
    let cursor = p.cursor as usize;
    p.timestamps[cursor] = now;
    p.cursor = ((usize::from(p.cursor) + 1) % ROLLING_CAPACITY) as u8;
    receipt.campaign = c.key();
    receipt.post_id = award.post_id;
    receipt.user_id = award.user_id;
    receipt.recipient = recipient;
    receipt.amount = award.amount;
    receipt.epoch = c.epoch;
    receipt.paid_at = now;
    receipt.rules_hash = award.rules_hash;
    emit!(RewardPaid {
        campaign: c.key(),
        post_id: award.post_id,
        user_id: award.user_id,
        recipient,
        reward_mint: c.rules.reward_mint,
        amount: award.amount,
        epoch: c.epoch
    });
    Ok(())
}
pub(crate) fn award_sol(ctx: Context<AwardSol>, award: Award) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    require!(
        ctx.accounts.campaign.rules.reward_mint == Pubkey::default(),
        RouterError::InvalidMint
    );
    approve(
        &mut ctx.accounts.campaign,
        &mut ctx.accounts.participant,
        &mut ctx.accounts.receipt,
        ctx.accounts.recipient.key(),
        award,
    )?;
    transfer_lamports(
        &ctx.accounts.campaign.to_account_info(),
        &ctx.accounts.recipient.to_account_info(),
        award.amount,
    )
}
pub(crate) fn award_tokens(ctx: Context<AwardTokens>, award: Award) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    approve(
        &mut ctx.accounts.campaign,
        &mut ctx.accounts.participant,
        &mut ctx.accounts.receipt,
        ctx.accounts.recipient.key(),
        award,
    )?;
    let c = &ctx.accounts.campaign;
    let bump = [c.bump];
    let seeds: &[&[u8]] = &[b"campaign", c.vault.as_ref(), &c.id, &bump];
    token::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.escrow.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.recipient_tokens.to_account_info(),
                authority: c.to_account_info(),
            },
            &[seeds],
        ),
        award.amount,
        ctx.accounts.mint.decimals,
    )
}
fn may_close(c: &Campaign, manager: Pubkey, actor: Pubkey) -> Result<()> {
    require!(c.status != 2, RouterError::Closed);
    require!(
        actor == manager || (!c.rules.recurring && Clock::get()?.unix_timestamp >= c.rules.ends_at),
        RouterError::InvalidAuthority
    );
    Ok(())
}
pub(crate) fn close_sol(ctx: Context<CloseCampaignSol>) -> Result<()> {
    let c = &mut ctx.accounts.campaign;
    may_close(c, ctx.accounts.vault.manager, ctx.accounts.actor.key())?;
    require!(
        c.rules.reward_mint == Pubkey::default(),
        RouterError::InvalidMint
    );
    let amount = spendable(&c.to_account_info())?;
    require!(amount >= c.remaining, RouterError::InsufficientFunds);
    if amount > 0 {
        transfer_lamports(
            &c.to_account_info(),
            &ctx.accounts.vault.to_account_info(),
            amount,
        )?;
        credit(&mut ctx.accounts.vault.balances, c.rules.bucket, amount)?;
    }
    c.remaining = 0;
    c.status = 2;
    emit!(CampaignRefunded {
        campaign: c.key(),
        amount
    });
    Ok(())
}
pub(crate) fn close_tokens(ctx: Context<CloseCampaignTokens>) -> Result<()> {
    let c = &mut ctx.accounts.campaign;
    may_close(c, ctx.accounts.vault.manager, ctx.accounts.actor.key())?;
    let amount = ctx.accounts.escrow.amount;
    require!(amount >= c.remaining, RouterError::InsufficientFunds);
    let bump = [c.bump];
    let seeds: &[&[u8]] = &[b"campaign", c.vault.as_ref(), &c.id, &bump];
    if amount > 0 {
        token::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.escrow.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.bank_tokens.to_account_info(),
                    authority: c.to_account_info(),
                },
                &[seeds],
            ),
            amount,
            ctx.accounts.mint.decimals,
        )?;
        credit(&mut ctx.accounts.bank.balances, c.rules.bucket, amount)?;
    }
    c.remaining = 0;
    c.status = 2;
    emit!(CampaignRefunded {
        campaign: c.key(),
        amount
    });
    Ok(())
}

#[derive(Accounts)]
#[instruction(id: [u8;32])]
pub struct CreateCampaign<'info> {
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(mut)]
    pub manager: Signer<'info>,
    #[account(seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump, has_one = manager @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(init, payer = manager, space = 8 + Campaign::INIT_SPACE, seeds = [b"campaign", vault.key().as_ref(), id.as_ref()], bump)]
    pub campaign: Box<Account<'info, Campaign>>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct InitializeCampaignTokens<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds = [b"campaign", campaign.vault.as_ref(), campaign.id.as_ref()], bump = campaign.bump)]
    pub campaign: Box<Account<'info, Campaign>>,
    #[account(address = campaign.rules.reward_mint @ RouterError::InvalidMint)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(init, payer = payer, token::mint = mint, token::authority = campaign, seeds = [b"campaign_tokens", campaign.key().as_ref()], bump)]
    pub escrow: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct FundCampaignSol<'info> {
    pub actor: Signer<'info>,
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"campaign", vault.key().as_ref(), campaign.id.as_ref()], bump = campaign.bump, has_one = vault)]
    pub campaign: Box<Account<'info, Campaign>>,
}
#[derive(Accounts)]
pub struct FundCampaignTokens<'info> {
    pub actor: Signer<'info>,
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"campaign", vault.key().as_ref(), campaign.id.as_ref()], bump = campaign.bump, has_one = vault)]
    pub campaign: Box<Account<'info, Campaign>>,
    #[account(address = campaign.rules.reward_mint @ RouterError::InvalidMint)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut, seeds = [b"token_bank", vault.key().as_ref(), mint.key().as_ref()], bump = bank.bump, has_one = vault, has_one = mint)]
    pub bank: Box<Account<'info, TokenBank>>,
    #[account(mut, seeds = [b"bank_tokens", bank.key().as_ref()], bump, token::mint = mint, token::authority = vault)]
    pub bank_tokens: Box<Account<'info, TokenAccount>>,
    #[account(mut, seeds = [b"campaign_tokens", campaign.key().as_ref()], bump, token::mint = mint, token::authority = campaign)]
    pub escrow: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}
#[derive(Accounts)]
pub struct ManageCampaign<'info> {
    pub manager: Signer<'info>,
    #[account(seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump, has_one = manager @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"campaign", vault.key().as_ref(), campaign.id.as_ref()], bump = campaign.bump, has_one = vault)]
    pub campaign: Box<Account<'info, Campaign>>,
}
#[derive(Accounts)]
pub struct AdvanceEpoch<'info> {
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"campaign", vault.key().as_ref(), campaign.id.as_ref()], bump = campaign.bump, has_one = vault)]
    pub campaign: Box<Account<'info, Campaign>>,
}
#[derive(Accounts)]
#[instruction(award: Award)]
pub struct AwardSol<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = campaign.rules.verifier @ RouterError::InvalidAuthority)]
    pub verifier: Signer<'info>,
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"campaign", vault.key().as_ref(), campaign.id.as_ref()], bump = campaign.bump, has_one = vault)]
    pub campaign: Box<Account<'info, Campaign>>,
    #[account(mut)]
    pub recipient: SystemAccount<'info>,
    #[account(init_if_needed, payer = payer, space = 8 + Participant::INIT_SPACE, seeds = [b"participant", campaign.key().as_ref(), award.user_id.as_ref()], bump)]
    pub participant: Box<Account<'info, Participant>>,
    #[account(init, payer = payer, space = 8 + RewardReceipt::INIT_SPACE, seeds = [b"reward", campaign.key().as_ref(), award.post_id.as_ref()], bump)]
    pub receipt: Box<Account<'info, RewardReceipt>>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
#[instruction(award: Award)]
pub struct AwardTokens<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(address = campaign.rules.verifier @ RouterError::InvalidAuthority)]
    pub verifier: Signer<'info>,
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"campaign", vault.key().as_ref(), campaign.id.as_ref()], bump = campaign.bump, has_one = vault)]
    pub campaign: Box<Account<'info, Campaign>>,
    pub recipient: SystemAccount<'info>,
    #[account(init_if_needed, payer = payer, space = 8 + Participant::INIT_SPACE, seeds = [b"participant", campaign.key().as_ref(), award.user_id.as_ref()], bump)]
    pub participant: Box<Account<'info, Participant>>,
    #[account(init, payer = payer, space = 8 + RewardReceipt::INIT_SPACE, seeds = [b"reward", campaign.key().as_ref(), award.post_id.as_ref()], bump)]
    pub receipt: Box<Account<'info, RewardReceipt>>,
    #[account(address = campaign.rules.reward_mint @ RouterError::InvalidMint)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut, seeds = [b"campaign_tokens", campaign.key().as_ref()], bump, token::mint = mint, token::authority = campaign)]
    pub escrow: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = mint, token::authority = recipient)]
    pub recipient_tokens: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct CloseCampaignSol<'info> {
    pub actor: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"campaign", vault.key().as_ref(), campaign.id.as_ref()], bump = campaign.bump, has_one = vault)]
    pub campaign: Box<Account<'info, Campaign>>,
}
#[derive(Accounts)]
pub struct CloseCampaignTokens<'info> {
    pub actor: Signer<'info>,
    #[account(seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut, seeds = [b"campaign", vault.key().as_ref(), campaign.id.as_ref()], bump = campaign.bump, has_one = vault)]
    pub campaign: Box<Account<'info, Campaign>>,
    #[account(address = campaign.rules.reward_mint @ RouterError::InvalidMint)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut, seeds = [b"token_bank", vault.key().as_ref(), mint.key().as_ref()], bump = bank.bump, has_one = vault, has_one = mint)]
    pub bank: Box<Account<'info, TokenBank>>,
    #[account(mut, seeds = [b"bank_tokens", bank.key().as_ref()], bump, token::mint = mint, token::authority = vault)]
    pub bank_tokens: Box<Account<'info, TokenAccount>>,
    #[account(mut, seeds = [b"campaign_tokens", campaign.key().as_ref()], bump, token::mint = mint, token::authority = campaign)]
    pub escrow: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}
