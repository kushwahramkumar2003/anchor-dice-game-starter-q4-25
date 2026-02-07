use anchor_lang::{
    prelude::*,
    solana_program::{
        sysvar::instructions::{
            load_current_index_checked, load_instruction_at_checked, ID as INSTRUCTIONS_SYSVAR_ID,
        },
    },
    system_program::{transfer, Transfer},
};

use crate::{errors::DiceError, state::Bet};

const ED25519_IX_DATA_LEN: usize = 16 + 64 + 32;
const ED25519_PROGRAM_ID: Pubkey = pubkey!("Ed25519SigVerify111111111111111111111111111");

#[derive(Accounts)]
pub struct ResolveBet<'info> {
    #[account(mut)]
    pub player: Signer<'info>,
    /// CHECK: House key is the authorized signer for result signatures.
    pub house: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"vault", house.key().as_ref()],
        bump
    )]
    pub vault: SystemAccount<'info>,
    #[account(
        mut,
        close = player,
        seeds = [b"bet", vault.key().as_ref(), bet.seed.to_le_bytes().as_ref()],
        bump = bet.bump,
        has_one = player
    )]
    pub bet: Account<'info, Bet>,
    /// CHECK: Instructions sysvar for instruction introspection.
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instruction_sysvar: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

impl<'info> ResolveBet<'info> {
    pub fn verify_ed25519_signature(&self, sig: &[u8]) -> Result<()> {
        let ix_sysvar = self.instruction_sysvar.to_account_info();
        let current_index = load_current_index_checked(&ix_sysvar)?;
        require!(current_index > 0, DiceError::Ed25519Header);

        let prev_ix = load_instruction_at_checked((current_index - 1) as usize, &ix_sysvar)?;

        require!(prev_ix.program_id == ED25519_PROGRAM_ID, DiceError::Ed25519Program);
        require!(prev_ix.accounts.is_empty(), DiceError::Ed25519Accounts);
        require!(prev_ix.data.len() >= ED25519_IX_DATA_LEN, DiceError::Ed25519DataLength);

        // Validate header for a single signature, then use offsets to verify payload.
        require!(prev_ix.data[0] == 1, DiceError::Ed25519Header);
        require!(prev_ix.data[1] == 0, DiceError::Ed25519Header);

        let signature_off = u16::from_le_bytes([prev_ix.data[2], prev_ix.data[3]]);
        let pubkey_off = u16::from_le_bytes([prev_ix.data[6], prev_ix.data[7]]);
        let msg_off = u16::from_le_bytes([prev_ix.data[10], prev_ix.data[11]]);
        let msg_size = u16::from_le_bytes([prev_ix.data[12], prev_ix.data[13]]) as usize;
        let signature_ix = u16::from_le_bytes([prev_ix.data[4], prev_ix.data[5]]);
        let pubkey_ix = u16::from_le_bytes([prev_ix.data[8], prev_ix.data[9]]);
        let msg_ix = u16::from_le_bytes([prev_ix.data[14], prev_ix.data[15]]);

        require!(signature_ix == u16::MAX, DiceError::Ed25519Header);
        require!(pubkey_ix == u16::MAX, DiceError::Ed25519Header);
        require!(msg_ix == u16::MAX, DiceError::Ed25519Header);
        require!(msg_size == sig.len(), DiceError::Ed25519Message);
        require!(
            prev_ix.data.len() >= signature_off as usize + 64,
            DiceError::Ed25519DataLength
        );
        require!(
            prev_ix.data.len() >= pubkey_off as usize + 32,
            DiceError::Ed25519DataLength
        );
        require!(
            prev_ix.data.len() >= msg_off as usize + msg_size,
            DiceError::Ed25519DataLength
        );

        let pubkey_start = pubkey_off as usize;
        let signature_start = signature_off as usize;
        let msg_start = msg_off as usize;

        require!(
            prev_ix.data[pubkey_start..pubkey_start + 32] == self.house.key().to_bytes(),
            DiceError::Ed25519Pubkey
        );
        require!(prev_ix.data[signature_start..signature_start + 64] != [0; 64], DiceError::Ed25519Signature);
        require!(&prev_ix.data[msg_start..msg_start + msg_size] == sig, DiceError::Ed25519Message);

        Ok(())
    }

    pub fn resolve_bet(&mut self, sig: &[u8], bumps: &ResolveBetBumps) -> Result<()> {
        require!(sig.len() == 1, DiceError::Ed25519Message);

        let result_roll = sig[0] % 100;
        if result_roll <= self.bet.roll {
            let payout = self
                .bet
                .amount
                .checked_mul(100)
                .ok_or(DiceError::Overflow)?
                .checked_div(self.bet.roll as u64)
                .ok_or(DiceError::Overflow)?;

            let accounts = Transfer {
                from: self.vault.to_account_info(),
                to: self.player.to_account_info(),
            };

            let signer_seeds: &[&[&[u8]]] = &[&[b"vault", &self.house.key().to_bytes(), &[bumps.vault]]];

            let ctx = CpiContext::new_with_signer(self.system_program.to_account_info(), accounts, signer_seeds);
            transfer(ctx, payout)?;
        }

        Ok(())
    }
}
