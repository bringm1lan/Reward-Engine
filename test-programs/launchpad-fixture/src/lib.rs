//! Test double for launchpad CPI plumbing, NOT Pump/Raydium implementations.
//! IDs, data shapes and account orders match the inspected official IDLs; actual
//! production contracts must also be exercised on a fork before release.
use anchor_lang::solana_program::{
    account_info::AccountInfo,
    entrypoint,
    entrypoint::ProgramResult,
    instruction::{AccountMeta, Instruction},
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
use anchor_lang::system_program;
use spl_token::state::Account as TokenAccount;
entrypoint!(process);
const CREATE: [u8; 8] = [195, 78, 86, 76, 111, 52, 251, 213];
const FINALIZE: [u8; 8] = [111, 251, 49, 6, 78, 78, 106, 18];
const DISTRIBUTE: [u8; 8] = [255, 203, 19, 79, 244, 68, 8, 159];
const CLAIM: [u8; 8] = [26, 97, 138, 203, 132, 171, 141, 252];
fn process(program_id: &Pubkey, a: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if data == [1] {
        if !a[2].is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        let mut curve = a[0].try_borrow_mut_data()?;
        if curve[49..81] != a[2].key.to_bytes() {
            return Err(ProgramError::InvalidAccountData);
        }
        curve[49..81].copy_from_slice(a[1].key.as_ref());
        return Ok(());
    }
    if data.starts_with(&CREATE) {
        if !a[2].is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        let ix = Instruction {
            program_id: *a[8].key,
            accounts: vec![
                AccountMeta::new(*a[7].key, false),
                AccountMeta::new_readonly(*a[5].key, false),
                AccountMeta::new_readonly(*a[2].key, true),
            ],
            data: vec![1],
        };
        invoke(
            &ix,
            &[a[7].clone(), a[5].clone(), a[2].clone(), a[8].clone()],
        )?;
        let mut d = a[5].try_borrow_mut_data()?;
        d.fill(0);
        d[..8].copy_from_slice(&[216, 74, 9, 0, 56, 140, 93, 75]);
        d[10] = 1;
        d[11..43].copy_from_slice(a[4].key.as_ref());
        d[43..75].copy_from_slice(a[2].key.as_ref());
        d[76..80].copy_from_slice(&1u32.to_le_bytes());
        d[80..112].copy_from_slice(a[2].key.as_ref());
        d[112..114].copy_from_slice(&10000u16.to_le_bytes());
        return Ok(());
    }
    if data.starts_with(&FINALIZE) {
        if !a[2].is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        let mut d = a[5].try_borrow_mut_data()?;
        if d[43..75] != a[2].key.to_bytes() || d[75] != 0 {
            return Err(ProgramError::InvalidAccountData);
        }
        d[75] = 1;
        let tail = &data[8..];
        d[76..76 + tail.len()].copy_from_slice(tail);
        return Ok(());
    }
    if data.starts_with(&DISTRIBUTE) {
        let d = a[3].try_borrow_data()?;
        let n = u32::from_le_bytes(
            d[76..80]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?,
        ) as usize;
        if a.len() != 12 + n {
            return Err(ProgramError::NotEnoughAccountKeys);
        }
        let rent = Rent::get()?.minimum_balance(0);
        let amount = a[4]
            .lamports()
            .checked_sub(rent)
            .ok_or(ProgramError::InsufficientFunds)?;
        let (_, bump) =
            Pubkey::find_program_address(&[b"creator-vault", a[3].key.as_ref()], program_id);
        let bump = [bump];
        let seeds: &[&[u8]] = &[b"creator-vault", a[3].key.as_ref(), &bump];
        let mut paid = 0u64;
        for i in 0..n {
            if d[80 + i * 34..112 + i * 34] != a[12 + i].key.to_bytes() {
                return Err(ProgramError::InvalidAccountData);
            }
            let bps = u16::from_le_bytes([d[112 + i * 34], d[113 + i * 34]]);
            let part = if i == n - 1 {
                amount - paid
            } else {
                (u128::from(amount) * u128::from(bps) / 10000) as u64
            };
            paid += part;
            let ix = anchor_lang::solana_program::system_instruction::transfer(
                a[4].key,
                a[12 + i].key,
                part,
            );
            invoke_signed(
                &ix,
                &[a[4].clone(), a[12 + i].clone(), a[5].clone()],
                &[seeds],
            )?;
        }
        return Ok(());
    }
    if data.starts_with(&CLAIM) {
        if !a[0].is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }
        let amount = TokenAccount::unpack(&a[2].try_borrow_data()?)?.amount;
        let ix =
            spl_token::instruction::transfer(a[5].key, a[2].key, a[3].key, a[1].key, &[], amount)?;
        let (_, bump) = Pubkey::find_program_address(&[b"creator_fee_vault_auth_seed"], program_id);
        let bump = [bump];
        invoke_signed(
            &ix,
            &[a[2].clone(), a[3].clone(), a[1].clone(), a[5].clone()],
            &[&[b"creator_fee_vault_auth_seed", &bump]],
        )?;
        return Ok(());
    }
    let _ = system_program::ID;
    Err(ProgramError::InvalidInstructionData)
}
