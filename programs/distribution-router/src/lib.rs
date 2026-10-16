use anchor_lang::prelude::*;
use anchor_lang::system_program::{self, Transfer};
use anchor_spl::token_interface::Mint;

mod policy;
use policy::split_lamports;

// Local development ID; replace with your generated deployment ID before release.
declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkgMQHGJkY4KT");

pub const ROUTER_SEED: &[u8] = b"distribution_router";
pub const VAULT_SEED: &[u8] = b"distribution_vault";
pub const BUCKETS: usize = 4;
pub const DEFAULT_ACTIVATION_FEE_LAMPORTS: u64 = 200_000_000;

#[program]
pub mod distribution_router {
    use super::*;

    /// The deployed program's upgrade authority bootstraps the registrar.
    /// A random wallet cannot claim the global config before the operator.
    pub fn initialize_router(ctx: Context<InitializeRouter>) -> Result<()> {
        let router = &mut ctx.accounts.router;
        router.authority = ctx.accounts.authority.key();
        router.pending_authority = Pubkey::default();
        router.activation_treasury = ctx.accounts.activation_treasury.key();
        router.activation_fee_lamports = DEFAULT_ACTIVATION_FEE_LAMPORTS;
        router.paused = false;
        router.bump = ctx.bumps.router;
        Ok(())
    }

    pub fn configure_router(
        ctx: Context<AdminRouter>,
        activation_treasury: Pubkey,
        activation_fee_lamports: u64,
        paused: bool,
    ) -> Result<()> {
        require!(
            activation_treasury != Pubkey::default(),
            RouterError::InvalidRecipient
        );
        let router = &mut ctx.accounts.router;
        router.activation_treasury = activation_treasury;
        router.activation_fee_lamports = activation_fee_lamports;
        router.paused = paused;
        Ok(())
    }

    /// Verification of creator/CTO identity happens off-chain. Registration
    /// requires both that approved manager and the configured registrar to sign.
    pub fn register_vault(ctx: Context<RegisterVault>, policy: AllocationPolicy) -> Result<()> {
        require!(!ctx.accounts.router.paused, RouterError::Paused);
        policy.validate(ctx.accounts.vault.key())?;
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
        let vault = &mut ctx.accounts.vault;
        vault.mint = ctx.accounts.mint.key();
        vault.manager = ctx.accounts.manager.key();
        vault.pending_manager = Pubkey::default();
        vault.policy = policy;
        vault.rules_version = 1;
        vault.route_sequence = 0;
        vault.total_routed_lamports = 0;
        vault.paused = false;
        vault.bump = ctx.bumps.vault;
        emit!(VaultRegistered {
            vault: vault.key(),
            mint: vault.mint,
            manager: vault.manager,
            activation_fee_lamports: fee,
        });
        Ok(())
    }

