use anchor_lang::prelude::*;

use crate::{error::EscrowError, Escrow, ESCROW_SEED};

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Update<'info> {
    pub maker: Signer<'info>,
    #[account(
        mut,
        has_one = maker,
        constraint = escrow.seed == seed,
        seeds = [ESCROW_SEED, maker.key().as_ref(), seed.to_le_bytes().as_ref()],
        bump = escrow.bump
    )]
    pub escrow: Account<'info, Escrow>,
}

impl<'info> Update<'info> {
    pub fn update(&mut self, receive: u64) -> Result<()> {
        require!(receive > 0, EscrowError::InvalidAmount);

        self.escrow.receive = receive;
        Ok(())
    }
}