use anchor_lang::prelude::*;

use crate::{constants::*, error::PerpetuaError, state::Market};

/// OVERRIDE DEMO: harga acuan di-set manual oleh authority (pembuat pasar), buat drama
/// pump/crash di demo & test deterministik. CUMA buat PERP yang BELUM PERNAH dapet harga Pyth:
/// - prediction market: ditolak selalu (harganya cuma boleh dari Pyth, termasuk putusannya);
/// - begitu harga Pyth pertama masuk (update_mark_from_pyth -> last_oracle_ts > 0, nggak pernah
///   balik 0), override ini PENSIUN permanen buat pasar itu, harga asli nggak bisa dikarang balik.
#[derive(Accounts)]
pub struct SetMarkPrice<'info> {
    #[account(
        mut,
        seeds = [MARKET_SEED, market.oracle_feed.as_ref(), market.expiry_ts.to_le_bytes().as_ref()],
        bump = market.bump,
        has_one = authority @ PerpetuaError::Unauthorized,
    )]
    pub market: Account<'info, Market>,
    pub authority: Signer<'info>,
}

pub fn handle_set_mark_price(ctx: Context<SetMarkPrice>, price: u64) -> Result<()> {
    require!(price > 0, PerpetuaError::InvalidPrice);
    let market = &mut ctx.accounts.market;
    require!(
        market.market_kind == MARKET_KIND_PERP,
        PerpetuaError::CreatorPriceOnPrediction
    );
    require!(market.last_oracle_ts == 0, PerpetuaError::CreatorPriceRetired);
    market.mark_price = price;
    msg!("mark_price -> {} (creator demo override; market has no Pyth price yet)", price);
    Ok(())
}