    /// Anyone may contribute. Plain SOL transfers to this same vault address
    /// also work, including permissionless Pump shareholder distributions.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
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
            amount,
        });
        Ok(())
    }

    /// Anybody can execute a batch, but every destination is fixed by the
    /// manager's policy. Expected sequence AND policy version prevent stale
    /// worker requests from routing new deposits or applying changed rules.
    pub fn route(
        ctx: Context<Route>,
        expected_sequence: u64,
        expected_rules_version: u64,
        minimum_amount: u64,
    ) -> Result<()> {
        let vault = &ctx.accounts.vault;
        require!(
            !ctx.accounts.router.paused && !vault.paused,
            RouterError::Paused
        );
        require_eq!(
            expected_sequence,
            vault.route_sequence,
            RouterError::StaleSequence
        );
        require_eq!(
            expected_rules_version,
            vault.rules_version,
            RouterError::StalePolicy
        );

        let rent = Rent::get()?.minimum_balance(vault.to_account_info().data_len());
        let amount = vault
            .get_lamports()
            .checked_sub(rent)
            .ok_or(RouterError::InsufficientFunds)?;
        require!(amount > 0, RouterError::InvalidAmount);
        require!(
            amount >= vault.policy.minimum_route_lamports && amount >= minimum_amount,
            RouterError::BelowThreshold
        );
        let allocations = split_lamports(amount, vault.policy.share_bps)?;
        let next_sequence = vault
            .route_sequence
            .checked_add(1)
            .ok_or(RouterError::Overflow)?;
        let total_routed = vault
            .total_routed_lamports
            .checked_add(amount)
            .ok_or(RouterError::Overflow)?;
        let destinations = [
            ctx.accounts.campaigns.to_account_info(),
            ctx.accounts.giveaways.to_account_info(),
            ctx.accounts.tips.to_account_info(),
            ctx.accounts.reserve.to_account_info(),
        ];
        for (recipient, allocation) in destinations.iter().zip(allocations) {
            if allocation > 0 {
                vault.sub_lamports(allocation)?;
                recipient.add_lamports(allocation)?;
            }
        }
        let vault = &mut ctx.accounts.vault;
        let sequence = vault.route_sequence;
        vault.route_sequence = next_sequence;
        vault.total_routed_lamports = total_routed;
        emit!(FundsRouted {
            vault: vault.key(),
            mint: vault.mint,
            sequence,
            rules_version: vault.rules_version,
            amount,
            allocations,
            recipients: vault.policy.recipients,
        });
        Ok(())
    }

    pub fn configure_vault(ctx: Context<ManageVault>, policy: AllocationPolicy) -> Result<()> {
        policy.validate(ctx.accounts.vault.key())?;
        let vault = &mut ctx.accounts.vault;
        vault.rules_version = vault
            .rules_version
            .checked_add(1)
            .ok_or(RouterError::Overflow)?;
        vault.policy = policy;
        emit!(PolicyChanged {
            vault: vault.key(),
            rules_version: vault.rules_version,
            policy,
        });
        Ok(())
    }

    pub fn pause_vault(ctx: Context<ManageVault>, paused: bool) -> Result<()> {
        ctx.accounts.vault.paused = paused;
        Ok(())
    }

    pub fn propose_manager(ctx: Context<ManageVault>, next_manager: Pubkey) -> Result<()> {
        require!(
            next_manager != Pubkey::default(),
            RouterError::InvalidAuthority
        );
        ctx.accounts.vault.pending_manager = next_manager;
        Ok(())
    }

    pub fn accept_manager(ctx: Context<AcceptManager>) -> Result<()> {
        ctx.accounts.vault.manager = ctx.accounts.next_manager.key();
        ctx.accounts.vault.pending_manager = Pubkey::default();
        Ok(())
    }

    pub fn propose_authority(ctx: Context<AdminRouter>, next_authority: Pubkey) -> Result<()> {
        require!(
            next_authority != Pubkey::default(),
            RouterError::InvalidAuthority
        );
        ctx.accounts.router.pending_authority = next_authority;
        Ok(())
    }

    pub fn accept_authority(ctx: Context<AcceptAuthority>) -> Result<()> {
        ctx.accounts.router.authority = ctx.accounts.next_authority.key();
        ctx.accounts.router.pending_authority = Pubkey::default();
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeRouter<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(constraint = program.programdata_address()? == Some(program_data.key()) @ RouterError::InvalidAuthority)]
    pub program: Program<'info, crate::program::DistributionRouter>,
    #[account(constraint = program_data.upgrade_authority_address == Some(authority.key()) @ RouterError::InvalidAuthority)]
    pub program_data: Account<'info, ProgramData>,
    pub activation_treasury: SystemAccount<'info>,
    #[account(init, payer = authority, space = 8 + RouterConfig::INIT_SPACE, seeds = [ROUTER_SEED], bump)]
    pub router: Account<'info, RouterConfig>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminRouter<'info> {
    pub authority: Signer<'info>,
    #[account(mut, seeds = [ROUTER_SEED], bump = router.bump, has_one = authority @ RouterError::InvalidAuthority)]
    pub router: Account<'info, RouterConfig>,
}

