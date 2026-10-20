//! Local-validator entrypoint for the real published SPL Token processor.
//! Test-only binary; it is never deployed by the V3 website or keeper.
use anchor_lang::solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, pubkey::Pubkey,
};
entrypoint!(process);
fn process(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    spl_token::processor::Processor::process(program_id, accounts, data)
}
