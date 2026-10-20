use crate::{accounting::*, policy::split_lamports, *};
use anchor_lang::system_program::{self, Transfer};
use anchor_spl::token_interface::Mint;

pub(crate) fn initialize(ctx: Context<InitializeRouter>, reward_mint: Pubkey) -> Result<()> {
    require!(reward_mint != Pubkey::default(), RouterError::InvalidMint);
    let r = &mut ctx.accounts.router;
    r.authority = ctx.accounts.authority.key();
    r.activation_treasury = ctx.accounts.activation_treasury.key();
    r.reward_mint = reward_mint;
    r.activation_fee_lamports = DEFAULT_ACTIVATION_FEE_LAMPORTS;
    r.bump = ctx.bumps.router;
    Ok(())
}
pub(crate) fn configure_router(
    ctx: Context<AdminRouter>,
    treasury: Pubkey,
    fee: u64,
    paused: bool,
) -> Result<()> {
    require!(treasury != Pubkey::default(), RouterError::InvalidRecipient);
    ctx.accounts.router.activation_treasury = treasury;
    ctx.accounts.router.activation_fee_lamports = fee;
    ctx.accounts.router.paused = paused;
    Ok(())
}
pub(crate) fn register(ctx: Context<RegisterVault>, policy: AllocationPolicy) -> Result<()> {
    require!(!ctx.accounts.router.paused, RouterError::Paused);
    policy.validate()?;
    let fee = ctx.accounts.router.activation_fee_lamports;
    if fee > 0 {
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.manager.to_account_info(),
                    to: ctx.accounts.activation_treasury.to_account_info(),
                },
            ),
            fee,
        )?;
    }
    let v = &mut ctx.accounts.vault;
    v.mint = ctx.accounts.mint.key();
    v.manager = ctx.accounts.manager.key();
    v.policy = policy;
    v.rules_version = 1;
    v.bump = ctx.bumps.vault;
    emit!(VaultRegistered {
        vault: v.key(),
        mint: v.mint,
        manager: v.manager,
        activation_fee_lamports: fee
    });
    Ok(())
}
pub(crate) fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(amount > 0, RouterError::InvalidAmount);
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.contributor.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        ),
        amount,
    )?;
    emit!(VaultDeposit {
        vault: ctx.accounts.vault.key(),
        contributor: ctx.accounts.contributor.key(),
        amount
    });
    Ok(())
}
pub(crate) fn allocate(
    ctx: Context<Allocate>,
    sequence: u64,
    version: u64,
    minimum: u64,
) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    let v = &mut ctx.accounts.vault;
    require_eq!(sequence, v.route_sequence, RouterError::StaleSequence);
    require_eq!(version, v.rules_version, RouterError::StalePolicy);
    let amount = sub(spendable(&v.to_account_info())?, total(&v.balances)?)?;
    require!(amount > 0, RouterError::InvalidAmount);
    require!(
        amount >= minimum && amount >= v.policy.minimum_route_lamports,
        RouterError::BelowThreshold
    );
    let allocations = split_lamports(amount, v.policy.share_bps)?;
    for (balance, allocation) in v.balances.iter_mut().zip(allocations) {
        *balance = add(*balance, allocation)?;
    }
    v.route_sequence = add(v.route_sequence, 1)?;
    v.total_allocated_lamports = add(v.total_allocated_lamports, amount)?;
    emit!(FundsAllocated {
        vault: v.key(),
        sequence,
        rules_version: v.rules_version,
        amount,
        allocations
    });
    Ok(())
}
pub(crate) fn withdraw_reserve(ctx: Context<WithdrawReserve>, amount: u64) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    debit(&mut ctx.accounts.vault.balances, RESERVE as u8, amount)?;
    transfer_lamports(
        &ctx.accounts.vault.to_account_info(),
        &ctx.accounts.recipient.to_account_info(),
        amount,
    )
}
pub(crate) fn live(router: &RouterConfig, vault: &DistributionVault) -> Result<()> {
    require!(!router.paused && !vault.paused, RouterError::Paused);
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeRouter<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(constraint = program.programdata_address()? == Some(program_data.key()) @ RouterError::InvalidAuthority)]
    pub program: Program<'info, crate::program::DistributionRouter>,
    #[account(constraint = program_data.upgrade_authority_address == Some(authority.key()) @ RouterError::InvalidAuthority)]
    pub program_data: Box<Account<'info, ProgramData>>,
    pub activation_treasury: SystemAccount<'info>,
    #[account(init, payer = authority, space = 8 + RouterConfig::INIT_SPACE, seeds = [ROUTER_SEED], bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct AdminRouter<'info> {
    pub authority: Signer<'info>,
    #[account(mut, seeds = [ROUTER_SEED], bump = router.bump, has_one = authority @ RouterError::InvalidAuthority)]
    pub router: Box<Account<'info, RouterConfig>>,
}
#[derive(Accounts)]
pub struct RegisterVault<'info> {
    pub authority: Signer<'info>,
    #[account(seeds = [ROUTER_SEED], bump = router.bump, has_one = authority @ RouterError::InvalidAuthority)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(mut)]
    pub manager: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut, address = router.activation_treasury @ RouterError::InvalidRecipient, constraint = activation_treasury.key() != manager.key() @ RouterError::InvalidRecipient)]
    pub activation_treasury: SystemAccount<'info>,
    #[account(init, payer = manager, space = 8 + DistributionVault::INIT_SPACE, seeds = [VAULT_SEED, mint.key().as_ref()], bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub contributor: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct Allocate<'info> {
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
}
#[derive(Accounts)]
pub struct ManageVault<'info> {
    pub manager: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump, has_one = manager @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
}
#[derive(Accounts)]
pub struct WithdrawReserve<'info> {
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    pub manager: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump, has_one = manager @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(mut)]
    pub recipient: SystemAccount<'info>,
}
#[derive(Accounts)]
pub struct AcceptManager<'info> {
    pub next_manager: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump, constraint = vault.pending_manager == next_manager.key() @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
}
#[derive(Accounts)]
pub struct AcceptAuthority<'info> {
    pub next_authority: Signer<'info>,
    #[account(mut, seeds = [ROUTER_SEED], bump = router.bump, constraint = router.pending_authority == next_authority.key() @ RouterError::InvalidAuthority)]
    pub router: Box<Account<'info, RouterConfig>>,
}
