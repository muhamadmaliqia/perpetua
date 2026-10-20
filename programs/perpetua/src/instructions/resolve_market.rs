use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::PerpetuaError,
    pyth::{read_verified_price, to_6dec_checked},
    state::Market,
};

/// F3+F2: resolve prediction market — PERMISSIONLESS. Siapa pun boleh poke pas/pasca jatuh
/// tempo (resolver cuma bayar fee, NOL privilege). Putusan = harga Pyth TERVERIFIKASI yang
/// dibaca kontrak sendiri dari akun PriceUpdateV2 — bukan mark_price, jadi pembuat pasar
/// (yang pegang set_mark_price) nggak bisa ngarang putusan. Harga wajib terbit di rentang
/// [expiry, expiry + RESOLVE_WINDOW_SECONDS]: bukan harga lama sebelum expiry, dan nggak bisa
/// nunggu harga enak berjam-jam.
#[derive(Accounts)]
pub struct ResolveMarket<'info> {
    /// Siapa pun. Cuma signer buat bayar fee; NOL privilege (benar-benar bebas-izin).
    pub resolver: Signer<'info>,
    #[account(
        mut,
        seeds = [MARKET_SEED, market.oracle_feed.as_ref()],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,
    /// CHECK: divalidasi manual di pyth::read_verified_price (owner Pyth Receiver,
    /// discriminator PriceUpdateV2, verifikasi Full, feed_id == market.oracle_feed).
    pub price_update: UncheckedAccount<'info>,
}

pub fn handle_resolve_market(ctx: Context<ResolveMarket>) -> Result<()> {
    let market = &mut ctx.accounts.market;
    require!(
        market.market_kind == MARKET_KIND_PREDICTION,
        PerpetuaError::NotPredictionMarket
    );
    require!(!market.resolved, PerpetuaError::AlreadyResolved);

    let now = Clock::get()?.unix_timestamp;
    require!(now >= market.expiry_ts, PerpetuaError::NotExpired);

    let p = read_verified_price(
        &ctx.accounts.price_update.to_account_info(),
        &market.oracle_feed.to_bytes(),
    )?;
    require!(
        p.publish_time >= market.expiry_ts,
        PerpetuaError::OraclePriceBeforeExpiry
    );
    require!(
        p.publish_time <= market.expiry_ts.saturating_add(RESOLVE_WINDOW_SECONDS),
        PerpetuaError::OraclePriceTooLate
    );
    let price = to_6dec_checked(&p)?;

    // Bekukan funding TEPAT di jatuh tempo (bukan di `now` yang bisa telat di-poke).
    let expiry = market.expiry_ts;
    market.accrue_funding(expiry);

    // Kunci harga Pyth sebagai putusan.
    market.resolution_price = price;
    market.mark_price = price;
    market.last_oracle_ts = p.publish_time;
    market.resolved = true;

    msg!(
        "RESOLVED: expiry={} resolution_price={} (Pyth verified, publish_time={}, permissionless)",
        market.expiry_ts,
        market.resolution_price,
        p.publish_time
    );
    Ok(())
}
