use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::PerpetuaError,
    pyth::{read_verified_price, to_6dec_checked},
    state::Market,
};

/// F2: dorong harga Pyth terverifikasi ke mark_price, PERMISSIONLESS. Siapa pun boleh
/// jadi keeper (cuma bayar fee): kontrak sendiri yang ngecek akun Pyth-nya asli, feed-nya
/// cocok, harganya seger (<= MAX_PRICE_AGE_SECONDS), dan nggak lebih lama dari yang terakhir.
/// Harga Pyth pertama yang masuk mensiunkan override pembuat pasar (set_mark_price) permanen.
#[derive(Accounts)]
pub struct UpdateMarkFromPyth<'info> {
    /// Siapa pun. Cuma signer buat bayar fee; NOL privilege.
    pub cranker: Signer<'info>,
    #[account(
        mut,
        seeds = [MARKET_SEED, market.oracle_feed.as_ref(), market.expiry_ts.to_le_bytes().as_ref()],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,
    /// CHECK: divalidasi manual di pyth::read_verified_price (owner Pyth Receiver,
    /// discriminator PriceUpdateV2, verifikasi Full, feed_id == market.oracle_feed).
    pub price_update: UncheckedAccount<'info>,
}

pub fn handle_update_mark_from_pyth(ctx: Context<UpdateMarkFromPyth>) -> Result<()> {
    let market = &mut ctx.accounts.market;
    require!(!market.resolved, PerpetuaError::AlreadyResolved);

    let p = read_verified_price(
        &ctx.accounts.price_update.to_account_info(),
        &market.oracle_feed.to_bytes(),
    )?;
    let now = Clock::get()?.unix_timestamp;
    require!(
        now.saturating_sub(p.publish_time) <= MAX_PRICE_AGE_SECONDS,
        PerpetuaError::StaleOracle
    );
    // Nggak boleh mundur ke harga yang lebih lama dari yang udah kepake (anti-replay).
    require!(
        p.publish_time >= market.last_oracle_ts,
        PerpetuaError::StaleOracle
    );

    let price = to_6dec_checked(&p)?;
    market.mark_price = price;
    market.last_oracle_ts = p.publish_time;

    msg!(
        "mark_price -> {} (Pyth verified, publish_time={})",
        price,
        p.publish_time
    );
    Ok(())
}
