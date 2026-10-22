use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::{
    constants::*,
    error::PerpetuaError,
    state::{Market, Position, Vault},
};

#[derive(Accounts)]
pub struct OpenPosition<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,
    #[account(
        mut,
        seeds = [MARKET_SEED, market.oracle_feed.as_ref(), market.expiry_ts.to_le_bytes().as_ref()],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,
    #[account(
        mut,
        seeds = [VAULT_SEED, market.key().as_ref()],
        bump = market.vault_bump,
    )]
    pub vault: Account<'info, Vault>,
    #[account(
        init,
        payer = trader,
        space = 8 + Position::INIT_SPACE,
        seeds = [POSITION_SEED, market.key().as_ref(), trader.key().as_ref()],
        bump
    )]
    pub position: Account<'info, Position>,
    pub system_program: Program<'info, System>,
}

pub fn handle_open_position(
    ctx: Context<OpenPosition>,
    collateral: u64,
    leverage: u64,
    is_long: bool,
) -> Result<()> {
    require!(collateral > 0, PerpetuaError::ZeroCollateral);
    require!(
        (1..=MAX_LEVERAGE).contains(&leverage),
        PerpetuaError::InvalidLeverage
    );
    require!(ctx.accounts.market.mark_price > 0, PerpetuaError::InvalidPrice);

    let now = Clock::get()?.unix_timestamp;
    // F3: prediction market yg udah resolve / lewat jatuh tempo nggak boleh dibuka posisi baru.
    if ctx.accounts.market.market_kind == MARKET_KIND_PREDICTION {
        require!(!ctx.accounts.market.resolved, PerpetuaError::AlreadyResolved);
        require!(now < ctx.accounts.market.expiry_ts, PerpetuaError::MarketExpired);
    }

    // Akru funding sampai sekarang SEBELUM skew & OI berubah (SIP-279).
    ctx.accounts.market.accrue_funding(now);

    let size = collateral
        .checked_mul(leverage)
        .ok_or(PerpetuaError::MathOverflow)?;

    // Skew-premium fill: harga isi = oracle * (1 + rata-rata premium sebelum&sesudah),
    // premium = skew / skew_scale, skew = total_long - total_short (bertanda). UKURAN posisi
    // trader yang GERAKIN harga isi, nggak ada yang ngetik harga. (Nol price-impact mati di sini.)
    let oracle = ctx.accounts.market.mark_price as i128;
    let skew_scale = ctx.accounts.market.skew_scale as i128;
    let skew_before =
        ctx.accounts.market.total_long as i128 - ctx.accounts.market.total_short as i128;
    let signed_size = if is_long { size as i128 } else { -(size as i128) };
    let skew_after = skew_before
        .checked_add(signed_size)
        .ok_or(PerpetuaError::MathOverflow)?;
    let one = PRICE_ONE as i128;
    let prem_before = skew_before.checked_mul(one).ok_or(PerpetuaError::MathOverflow)? / skew_scale;
    let prem_after = skew_after.checked_mul(one).ok_or(PerpetuaError::MathOverflow)? / skew_scale;
    let avg_prem = (prem_before + prem_after) / 2;
    let fill = oracle
        .checked_mul(one.checked_add(avg_prem).ok_or(PerpetuaError::MathOverflow)?)
        .ok_or(PerpetuaError::MathOverflow)?
        / one;
    require!(fill > 0, PerpetuaError::InvalidPrice);
    let entry_price = fill as u64;
    let entry_funding = ctx.accounts.market.cumulative_funding;
    let market_key = ctx.accounts.market.key();

    // Kunci jaminan ke kolam lawan-dagang.
    let cpi_accounts = system_program::Transfer {
        from: ctx.accounts.trader.to_account_info(),
        to: ctx.accounts.vault.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(anchor_lang::system_program::ID, cpi_accounts);
    system_program::transfer(cpi_ctx, collateral)?;

    let position = &mut ctx.accounts.position;
    position.owner = ctx.accounts.trader.key();
    position.market = market_key;
    position.is_long = is_long;
    position.size = size;
    position.collateral = collateral;
    position.entry_price = entry_price;
    position.entry_funding = entry_funding;
    position.bump = ctx.bumps.position;

    let market = &mut ctx.accounts.market;
    if is_long {
        market.total_long = market
            .total_long
            .checked_add(size)
            .ok_or(PerpetuaError::MathOverflow)?;
    } else {
        market.total_short = market
            .total_short
            .checked_add(size)
            .ok_or(PerpetuaError::MathOverflow)?;
    }

    msg!(
        "Position opened: {} size={} entry={} collateral={}",
        if is_long { "LONG" } else { "SHORT" },
        size,
        entry_price,
        collateral
    );
    Ok(())
}
