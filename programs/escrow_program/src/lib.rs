pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("GGWVPdVZmY6bULgNnnEhTaq1h7UWMQMU2WW7Wk5hVfv2");

#[program]
pub mod escrow_program {
    use super::*;
   
   pub fn take(ctx: Context<Take>) -> Result<()> {
        ctx.accounts.take()
    }

    pub fn make(ctx: Context<Make>, seed: u64, deposit: u64, receive: u64) -> Result<()> {
        ctx.accounts.make(seed, deposit, receive, &ctx.bumps)
    }

     pub fn refund(ctx: Context<Refund>) -> Result<()> {
        ctx.accounts.refund()
    }

     pub fn update(ctx: Context<Update>, receive: u64) -> Result<()> {
        ctx.accounts.update(receive)
    }
}
 