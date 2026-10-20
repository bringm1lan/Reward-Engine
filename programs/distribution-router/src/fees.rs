//! SOL-only adapters for the published Pump and Raydium LaunchLab ABIs.
//! Every CPI opcode, program, mint, source PDA and recipient is constrained here.
use crate::{accounting::*, vault::live, *};
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke,
};
use anchor_lang::system_program::{self, Transfer};
use anchor_spl::token::{self, CloseAccount, Mint, Token, TokenAccount, TransferChecked};

pub const PUMP: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
pub const PUMP_FEES: Pubkey = pubkey!("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");
pub const PUMP_AMM: Pubkey = pubkey!("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
pub const LAUNCHLAB: Pubkey = pubkey!("LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj");
pub const STONK_PLATFORM: Pubkey = pubkey!("AvVCE7Ue49iZjYzkkHz6ZhVyvY6NLHw67vB8eQaffVPz");
pub const WSOL: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
pub const TOKEN: Pubkey = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const ASSOCIATED: Pubkey = pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const CURVE_DISC: [u8; 8] = [23, 183, 248, 55, 96, 216, 172, 96];
const SHARING_DISC: [u8; 8] = [216, 74, 9, 0, 56, 140, 93, 75];

fn pda(program: &Pubkey, seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, program).0
}
fn ata(owner: &Pubkey) -> Pubkey {
    pda(
        &ASSOCIATED,
        &[owner.as_ref(), TOKEN.as_ref(), WSOL.as_ref()],
    )
}
fn event(program: &Pubkey) -> Pubkey {
    pda(program, &[b"__event_authority"])
}
fn curve_address(mint: &Pubkey) -> Pubkey {
    pda(&PUMP, &[b"bonding-curve", mint.as_ref()])
}
fn sharing_address(mint: &Pubkey) -> Pubkey {
    pda(&PUMP_FEES, &[b"sharing-config", mint.as_ref()])
}
fn pool_address(mint: &Pubkey) -> Pubkey {
    let authority = pda(&PUMP, &[b"pool-authority", mint.as_ref()]);
    pda(
        &PUMP_AMM,
        &[
            b"pool",
            &0u16.to_le_bytes(),
            authority.as_ref(),
            mint.as_ref(),
            WSOL.as_ref(),
        ],
    )
}
fn key(data: &[u8], offset: usize) -> Result<Pubkey> {
    Ok(Pubkey::new_from_array(
        data.get(offset..offset + 32)
            .ok_or(RouterError::InvalidFeeSource)?
            .try_into()
            .map_err(|_| RouterError::InvalidFeeSource)?,
    ))
}
fn matches(accounts: &[AccountInfo], expected: &[(usize, Pubkey)]) -> Result<()> {
    for (index, address) in expected {
        require_keys_eq!(
            accounts
                .get(*index)
                .ok_or(RouterError::InvalidFeeSource)?
                .key(),
            *address,
            RouterError::InvalidFeeSource
        );
    }
    Ok(())
}
fn curve_creator(account: &AccountInfo, mint: &Pubkey) -> Result<Pubkey> {
    require_keys_eq!(
        account.key(),
        curve_address(mint),
        RouterError::InvalidFeeSource
    );
    require_keys_eq!(*account.owner, PUMP, RouterError::InvalidFeeSource);
    let d = account.try_borrow_data()?;
    require!(
        d.len() >= 81 && d[..8] == CURVE_DISC,
        RouterError::InvalidFeeSource
    );
    if d.len() >= 115 {
        let quote = key(&d, 83)?;
        require!(
            quote == WSOL || quote == Pubkey::default(),
            RouterError::UnsupportedQuote
        );
    }
    if d.len() >= 125 {
        require!(d[124] == 0, RouterError::UnsupportedQuote);
    }
    key(&d, 49)
}
struct Sharing {
    admin: Pubkey,
    revoked: bool,
    shares: Vec<(Pubkey, u16)>,
}
fn sharing(account: &AccountInfo, mint: &Pubkey) -> Result<Sharing> {
    require_keys_eq!(
        account.key(),
        sharing_address(mint),
        RouterError::InvalidFeeSource
    );
    require_keys_eq!(*account.owner, PUMP_FEES, RouterError::InvalidFeeSource);
    let d = account.try_borrow_data()?;
    require!(
        d.len() >= 80 && d[..8] == SHARING_DISC && d[10] == 1 && key(&d, 11)? == *mint,
        RouterError::InvalidFeeSource
    );
    let count = u32::from_le_bytes(
        d[76..80]
            .try_into()
            .map_err(|_| RouterError::InvalidFeeSource)?,
    ) as usize;
    require!(
        count > 0 && count <= 10 && d.len() >= 80 + count * 34,
        RouterError::InvalidFeeSource
    );
    let mut shares = Vec::with_capacity(count);
    let mut total = 0u32;
    for i in 0..count {
        let address = key(&d, 80 + i * 34)?;
        let n = u16::from_le_bytes([d[112 + i * 34], d[113 + i * 34]]);
        require!(
            n > 0 && !shares.iter().any(|&(a, _)| a == address),
            RouterError::InvalidShares
        );
        total = total
            .checked_add(u32::from(n))
            .ok_or(RouterError::Overflow)?;
        shares.push((address, n));
    }
    require_eq!(total, 10000, RouterError::InvalidShares);
    Ok(Sharing {
        admin: key(&d, 43)?,
        revoked: d[75] != 0,
        shares,
    })
}
fn verified_pump(
    curve: &AccountInfo,
    config: &AccountInfo,
    mint: &Pubkey,
    vault: Pubkey,
) -> Result<u16> {
    let s = sharing(config, mint)?;
    require!(s.revoked, RouterError::FeeAuthorityLocked);
    require_keys_eq!(
        curve_creator(curve, mint)?,
        config.key(),
        RouterError::InvalidFeeSource
    );
    s.shares
        .iter()
        .find(|&&(address, _)| address == vault)
        .map(|&(_, bps)| bps)
        .ok_or_else(|| error!(RouterError::InvalidFeeSource))
}
fn call<'info>(
    program: &AccountInfo<'info>,
    accounts: &[AccountInfo<'info>],
    writable: &[usize],
    signers: &[usize],
    data: Vec<u8>,
) -> Result<()> {
    require!(program.executable, RouterError::InvalidFeeSource);
    let metas = accounts
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if writable.contains(&i) {
                AccountMeta::new(a.key(), signers.contains(&i))
            } else {
                AccountMeta::new_readonly(a.key(), signers.contains(&i))
            }
        })
        .collect();
    let mut infos = accounts.to_vec();
    infos.push(program.clone());
    invoke(
        &Instruction {
            program_id: program.key(),
            accounts: metas,
            data,
        },
        &infos,
    )
    .map_err(Into::into)
}
fn connected(
    v: &mut Account<DistributionVault>,
    source: Pubkey,
    creator: Pubkey,
    bps: u16,
    kind: u8,
) {
    v.fee_source = source;
    v.fee_creator = creator;
    v.fee_share_bps = bps;
    v.fee_kind = kind;
}
pub(crate) fn create_pump<'info>(ctx: Context<'_, '_, '_, 'info, FeeSetup<'info>>) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    require_keys_eq!(
        ctx.accounts.external_program.key(),
        PUMP_FEES,
        RouterError::InvalidFeeSource
    );
    let a = ctx.remaining_accounts;
    require_eq!(a.len(), 13, RouterError::InvalidFeeSource);
    let mint = ctx.accounts.vault.mint;
    let creator = ctx.accounts.creator.key();
    matches(
        a,
        &[
            (0, event(&PUMP_FEES)),
            (1, PUMP_FEES),
            (2, creator),
            (3, pda(&PUMP, &[b"global"])),
            (4, mint),
            (5, sharing_address(&mint)),
            (6, system_program::ID),
            (7, curve_address(&mint)),
            (8, PUMP),
            (9, event(&PUMP)),
        ],
    )?;
    require_keys_eq!(
        curve_creator(&a[7], &mint)?,
        creator,
        RouterError::InvalidAuthority
    );
    let mut writable = vec![2, 5, 7];
    if a[10].key() == PUMP_FEES {
        matches(a, &[(11, PUMP_FEES), (12, PUMP_FEES)])?;
    } else {
        matches(
            a,
            &[
                (10, pool_address(&mint)),
                (11, PUMP_AMM),
                (12, event(&PUMP_AMM)),
            ],
        )?;
        writable.push(10);
    }
    call(
        &ctx.accounts.external_program.to_account_info(),
        a,
        &writable,
        &[2],
        vec![195, 78, 86, 76, 111, 52, 251, 213],
    )
}
pub(crate) fn finalize_pump<'info>(
    ctx: Context<'_, '_, '_, 'info, FeeSetup<'info>>,
    bps: u16,
) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    require!(bps > 0 && bps <= 10000, RouterError::InvalidShares);
    require_keys_eq!(
        ctx.accounts.external_program.key(),
        PUMP_FEES,
        RouterError::InvalidFeeSource
    );
    let a = ctx.remaining_accounts;
    require_eq!(a.len(), 20, RouterError::InvalidFeeSource);
    let mint = ctx.accounts.vault.mint;
    let creator = ctx.accounts.creator.key();
    let config = sharing_address(&mint);
    let cv = pda(&PUMP, &[b"creator-vault", config.as_ref()]);
    let av = pda(&PUMP_AMM, &[b"creator_vault", config.as_ref()]);
    matches(
        a,
        &[
            (0, event(&PUMP_FEES)),
            (1, PUMP_FEES),
            (2, creator),
            (3, pda(&PUMP, &[b"global"])),
            (4, mint),
            (5, config),
            (6, curve_address(&mint)),
            (7, cv),
            (8, ata(&cv)),
            (9, system_program::ID),
            (10, PUMP),
            (11, event(&PUMP)),
            (12, PUMP_AMM),
            (13, event(&PUMP_AMM)),
            (14, WSOL),
            (15, TOKEN),
            (16, ASSOCIATED),
            (17, av),
            (18, ata(&av)),
            (19, creator),
        ],
    )?;
    let current = sharing(&a[5], &mint)?;
    require!(
        !current.revoked && current.admin == creator && current.shares == vec![(creator, 10000)],
        RouterError::FeeAuthorityLocked
    );
    require_keys_eq!(
        curve_creator(&a[6], &mint)?,
        config,
        RouterError::InvalidFeeSource
    );
    let mut next = Vec::new();
    if bps < 10000 {
        next.push((creator, 10000 - bps));
    }
    next.push((ctx.accounts.vault.key(), bps));
    let mut data = vec![111, 251, 49, 6, 78, 78, 106, 18];
    data.extend_from_slice(&(next.len() as u32).to_le_bytes());
    for (address, n) in next {
        data.extend_from_slice(address.as_ref());
        data.extend_from_slice(&n.to_le_bytes());
    }
    call(
        &ctx.accounts.external_program.to_account_info(),
        a,
        &[2, 5, 7, 8, 17, 18, 19],
        &[2],
        data,
    )?;
    require_eq!(
        verified_pump(&a[6], &a[5], &mint, ctx.accounts.vault.key())?,
        bps,
        RouterError::InvalidShares
    );
    connected(&mut ctx.accounts.vault, config, creator, bps, 1);
    Ok(())
}
pub(crate) fn connect_pump(ctx: Context<ConnectPump>) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    let bps = verified_pump(
        &ctx.accounts.curve,
        &ctx.accounts.sharing,
        &ctx.accounts.vault.mint,
        ctx.accounts.vault.key(),
    )?;
    connected(
        &mut ctx.accounts.vault,
        ctx.accounts.sharing.key(),
        Pubkey::default(),
        bps,
        1,
    );
    Ok(())
}
pub(crate) fn sweep_pump<'info>(ctx: Context<'_, '_, '_, 'info, CollectPump<'info>>) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    require_eq!(
        ctx.accounts.vault.fee_kind,
        1,
        RouterError::InvalidFeeSource
    );
    require_keys_eq!(
        ctx.accounts.external_program.key(),
        PUMP_AMM,
        RouterError::InvalidFeeSource
    );
    let a = ctx.remaining_accounts;
    require_eq!(a.len(), 12, RouterError::InvalidFeeSource);
    let config = ctx.accounts.vault.fee_source;
    let av = pda(&PUMP_AMM, &[b"creator_vault", config.as_ref()]);
    let cv = pda(&PUMP, &[b"creator-vault", config.as_ref()]);
    matches(
        a,
        &[
            (0, ctx.accounts.payer.key()),
            (1, WSOL),
            (2, TOKEN),
            (3, system_program::ID),
            (4, ASSOCIATED),
            (5, config),
            (6, av),
            (7, ata(&av)),
            (8, cv),
            (9, ata(&cv)),
            (10, event(&PUMP_AMM)),
            (11, PUMP_AMM),
        ],
    )?;
    call(
        &ctx.accounts.external_program.to_account_info(),
        a,
        &[0, 6, 7, 8, 9],
        &[0],
        vec![1, 33, 78, 185, 33, 67, 44, 92],
    )
}
pub(crate) fn collect_pump<'info>(
    ctx: Context<'_, '_, '_, 'info, CollectPump<'info>>,
    minimum: u64,
) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    require!(
        minimum > 0 && ctx.accounts.vault.fee_kind == 1,
        RouterError::InvalidFeeSource
    );
    require_keys_eq!(
        ctx.accounts.external_program.key(),
        PUMP,
        RouterError::InvalidFeeSource
    );
    let a = ctx.remaining_accounts;
    require!(
        a.len() >= 13 && a.len() <= 22,
        RouterError::InvalidFeeSource
    );
    let mint = ctx.accounts.vault.mint;
    let config = ctx.accounts.vault.fee_source;
    let cv = pda(&PUMP, &[b"creator-vault", config.as_ref()]);
    matches(
        a,
        &[
            (0, ctx.accounts.payer.key()),
            (1, mint),
            (2, curve_address(&mint)),
            (3, config),
            (4, cv),
            (5, system_program::ID),
            (6, event(&PUMP)),
            (7, PUMP),
            (8, ata(&cv)),
            (9, WSOL),
            (10, TOKEN),
            (11, ASSOCIATED),
        ],
    )?;
    verified_pump(&a[2], &a[3], &mint, ctx.accounts.vault.key())?;
    let s = sharing(&a[3], &mint)?;
    require_eq!(a.len(), 12 + s.shares.len(), RouterError::InvalidFeeSource);
    for (i, (address, _)) in s.shares.iter().enumerate() {
        matches(a, &[(12 + i, *address)])?;
    }
    let before = ctx.accounts.vault.get_lamports();
    let mut writable = vec![0, 4, 8];
    writable.extend(12..a.len());
    call(
        &ctx.accounts.external_program.to_account_info(),
        a,
        &writable,
        &[0],
        vec![255, 203, 19, 79, 244, 68, 8, 159, 0],
    )?;
    let amount = sub(ctx.accounts.vault.get_lamports(), before)?;
    require!(amount >= minimum, RouterError::BelowThreshold);
    ctx.accounts.vault.verified_creator_fees =
        add(ctx.accounts.vault.verified_creator_fees, amount)?;
    emit!(CreatorFeesCollected {
        vault: ctx.accounts.vault.key(),
        source: config,
        kind: 1,
        amount
    });
    Ok(())
}
pub(crate) fn sweep_buckets<'info>(
    ctx: Context<'_, '_, '_, 'info, SweepPumpBuckets<'info>>,
    amm: bool,
) -> Result<()> {
    let mint = ctx.accounts.vault.mint;
    let creator = curve_creator(&ctx.accounts.curve, &mint)?;
    let a = ctx.remaining_accounts;
    let program = if amm { PUMP_AMM } else { PUMP };
    require_keys_eq!(
        ctx.accounts.external_program.key(),
        program,
        RouterError::InvalidFeeSource
    );
    if amm {
        require_eq!(a.len(), 12, RouterError::InvalidFeeSource);
        let pool = pool_address(&mint);
        let recipient = pda(&PUMP_AMM, &[b"creator_vault", creator.as_ref()]);
        matches(
            a,
            &[
                (0, ctx.accounts.payer.key()),
                (1, pda(&PUMP_AMM, &[b"global_config"])),
                (2, pool),
                (3, WSOL),
                (4, TOKEN),
                (5, ata(&pool)),
                (6, recipient),
                (7, ata(&recipient)),
                (8, system_program::ID),
                (9, ASSOCIATED),
                (10, event(&PUMP_AMM)),
                (11, PUMP_AMM),
            ],
        )?;
        call(
            &ctx.accounts.external_program.to_account_info(),
            a,
            &[0, 2, 5, 7],
            &[0],
            vec![32, 246, 191, 52, 8, 201, 73, 186],
        )
    } else {
        require_eq!(a.len(), 13, RouterError::InvalidFeeSource);
        let curve = curve_address(&mint);
        let recipient = pda(&PUMP, &[b"creator-vault", creator.as_ref()]);
        matches(
            a,
            &[
                (0, ctx.accounts.payer.key()),
                (1, pda(&PUMP, &[b"global"])),
                (2, mint),
                (3, WSOL),
                (4, TOKEN),
                (5, ASSOCIATED),
                (6, system_program::ID),
                (7, curve),
                (8, ata(&curve)),
                (9, recipient),
                (10, ata(&recipient)),
                (11, event(&PUMP)),
                (12, PUMP),
            ],
        )?;
        call(
            &ctx.accounts.external_program.to_account_info(),
            a,
            &[0, 7, 8, 9, 10],
            &[0],
            vec![32, 246, 191, 52, 8, 201, 73, 186],
        )
    }
}
pub(crate) fn connect_launchlab(ctx: Context<ConnectLaunchLab>, bps: u16) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    require!(bps > 0 && bps <= 10000, RouterError::InvalidShares);
    require_keys_eq!(
        *ctx.accounts.pool.owner,
        LAUNCHLAB,
        RouterError::InvalidFeeSource
    );
    require_keys_eq!(
        *ctx.accounts.platform.owner,
        LAUNCHLAB,
        RouterError::InvalidFeeSource
    );
    let d = ctx.accounts.pool.try_borrow_data()?;
    let p = ctx.accounts.platform.try_borrow_data()?;
    require!(
        d.len() >= 365
            && d[..8] == [247, 237, 227, 245, 215, 195, 222, 70]
            && p.len() >= 48
            && p[..8] == [160, 78, 128, 0, 248, 83, 230, 160],
        RouterError::InvalidFeeSource
    );
    require_keys_eq!(
        key(&d, 205)?,
        ctx.accounts.vault.mint,
        RouterError::InvalidFeeSource
    );
    require_keys_eq!(key(&d, 237)?, WSOL, RouterError::UnsupportedQuote);
    require_keys_eq!(
        key(&d, 173)?,
        ctx.accounts.platform.key(),
        RouterError::InvalidFeeSource
    );
    require_keys_eq!(key(&p, 16)?, STONK_PLATFORM, RouterError::InvalidFeeSource);
    require_keys_eq!(
        key(&d, 333)?,
        ctx.accounts.creator.key(),
        RouterError::InvalidAuthority
    );
    connected(
        &mut ctx.accounts.vault,
        ctx.accounts.pool.key(),
        ctx.accounts.creator.key(),
        bps,
        2,
    );
    Ok(())
}
/// Existing LaunchLab creators explicitly allocate their own SOL. LaunchLab
/// aggregates claims by creator/quote, so this is never labeled a per-mint claim.
pub(crate) fn relay_launchlab(ctx: Context<RelayLaunchLab>, amount: u64) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    require!(
        amount > 0 && ctx.accounts.vault.fee_kind == 2,
        RouterError::InvalidAmount
    );
    require_keys_eq!(
        ctx.accounts.creator.key(),
        ctx.accounts.vault.fee_creator,
        RouterError::InvalidAuthority
    );
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.creator.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        ),
        amount,
    )?;
    emit!(CreatorFeesCollected {
        vault: ctx.accounts.vault.key(),
        source: ctx.accounts.vault.fee_source,
        kind: 2,
        amount
    });
    Ok(())
}
/// Claim a creator-wide WSOL balance with the actual creator's signature, then
/// atomically allocate the agreed share to this vault. Old WSOL in the creator's
/// ATA is excluded. Closing a fresh buffer unwraps only the allocated amount;
/// buffer rent is returned to its payer. This does not claim post-graduation CPMM
/// revenue or pretend LaunchLab's creator-wide balance belongs to a single mint.
pub(crate) fn claim_launchlab<'info>(
    ctx: Context<'_, '_, '_, 'info, ClaimLaunchLab<'info>>,
    minimum: u64,
) -> Result<()> {
    live(&ctx.accounts.router, &ctx.accounts.vault)?;
    require!(
        minimum > 0 && ctx.accounts.vault.fee_kind == 2,
        RouterError::InvalidFeeSource
    );
    require_keys_eq!(
        ctx.accounts.creator.key(),
        ctx.accounts.vault.fee_creator,
        RouterError::InvalidAuthority
    );
    let a = ctx.remaining_accounts;
    require_eq!(a.len(), 8, RouterError::InvalidFeeSource);
    let creator = ctx.accounts.creator.key();
    matches(
        a,
        &[
            (0, creator),
            (1, pda(&LAUNCHLAB, &[b"creator_fee_vault_auth_seed"])),
            (2, pda(&LAUNCHLAB, &[creator.as_ref(), WSOL.as_ref()])),
            (3, ata(&creator)),
            (4, WSOL),
            (5, TOKEN),
            (6, system_program::ID),
            (7, ASSOCIATED),
        ],
    )?;
    require_keys_eq!(
        ctx.accounts.creator_tokens.key(),
        a[3].key(),
        RouterError::InvalidFeeSource
    );
    let before = ctx.accounts.creator_tokens.amount;
    call(
        &ctx.accounts.external_program.to_account_info(),
        a,
        &[0, 2, 3],
        &[0],
        vec![26, 97, 138, 203, 132, 171, 141, 252],
    )?;
    ctx.accounts.creator_tokens.reload()?;
    let claimed = sub(ctx.accounts.creator_tokens.amount, before)?;
    let amount =
        u64::try_from(u128::from(claimed) * u128::from(ctx.accounts.vault.fee_share_bps) / 10000)
            .map_err(|_| RouterError::Overflow)?;
    require!(amount >= minimum, RouterError::BelowThreshold);
    token::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.creator_tokens.to_account_info(),
                mint: ctx.accounts.quote_mint.to_account_info(),
                to: ctx.accounts.buffer.to_account_info(),
                authority: ctx.accounts.creator.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.quote_mint.decimals,
    )?;
    let mint = ctx.accounts.vault.mint;
    let bump = [ctx.accounts.vault.bump];
    let seeds: &[&[u8]] = &[VAULT_SEED, mint.as_ref(), &bump];
    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        CloseAccount {
            account: ctx.accounts.buffer.to_account_info(),
            destination: ctx.accounts.payer.to_account_info(),
            authority: ctx.accounts.vault.to_account_info(),
        },
        &[seeds],
    ))?;
    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        ),
        amount,
    )?;
    emit!(CreatorFeesCollected {
        vault: ctx.accounts.vault.key(),
        source: ctx.accounts.vault.fee_source,
        kind: 3,
        amount
    });
    Ok(())
}

