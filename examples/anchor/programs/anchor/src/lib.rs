use anchor_lang::prelude::*;

declare_id!("DUMMYPRoGRAM1111111111111111111111111111111");

#[program]
pub mod anchor_example {
    use super::*;

    pub fn initialize(_ctx: Context<Initialize>) -> Result<()> {
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    pub account: AccountInfo<'info>,
}
