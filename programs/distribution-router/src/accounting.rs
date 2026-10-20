use crate::{RouterError, BUCKETS};
use anchor_lang::prelude::*;
pub fn add(a: u64, b: u64) -> Result<u64> {
    a.checked_add(b)
        .ok_or_else(|| error!(RouterError::Overflow))
}
pub fn sub(a: u64, b: u64) -> Result<u64> {
    a.checked_sub(b)
        .ok_or_else(|| error!(RouterError::InsufficientFunds))
}
pub fn total(balances: &[u64; BUCKETS]) -> Result<u64> {
    balances.iter().try_fold(0, |sum, &n| add(sum, n))
}
pub fn spendable(account: &AccountInfo) -> Result<u64> {
    sub(
        account.lamports(),
        Rent::get()?.minimum_balance(account.data_len()),
    )
}
pub fn transfer_lamports(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<()> {
    require_keys_neq!(*from.key, *to.key, RouterError::InvalidRecipient);
    require!(spendable(from)? >= amount, RouterError::InsufficientFunds);
    from.sub_lamports(amount)?;
    to.add_lamports(amount)?;
    Ok(())
}
pub fn credit(balances: &mut [u64; BUCKETS], bucket: u8, amount: u64) -> Result<()> {
    let index = usize::from(bucket);
    require!(index < BUCKETS, RouterError::InvalidBucket);
    balances[index] = add(balances[index], amount)?;
    Ok(())
}
pub fn debit(balances: &mut [u64; BUCKETS], bucket: u8, amount: u64) -> Result<()> {
    let index = usize::from(bucket);
    require!(index < BUCKETS && amount > 0, RouterError::InvalidBucket);
    balances[index] = sub(balances[index], amount)?;
    Ok(())
}