#[derive(Accounts)]
pub struct FeeSetup<'info> {
    #[account(seeds=[ROUTER_SEED],bump=router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    pub manager: Signer<'info>,
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut,seeds=[VAULT_SEED,vault.mint.as_ref()],bump=vault.bump,has_one=manager @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
    /// CHECK: The handler permits only the published Pump Fees program.
    #[account(executable)]
    pub external_program: UncheckedAccount<'info>,
}
#[derive(Accounts)]
pub struct ConnectPump<'info> {
    #[account(seeds=[ROUTER_SEED],bump=router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    pub manager: Signer<'info>,
    #[account(mut,seeds=[VAULT_SEED,vault.mint.as_ref()],bump=vault.bump,has_one=manager @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
    /// CHECK: Owner, discriminator, PDA, quote and creator checked by verified_pump.
    pub curve: UncheckedAccount<'info>,
    /// CHECK: Owner, PDA, mint, finalized shares and vault beneficiary checked.
    pub sharing: UncheckedAccount<'info>,
}
#[derive(Accounts)]
pub struct CollectPump<'info> {
    #[account(seeds=[ROUTER_SEED],bump=router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut,seeds=[VAULT_SEED,vault.mint.as_ref()],bump=vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    /// CHECK: Fixed program ID and permitted opcode checked in each handler.
    #[account(executable)]
    pub external_program: UncheckedAccount<'info>,
}
#[derive(Accounts)]
pub struct SweepPumpBuckets<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(seeds=[VAULT_SEED,vault.mint.as_ref()],bump=vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    /// CHECK: Verified Pump curve at this vault's mint PDA; read only.
    pub curve: UncheckedAccount<'info>,
    /// CHECK: Only fixed Pump/Pump AMM sweep instructions are allowed.
    #[account(executable)]
    pub external_program: UncheckedAccount<'info>,
}
#[derive(Accounts)]
pub struct ConnectLaunchLab<'info> {
    #[account(seeds=[ROUTER_SEED],bump=router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    pub manager: Signer<'info>,
    pub creator: Signer<'info>,
    #[account(mut,seeds=[VAULT_SEED,vault.mint.as_ref()],bump=vault.bump,has_one=manager @ RouterError::InvalidAuthority)]
    pub vault: Box<Account<'info, DistributionVault>>,
    /// CHECK: LaunchLab owner, discriminator, base/quote mint and creator checked.
    pub pool: UncheckedAccount<'info>,
    /// CHECK: LaunchLab owner, discriminator and Stonk platform wallet checked.
    pub platform: UncheckedAccount<'info>,
}
#[derive(Accounts)]
pub struct RelayLaunchLab<'info> {
    #[account(seeds=[ROUTER_SEED],bump=router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut,seeds=[VAULT_SEED,vault.mint.as_ref()],bump=vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct ClaimLaunchLab<'info> {
    #[account(seeds=[ROUTER_SEED],bump=router.bump)]
    pub router: Box<Account<'info, RouterConfig>>,
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut,seeds=[VAULT_SEED,vault.mint.as_ref()],bump=vault.bump)]
    pub vault: Box<Account<'info, DistributionVault>>,
    #[account(address=WSOL @ RouterError::UnsupportedQuote)]
    pub quote_mint: Box<Account<'info, Mint>>,
    #[account(mut,token::mint=quote_mint,token::authority=creator)]
    pub creator_tokens: Box<Account<'info, TokenAccount>>,
    #[account(init,payer=payer,token::mint=quote_mint,token::authority=vault,seeds=[b"claim_buffer",vault.key().as_ref()],bump)]
    pub buffer: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    /// CHECK: Canonical LaunchLab executable, not a caller-selected CPI target.
    #[account(address=LAUNCHLAB,executable)]
    pub external_program: UncheckedAccount<'info>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shares_reject_truncated_wrong_mint_and_duplicate_beneficiaries() {
        let mint = Pubkey::new_unique();
        let address = sharing_address(&mint);
        let owner = PUMP_FEES;
        let mut lamports = 1;
        let mut data = vec![0u8; 148];
        data[..8].copy_from_slice(&SHARING_DISC);
        data[10] = 1;
        data[11..43].copy_from_slice(mint.as_ref());
        data[75] = 1;
        data[76..80].copy_from_slice(&2u32.to_le_bytes());
        let wallet = Pubkey::new_unique();
        for i in 0..2 {
            data[80 + i * 34..112 + i * 34].copy_from_slice(wallet.as_ref());
            data[112 + i * 34..114 + i * 34].copy_from_slice(&5000u16.to_le_bytes());
        }
        let info = AccountInfo::new(
            &address,
            false,
            false,
            &mut lamports,
            &mut data,
            &owner,
            false,
            0,
        );
        assert!(sharing(&info, &mint).is_err());
        assert!(sharing(&info, &Pubkey::new_unique()).is_err());
    }
}