#[derive(Accounts)]
pub struct RegisterVault<'info> {
    pub authority: Signer<'info>,
    #[account(seeds = [ROUTER_SEED], bump = router.bump, has_one = authority @ RouterError::InvalidAuthority)]
    pub router: Account<'info, RouterConfig>,
    #[account(mut)]
    pub manager: Signer<'info>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut, address = router.activation_treasury @ RouterError::InvalidRecipient, constraint = activation_treasury.key() != manager.key() @ RouterError::InvalidRecipient)]
    pub activation_treasury: SystemAccount<'info>,
    #[account(init, payer = manager, space = 8 + DistributionVault::INIT_SPACE, seeds = [VAULT_SEED, mint.key().as_ref()], bump)]
    pub vault: Account<'info, DistributionVault>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub contributor: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Account<'info, DistributionVault>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Route<'info> {
    #[account(seeds = [ROUTER_SEED], bump = router.bump)]
    pub router: Account<'info, RouterConfig>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump)]
    pub vault: Account<'info, DistributionVault>,
    /// CHECK: Fixed policy address; receives lamports only. Never used for a CPI.
    #[account(mut, address = vault.policy.recipients[0] @ RouterError::InvalidRecipient)]
    pub campaigns: UncheckedAccount<'info>,
    /// CHECK: Fixed policy address; receives lamports only. Never used for a CPI.
    #[account(mut, address = vault.policy.recipients[1] @ RouterError::InvalidRecipient)]
    pub giveaways: UncheckedAccount<'info>,
    /// CHECK: Fixed policy address; receives lamports only. Never used for a CPI.
    #[account(mut, address = vault.policy.recipients[2] @ RouterError::InvalidRecipient)]
    pub tips: UncheckedAccount<'info>,
    /// CHECK: Fixed policy address; receives lamports only. Never used for a CPI.
    #[account(mut, address = vault.policy.recipients[3] @ RouterError::InvalidRecipient)]
    pub reserve: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct ManageVault<'info> {
    pub manager: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump, has_one = manager @ RouterError::InvalidAuthority)]
    pub vault: Account<'info, DistributionVault>,
}

#[derive(Accounts)]
pub struct AcceptManager<'info> {
    pub next_manager: Signer<'info>,
    #[account(mut, seeds = [VAULT_SEED, vault.mint.as_ref()], bump = vault.bump, constraint = vault.pending_manager == next_manager.key() @ RouterError::InvalidAuthority)]
    pub vault: Account<'info, DistributionVault>,
}

#[derive(Accounts)]
pub struct AcceptAuthority<'info> {
    pub next_authority: Signer<'info>,
    #[account(mut, seeds = [ROUTER_SEED], bump = router.bump, constraint = router.pending_authority == next_authority.key() @ RouterError::InvalidAuthority)]
    pub router: Account<'info, RouterConfig>,
}

#[account]
#[derive(InitSpace)]
pub struct RouterConfig {
    pub authority: Pubkey,
    pub pending_authority: Pubkey,
    pub activation_treasury: Pubkey,
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
    pub rules_version: u64,
    pub route_sequence: u64,
    pub total_routed_lamports: u64,
    pub paused: bool,
    pub bump: u8,
}

/// Bucket order: campaigns, giveaways, tips, reserve.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, InitSpace)]
pub struct AllocationPolicy {
    pub recipients: [Pubkey; BUCKETS],
    pub share_bps: [u16; BUCKETS],
    pub minimum_route_lamports: u64,
}

impl AllocationPolicy {
    pub fn validate(&self, vault: Pubkey) -> Result<()> {
        require!(self.minimum_route_lamports > 0, RouterError::InvalidAmount);
        require_eq!(
            self.share_bps
                .iter()
                .map(|&bps| u32::from(bps))
                .sum::<u32>(),
            10_000,
            RouterError::InvalidShares
        );
        for (index, recipient) in self.recipients.iter().enumerate() {
            require!(
                *recipient != Pubkey::default() && *recipient != vault,
                RouterError::InvalidRecipient
            );
            require!(
                !self.recipients[..index].contains(recipient),
                RouterError::DuplicateRecipient
            );
        }
        Ok(())
    }
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
pub struct FundsRouted {
    pub vault: Pubkey,
    pub mint: Pubkey,
    pub sequence: u64,
    pub rules_version: u64,
    pub amount: u64,
    pub allocations: [u64; BUCKETS],
    pub recipients: [Pubkey; BUCKETS],
}

#[event]
pub struct PolicyChanged {
    pub vault: Pubkey,
    pub rules_version: u64,
    pub policy: AllocationPolicy,
}

#[error_code]
pub enum RouterError {
    #[msg("The required manager or registrar did not authorize this instruction")]
    InvalidAuthority,
    #[msg("The router or vault is paused")]
    Paused,
    #[msg("Shares must total exactly 10000 basis points")]
    InvalidShares,
    #[msg("Recipient does not match the policy or is an invalid address")]
    InvalidRecipient,
    #[msg("Each allocation bucket needs a distinct recipient")]
    DuplicateRecipient,
    #[msg("Amount must be positive")]
    InvalidAmount,
    #[msg("The vault cannot cover its rent exemption")]
    InsufficientFunds,
    #[msg("Available SOL is below the distribution threshold")]
    BelowThreshold,
    #[msg("A batch with this sequence has already executed")]
    StaleSequence,
    #[msg("The manager changed the allocation policy; refresh before routing")]
    StalePolicy,
    #[msg("Integer arithmetic overflow")]
    Overflow,
}
