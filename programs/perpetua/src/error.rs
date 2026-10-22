use anchor_lang::prelude::*;

#[error_code]
pub enum PerpetuaError {
    #[msg("Only the market authority may perform this action")]
    Unauthorized,
    #[msg("Leverage out of allowed range (1..=MAX_LEVERAGE)")]
    InvalidLeverage,
    #[msg("Collateral must be greater than zero")]
    ZeroCollateral,
    #[msg("Invalid market price (zero)")]
    InvalidPrice,
    #[msg("Liquidity pool has insufficient funds to cover the payout")]
    InsufficientLiquidity,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Position is still healthy — not eligible for liquidation")]
    PositionHealthy,
    #[msg("Skew scale (market depth) must be greater than zero")]
    InvalidSkewScale,
    #[msg("Invalid market kind (must be 0=perp or 1=prediction)")]
    InvalidMarketKind,
    #[msg("Prediction market expiry must be in the future")]
    InvalidExpiry,
    #[msg("This action requires a prediction market")]
    NotPredictionMarket,
    #[msg("Prediction market has not reached its expiry yet")]
    NotExpired,
    #[msg("Prediction market is already resolved")]
    AlreadyResolved,
    #[msg("Prediction market is expired — cannot open new positions")]
    MarketExpired,
    #[msg("Oracle account is not a Pyth PriceUpdateV2 account")]
    OracleInvalidAccount,
    #[msg("Oracle price is not fully verified by Wormhole guardians")]
    OracleNotFullyVerified,
    #[msg("Oracle price is for a different feed than this market")]
    OracleFeedMismatch,
    #[msg("Oracle confidence interval is too wide")]
    OracleConfidenceTooWide,
    #[msg("Oracle price is too old")]
    StaleOracle,
    #[msg("Oracle price was published before the market expiry")]
    OraclePriceBeforeExpiry,
    #[msg("Oracle price was published after the resolution window")]
    OraclePriceTooLate,
    #[msg("Prediction markets are priced by Pyth only — the creator cannot set the price")]
    CreatorPriceOnPrediction,
    #[msg("This market already has a Pyth price — the creator price override is retired")]
    CreatorPriceRetired,
    #[msg("Perp markets have no expiry (expiry_ts must be 0)")]
    PerpHasNoExpiry,
}
