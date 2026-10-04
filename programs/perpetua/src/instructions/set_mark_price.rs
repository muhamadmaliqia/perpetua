use anchor_lang::prelude::*;

use crate::{constants::*, error::PerpetuaError, state::Market};

/// ⚠️ PLACEHOLDER H-1: harga acuan di-set manual oleh authority.
/// H-2: hapus instruksi ini, harga dibaca langsung dari feed Pyth di
/// dalam open_position / close_position.
#[derive(Accounts)]
pub struct SetMarkPrice<'info> {
    #[account(
        mut,
        seeds = [MARKET_SEED, market.oracle_feed.as_ref()],
        bump = market.bump,
        has_one = authority @ PerpetuaError::Unauthorized,
    )]
    pub market: Account<'info, Market>,
    pub authority: Signer<'info>,
}

pub fn handle_set_mark_price(ctx: Context<SetMarkPrice>, price: u64) -> Result<()> {
    require!(price > 0, PerpetuaError::InvalidPrice);
    ctx.accounts.market.mark_price = price;
    msg!("mark_price -> {}", price);
    Ok(())
}
