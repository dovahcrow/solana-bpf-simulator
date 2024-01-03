use solana_program::{
    account_info::AccountInfo, entrypoint, entrypoint::ProgramResult, msg, pubkey::Pubkey,
};

entrypoint!(process_instruction);
fn process_instruction<'a>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'a>],
    instruction_data: &[u8],
) -> ProgramResult {
    msg!("program_id: {}", program_id);
    msg!("ix: {:?}", instruction_data);

    for account in accounts {
        msg!("account: {}", account.key)
    }

    Ok(())
}
