// Perpetua engine math: a line-for-line mirror of the on-chain program (programs/perpetua/src:
// open_position.rs fill · state.rs exit_price / accrue_funding / funding_owed / unrealized_pnl ·
// liquidate_position.rs maintenance). BigInt division truncates toward zero exactly like Rust's i128, so the UI
// and the CLI e2e (app/e2e-engine.mjs) predict the on-chain numbers to the lamport. Pure: no imports, no I/O.
// (The program uses saturating math; at these magnitudes it never saturates, so plain BigInt matches.)

export const PRICE_ONE = 1_000_000n // 6-decimal prices / fractions (1_000_000 = 1.0)
export const FUNDING_SCALE = 1_000_000_000n // funding index + rate resolution (1e9 = 1.0)
export const MAX_FUNDING_VELOCITY = 15n // FUNDING_SCALE per second, per second, at full skew
export const MAINTENANCE_MARGIN_BPS = 500n
export const BPS = 10_000n

export type EngineMarket = {
  markPrice: bigint
  totalLong: bigint
  totalShort: bigint
  skewScale: bigint
  cumulativeFunding: bigint
  fundingRate: bigint
  lastFundingTs: bigint
}

export type EnginePosition = {
  isLong: boolean
  size: bigint
  collateral: bigint
  entryPrice: bigint
  entryFunding: bigint
}

/** Signed skew in lamports: long open interest minus short. */
export const skew = (m: EngineMarket): bigint => m.totalLong - m.totalShort

/** Current skew premium as a 6-decimal fraction (skew / skew_scale). */
export const skewPremium = (m: EngineMarket): bigint => (skew(m) * PRICE_ONE) / m.skewScale

/** open_position fill: mark * (1 + average of the skew premium before and after this order). */
export function fillPrice(m: EngineMarket, size: bigint, isLong: boolean): bigint {
  const before = skew(m)
  const after = before + (isLong ? size : -size)
  const avg = ((before * PRICE_ONE) / m.skewScale + (after * PRICE_ONE) / m.skewScale) / 2n
  return (m.markPrice * (PRICE_ONE + avg)) / PRICE_ONE
}

/** Funding velocity (SIP-279): change of funding_rate per second, proportional to skew, clamped at |1|. */
export function fundingVelocity(m: EngineMarket): bigint {
  let prop = (skew(m) * FUNDING_SCALE) / m.skewScale
  if (prop > FUNDING_SCALE) prop = FUNDING_SCALE
  else if (prop < -FUNDING_SCALE) prop = -FUNDING_SCALE
  return (MAX_FUNDING_VELOCITY * prop) / FUNDING_SCALE
}

/** Market.accrue_funding(now) without mutating: the rate and index the program would hold at `now`. */
export function accrueFunding(m: EngineMarket, now: bigint): { fundingRate: bigint; cumulativeFunding: bigint } {
  const dt = now - m.lastFundingTs
  if (dt <= 0n) return { fundingRate: m.fundingRate, cumulativeFunding: m.cumulativeFunding }
  const fundingRate = m.fundingRate + fundingVelocity(m) * dt
  const avg = (m.fundingRate + fundingRate) / 2n
  return { fundingRate, cumulativeFunding: m.cumulativeFunding + avg * dt }
}

/** Position.funding_owed: + = the position pays, - = it receives (lamports). */
export function fundingOwed(p: EnginePosition, cumulativeFunding: bigint): bigint {
  const signed = p.isLong ? p.size : -p.size
  return (signed * (cumulativeFunding - p.entryFunding)) / FUNDING_SCALE
}

/** Position.unrealized_pnl at `mark` (lamports, signed). */
export function unrealizedPnl(p: EnginePosition, mark: bigint): bigint {
  if (p.entryPrice === 0n) return 0n
  const raw = (p.size * (mark - p.entryPrice)) / p.entryPrice
  return p.isLong ? raw : -raw
}

/** Average skew premium (6-dec fraction) for closing `p` now: skew before vs after removing it (Market::exit_price). */
export function exitPremium(m: EngineMarket, p: EnginePosition): bigint {
  const before = skew(m)
  const after = before - (p.isLong ? p.size : -p.size)
  return ((before * PRICE_ONE) / m.skewScale + (after * PRICE_ONE) / m.skewScale) / 2n
}

/** Exit price for close_position / liquidate_position: the mark adjusted by the closing order's skew premium, the same
 *  rule as the fill on open, so an instant round trip in a quiet market is flat. */
export const exitPrice = (m: EngineMarket, mark: bigint, p: EnginePosition): bigint =>
  (mark * (PRICE_ONE + exitPremium(m, p))) / PRICE_ONE

/** Equity after funding at an exit price, exactly as close_position / liquidate_position compute it. */
export const equityAfterFunding = (p: EnginePosition, exit: bigint, cumulativeFunding: bigint): bigint =>
  p.collateral + unrealizedPnl(p, exit) - fundingOwed(p, cumulativeFunding)

/** What close_position pays out at an exit price: max(0, equity after funding). */
export function closePayout(p: EnginePosition, exit: bigint, cumulativeFunding: bigint): bigint {
  const e = equityAfterFunding(p, exit, cumulativeFunding)
  return e > 0n ? e : 0n
}

/** Maintenance margin (lamports): the position is liquidatable once equity <= this. */
export const maintenance = (p: EnginePosition): bigint => (p.size * MAINTENANCE_MARGIN_BPS) / BPS

/** Mark (6-dec) at which equity after `owed` funding hits maintenance, valuing the position at its exit price. */
export function liquidationPrice(m: EngineMarket, p: EnginePosition, owed: bigint): number {
  const e = Number(p.entryPrice)
  const k = Number(maintenance(p) - p.collateral + owed) / Number(p.size)
  const exitAtLiq = p.isLong ? e * (1 + k) : e * (1 - k)
  return Math.max(0, Math.round(exitAtLiq / (1 + Number(exitPremium(m, p)) / 1e6)))
}

/** funding_rate (FUNDING_SCALE per second) as percent per hour. */
export const ratePctPerHour = (rate: bigint): number => (Number(rate) * 3600 * 100) / Number(FUNDING_SCALE)
