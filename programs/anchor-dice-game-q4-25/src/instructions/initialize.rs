use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke_signed, system_instruction},
    system_program::{transfer, Transfer},
};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub house: Signer<'info>,
    #[account(
        mut,
        seeds = [b"vault", house.key().as_ref()],
        bump
    )]
    /// CHECK: PDA validated by seeds; account is created/used as a system-owned vault.
    pub vault: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>
}

impl<'info> Initialize<'info> {
    pub fn init(&mut self, amount: u64, bumps: &InitializeBumps) -> Result<()> {
        if self.vault.lamports() == 0 {
            let create_ix = system_instruction::create_account(
                &self.house.key(),
                &self.vault.key(),
                1.max(Rent::get()?.minimum_balance(0)),
                0,
                &System::id(),
            );

            let signer_seeds: &[&[&[u8]]] =
                &[&[b"vault", &self.house.key().to_bytes(), &[bumps.vault]]];

            invoke_signed(
                &create_ix,
                &[
                    self.house.to_account_info(),
                    self.vault.to_account_info(),
                    self.system_program.to_account_info(),
                ],
                signer_seeds,
            )?;
        }

        let accounts = Transfer {
            from: self.house.to_account_info(),
            to: self.vault.to_account_info(),
        };

        let ctx = CpiContext::new(self.system_program.to_account_info(), accounts);

        transfer(ctx, amount)
    }
}
