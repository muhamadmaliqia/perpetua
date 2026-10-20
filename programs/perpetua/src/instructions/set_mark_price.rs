use anchor_lang::prelude::*;

use crate::{constants::*, error::PerpetuaError, state::Market};

/// OVERRIDE DEMO: harga acuan di-set manual oleh authority (pembuat pasar) — buat drama
/// pump/crash di demo & test deterministik. Harga asli masuk lewat update_mark_from_pyth.
/// NGGAK dipakai buat resolve prediction market (resolve baca Pyth sendiri). Nge-reset
/// last_oracle_ts ke 0 biar UI tau mark sekarang BUKAN dari Pyth.
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
    let market = &mut ctx.accounts.market;
    market.mark_price = price;
    market.last_oracle_ts = 0;
    msg!("mark_price -> {} (authority override, not Pyth)", price);
    Ok(())
}
