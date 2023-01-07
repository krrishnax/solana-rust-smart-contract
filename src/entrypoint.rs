use solana_program::{
    pubkey::Pubkey,
    account_info::AccountInfo,
    entrypoint,
    entrypoint::ProgramResult,
    msg, 
};

use crate::processor;

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8]
) -> ProgramResult {
    msg!(
        "process_instruction: {}: {} accounts, data={:?}",
        program_id,
        accounts.len(),
        instruction_data
    );
    processor::process_instruction(program_id, accounts, instruction_data)?;

    Ok(())
}