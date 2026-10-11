use anchor_lang::prelude::*;

use crate::{constants::*, error::PerpetuaError, state::Market};

/// F3: resolve prediction market — PERMISSIONLESS. Siapa pun boleh poke pas/pasca jatuh
/// tempo (resolver cuma bayar fee, NOL privilege). Kontrak mengunci harga feed (mark_price,
/// diisi keeper dari Pyth) jadi resolution_price. Nggak ada komite/oracle manusia yang
/// mutusin menang/kalah — feed-nya sendiri yang jadi hakim. Inti wedge: self-resolving.
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
    require!(market.mark_price > 0, PerpetuaError::InvalidPrice);

    // Bekukan funding TEPAT di jatuh tempo (bukan di `now` yang bisa telat di-poke).
    let expiry = market.expiry_ts;
    market.accrue_funding(expiry);

    // Kunci harga feed sebagai putusan. Pyth (via keeper ingress) yang nentuin.
    market.resolution_price = market.mark_price;
    market.resolved = true;

    msg!(
        "RESOLVED: expiry={} resolution_price={} (feed is judge, permissionless)",
        market.expiry_ts,
        market.resolution_price
    );
    Ok(())
}
