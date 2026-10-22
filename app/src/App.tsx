import { useCallback, useEffect, useMemo, useState } from 'react'
import { useConnection, useAnchorWallet, useWallet } from '@solana/wallet-adapter-react'
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui'
import { SystemProgram } from '@solana/web3.js'
import {
  BN, FEEDS, LAMPORTS, PRICE_ONE, SKEW_SCALE, MARKET_KIND_PERP, MARKET_KIND_PREDICTION,
  feedPubkey, getProgram, marketPda, vaultPda, positionPda, pythPriceAccount, decodePythPrice,
  pick, getBurnerKeypair, makeBurnerWallet,
} from './anchor'
// Engine math mirrored from the program (fill, funding, liquidation) — the same file the CLI e2e checks against.
import {
  accrueFunding, equityAfterFunding, fillPrice, fundingOwed, liquidationPrice, maintenance, ratePctPerHour,
  skewPremium, type EngineMarket, type EnginePosition,
} from './engine'

const fmtUsd = (p6: number) =>
  '$' + (p6 / PRICE_ONE).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })
const fmtSol = (lam: number) =>
  (lam / LAMPORTS).toLocaleString('en-US', { minimumFractionDigits: 3, maximumFractionDigits: 3 })
// split a 6-decimal price into dollars + cents for the decimal-scaled hero
const priceParts = (p6: number) => {
  const usd = p6 / PRICE_ONE
  const whole = Math.floor(usd)
  return { whole: whole.toLocaleString('en-US'), cents: Math.round((usd - whole) * 100).toString().padStart(2, '0') }
}
const fmtNum = (p6: number) => (p6 / PRICE_ONE).toLocaleString('en-US', { maximumFractionDigits: 2 })
const RPC = new URLSearchParams(window.location.search).get('rpc') || 'https://api.devnet.solana.com'
const isLocal = RPC.includes('127.0.0.1') || RPC.includes('localhost')
const explorer = (sig: string) =>
  isLocal
    ? `https://explorer.solana.com/tx/${sig}?cluster=custom&customUrl=${encodeURIComponent(RPC)}`
    : `https://explorer.solana.com/tx/${sig}?cluster=devnet`

// prediction countdown / "published 3m ago"
const fmtDur = (s: number) => {
  s = Math.max(0, Math.floor(s))
  if (s >= 3600) return `${Math.floor(s / 3600)}h ${String(Math.floor((s % 3600) / 60)).padStart(2, '0')}m`
  return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`
}
const fmtAgo = (s: number) => (s < 90 ? `${Math.max(0, Math.round(s))}s ago` : s < 5400 ? `${Math.round(s / 60)}m ago` : `${(s / 3600).toFixed(1)}h ago`)
const fmtClock = (unix: number) => new Date(unix * 1000).toLocaleTimeString('en-GB')
const fmtPct = (x: number, dp = 2) => `${x >= 0 ? '+' : ''}${x.toFixed(dp)}%`
const DURATIONS = [60, 300, 3600]
const MAX_LEVERAGE = 20 // constants.rs
const big = (v: any): bigint => BigInt(v.toString())

type MarketState = {
  authority: string; markPrice: number; totalLong: number; totalShort: number
  kind: number; expiryTs: number; resolved: boolean; resolutionPrice: number; lastOracleTs: number
  eng: EngineMarket
} | null
type PositionState = { isLong: boolean; size: number; collateral: number; entryPrice: number; eng: EnginePosition } | null
// akun harga Pyth sponsored buat feed ini di cluster sekarang: harga + publish_time, atau 'missing'
type PythAcct = { price6: number; publishTime: number } | 'missing' | null

// Error resolve -> kalimat jujur buat statusbar (keputusan tetep di kontrak).
const explainResolve = (e: any, pythAcct: PythAcct, now: number) => {
  const m = e?.message ?? String(e)
  if (m.includes('OraclePriceBeforeExpiry'))
    return pythAcct && pythAcct !== 'missing'
      ? `Pyth hasn't published a price after expiry yet (last one ${fmtAgo(now - pythAcct.publishTime)}). Sponsored feeds update about hourly.`
      : "Pyth hasn't published a price after expiry yet."
  if (m.includes('OracleInvalidAccount')) return 'No Pyth price account for this feed on this cluster (localnet has none — use devnet or a Hermes-posted price).'
  if (m.includes('OraclePriceTooLate')) return 'The first Pyth price after expiry was outside the 2h resolution window.'
  if (m.includes('NotExpired')) return 'Not expired yet.'
  return m
}

export default function App() {
  const { connection } = useConnection()
  const phantomWallet = useAnchorWallet()
  const { publicKey: phantomPk } = useWallet()
  // Localnet: use an auto-signing burner (no Phantom, no faucet). Devnet: the real Phantom wallet.
  const burner = useMemo(() => (isLocal ? getBurnerKeypair() : null), [])
  const burnerWallet = useMemo(() => (burner ? makeBurnerWallet(burner) : null), [burner])
  const wallet = isLocal ? burnerWallet : phantomWallet
  const publicKey = isLocal ? burner?.publicKey ?? null : phantomPk

  const [feedSym, setFeedSym] = useState(FEEDS[0].symbol)
  const feed = useMemo(() => FEEDS.find((f) => f.symbol === feedSym)!, [feedSym])

  const [pyth, setPyth] = useState<number | null>(null)
  const [market, setMarket] = useState<MarketState>(null)
  const [position, setPosition] = useState<PositionState>(null)
  const [balance, setBalance] = useState<number | null>(null)
  const [sig, setSig] = useState<string | null>(null)
  const [log, setLog] = useState('')
  const [busy, setBusy] = useState(false)
  const [keeperOn, setKeeperOn] = useState(false)

  const [collateral, setCollateral] = useState('0.5')
  const [leverage, setLeverage] = useState('5')
  const [isLong, setIsLong] = useState(true)

  // F3 prediction: jenis pasar yg mau dinyalain + durasi, jam on-chain (offset vs jam lokal), akun Pyth.
  const [igniteKind, setIgniteKind] = useState(MARKET_KIND_PERP)
  const [duration, setDuration] = useState(DURATIONS[0])
  const [chainOffset, setChainOffset] = useState(0) // detik: jam on-chain - jam lokal
  const [tick, setTick] = useState(() => Date.now())
  const [pythAcct, setPythAcct] = useState<PythAcct>(null)

  const feedKey = useMemo(() => feedPubkey(feed.pythId), [feed])
  const marketKey = useMemo(() => marketPda(feedKey), [feedKey])
  const vaultKey = useMemo(() => vaultPda(marketKey), [marketKey])
  const positionKey = useMemo(() => (publicKey ? positionPda(marketKey, publicKey) : null), [marketKey, publicKey])
  const program = useMemo(() => (wallet ? getProgram(connection, wallet) : null), [connection, wallet])

  const say = (m: string) => setLog(m)

  const refresh = useCallback(async () => {
    if (!program) return
    try {
      const m: any = await (program.account as any).market.fetchNullable(marketKey)
      setMarket(
        m
          ? {
              authority: pick(m, 'authority', 'authority').toString(),
              markPrice: Number(pick(m, 'markPrice', 'mark_price').toString()),
              totalLong: Number(pick(m, 'totalLong', 'total_long').toString()),
              totalShort: Number(pick(m, 'totalShort', 'total_short').toString()),
              kind: Number(pick(m, 'marketKind', 'market_kind')),
              expiryTs: Number(pick(m, 'expiryTs', 'expiry_ts').toString()),
              resolved: !!pick(m, 'resolved', 'resolved'),
              resolutionPrice: Number(pick(m, 'resolutionPrice', 'resolution_price').toString()),
              lastOracleTs: Number(pick(m, 'lastOracleTs', 'last_oracle_ts').toString()),
              eng: {
                markPrice: big(pick(m, 'markPrice', 'mark_price')),
                totalLong: big(pick(m, 'totalLong', 'total_long')),
                totalShort: big(pick(m, 'totalShort', 'total_short')),
                skewScale: big(pick(m, 'skewScale', 'skew_scale')),
                cumulativeFunding: big(pick(m, 'cumulativeFunding', 'cumulative_funding')),
                fundingRate: big(pick(m, 'fundingRate', 'funding_rate')),
                lastFundingTs: big(pick(m, 'lastFundingTs', 'last_funding_ts')),
              },
            }
          : null,
      )
      if (positionKey) {
        const p: any = await (program.account as any).position.fetchNullable(positionKey)
        setPosition(
          p
            ? {
                isLong: pick(p, 'isLong', 'is_long'),
                size: Number(pick(p, 'size', 'size').toString()),
                collateral: Number(pick(p, 'collateral', 'collateral').toString()),
                entryPrice: Number(pick(p, 'entryPrice', 'entry_price').toString()),
                eng: {
                  isLong: pick(p, 'isLong', 'is_long'),
                  size: big(pick(p, 'size', 'size')),
                  collateral: big(pick(p, 'collateral', 'collateral')),
                  entryPrice: big(pick(p, 'entryPrice', 'entry_price')),
                  entryFunding: big(pick(p, 'entryFunding', 'entry_funding')),
                },
              }
            : null,
        )
      }
    } catch {
      /* next refresh retries */
    }
  }, [program, marketKey, positionKey])

  // Pyth's public Hermes is API-key gated (since the Aug 26 2026 Core upgrade) and a push-every-3s keeper would
  // prompt a wallet signature each tick. The demo uses a reference price from the feed (the on-chain oracle_feed
  // stays the Pyth feed id); the mark is moved by the pump/crash controls. Production: a keyed off-chain Pyth keeper.
  const refreshPyth = useCallback(async () => {
    setPyth(Math.round(feed.fallback * PRICE_ONE))
  }, [feed])

  const refreshBalance = useCallback(async () => {
    if (!publicKey) return
    try {
      setBalance((await connection.getBalance(publicKey)) / LAMPORTS)
    } catch {
      /* ignore */
    }
  }, [connection, publicKey])

  useEffect(() => {
    refresh()
    refreshPyth()
    refreshBalance()
    const t = setInterval(() => {
      refresh()
      refreshPyth()
      refreshBalance()
    }, 1500)
    return () => clearInterval(t)
  }, [refresh, refreshPyth, refreshBalance])

  // Jam on-chain (bukan jam laptop): countdown & expiry ngikut Clock validator.
  const refreshChain = useCallback(async () => {
    try {
      const t = await connection.getBlockTime(await connection.getSlot('confirmed'))
      if (t != null) setChainOffset(t - Date.now() / 1000)
    } catch {
      /* ignore */
    }
  }, [connection])

  // Akun harga Pyth sponsored buat feed ini — cuma buat tampilan ("terakhir terbit"); kontrak baca sendiri.
  const refreshPythAcct = useCallback(async () => {
    try {
      const a = await connection.getAccountInfo(pythPriceAccount(feed.pythId))
      setPythAcct(a ? decodePythPrice(a.data) ?? 'missing' : 'missing')
    } catch {
      /* ignore */
    }
  }, [connection, feed])

  useEffect(() => {
    refreshChain()
    refreshPythAcct()
    const t = setInterval(() => {
      refreshChain()
      refreshPythAcct()
    }, 10000)
    return () => clearInterval(t)
  }, [refreshChain, refreshPythAcct])

  const isPred = market?.kind === MARKET_KIND_PREDICTION
  // On-chain clock ticks while a market exists: the prediction countdown and funding (which accrues
  // every second on-chain) both follow it.
  const hasMarket = !!market
  useEffect(() => {
    if (!hasMarket) return
    const t = setInterval(() => setTick(Date.now()), 500)
    return () => clearInterval(t)
  }, [hasMarket])
  const chainNow = tick / 1000 + chainOffset
  const expired = isPred && !!market && chainNow >= market.expiryTs

  // Localnet burner: auto top-up from the local validator (unlimited). Retries until the validator is up
  // and the burner is funded, so there is zero wallet/faucet friction.
  useEffect(() => {
    if (!isLocal || !publicKey) return
    let cancelled = false
    let tries = 0
    const fund = async () => {
      if (cancelled) return
      try {
        const b = await connection.getBalance(publicKey)
        if (b >= LAMPORTS) return // already funded
        const s = await connection.requestAirdrop(publicKey, 5 * LAMPORTS)
        await connection.confirmTransaction(s, 'confirmed')
        if (!cancelled) refreshBalance()
      } catch {
        tries++
        if (tries < 12 && !cancelled) setTimeout(fund, 2000) // validator may still be starting
      }
    }
    fund()
    return () => {
      cancelled = true
    }
  }, [publicKey, connection, refreshBalance])

  const isAuthority = !!(market && publicKey && market.authority === publicKey.toString())

  const airdrop = async () => {
    if (!publicKey) return
    setBusy(true)
    say('Requesting airdrop…')
    try {
      const s = await connection.requestAirdrop(publicKey, (isLocal ? 5 : 2) * LAMPORTS)
      await connection.confirmTransaction(s, 'confirmed')
      say('Airdrop received.')
      await refreshBalance()
    } catch (e: any) {
      say('Airdrop failed (devnet rate limit — use faucet.solana.com): ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  const createMarket = async () => {
    if (!program || !publicKey) return
    const pred = igniteKind === MARKET_KIND_PREDICTION
    setBusy(true)
    say(pred ? 'Igniting prediction market…' : 'Igniting market…')
    try {
      // Prediction: start from the latest Pyth price for this feed when the cluster has one (devnet), so the
      // bet is against the real price; perp golden path keeps the reference price, unchanged.
      const pythNow = pythAcct && pythAcct !== 'missing' ? pythAcct.price6 : null
      const p6 = pred && pythNow ? pythNow : pyth ?? Math.round(feed.fallback * PRICE_ONE)
      let expiry = 0
      if (pred) {
        const t = await connection.getBlockTime(await connection.getSlot('confirmed'))
        expiry = (t ?? Math.floor(Date.now() / 1000)) + duration
      }
      // Pool must cover a winning 5x close (+125% on collateral). 0.5 SOL was too thin → close-after-pump
      // hit InsufficientLiquidity (audit 4 Oct); 2 SOL covers the scripted demo with margin.
      const s = await (program.methods as any)
        // F3: perp = market_kind 0 + expiry 0 (golden path unchanged); prediction = kind 1 + on-chain expiry.
        .createMarket(feedKey, new BN(p6), new BN(2 * LAMPORTS), new BN(SKEW_SCALE), pred ? MARKET_KIND_PREDICTION : MARKET_KIND_PERP, new BN(expiry))
        .accountsPartial({
          authority: publicKey,
          market: marketKey,
          vault: vaultKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc()
      setSig(s)
      say(
        pred
          ? `Prediction market live — expires in ${fmtDur(duration)}, then anyone can resolve it from Pyth.`
          : 'Market live — on-chain mark ready. Move the price with pump / crash (demo).',
      )
      setKeeperOn(!pred)
      await refresh()
    } catch (e: any) {
      say('Failed: ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  // F3+F2: resolve — anyone, after expiry. The contract reads the Pyth price account itself (owner, Full
  // verification, feed id, published inside [expiry, expiry+2h]) and locks it. Nobody types the price.
  const resolveMarket = async () => {
    if (!program || !publicKey) return
    setBusy(true)
    say('Resolving from Pyth…')
    try {
      const s = await (program.methods as any)
        .resolveMarket()
        .accountsPartial({ resolver: publicKey, market: marketKey, priceUpdate: pythPriceAccount(feed.pythId) })
        .rpc()
      setSig(s)
      say('Resolved — the contract read Pyth and locked the price. Close to settle.')
      await refresh()
    } catch (e: any) {
      say('Not resolved: ' + explainResolve(e, pythAcct, chainNow))
    }
    setBusy(false)
  }

  const openPosition = async () => {
    if (!program || !publicKey || !positionKey) return
    setBusy(true)
    say('Opening position…')
    try {
      const col = Math.round(parseFloat(collateral) * LAMPORTS)
      const lev = parseInt(leverage)
      const s = await (program.methods as any)
        .openPosition(new BN(col), new BN(lev), isLong)
        .accountsPartial({
          trader: publicKey,
          market: marketKey,
          vault: vaultKey,
          position: positionKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc()
      setSig(s)
      say(`${isLong ? 'LONG' : 'SHORT'} ${lev}× opened.`)
      await refresh()
    } catch (e: any) {
      say('Failed: ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  const closePosition = async () => {
    if (!program || !publicKey || !positionKey) return
    setBusy(true)
    say('Closing position…')
    try {
      const s = await (program.methods as any)
        .closePosition()
        .accountsPartial({ trader: publicKey, market: marketKey, vault: vaultKey, position: positionKey })
        .rpc()
      setSig(s)
      say('Position closed — payout sent to your wallet.')
      await refresh()
    } catch (e: any) {
      say('Failed: ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  const liquidate = async () => {
    if (!program || !publicKey || !positionKey) return
    setBusy(true)
    say('Liquidating…')
    try {
      const s = await (program.methods as any)
        .liquidatePosition()
        .accountsPartial({
          liquidator: publicKey,
          market: marketKey,
          vault: vaultKey,
          owner: publicKey,
          position: positionKey,
        })
        .rpc()
      setSig(s)
      say('Position liquidated — bounty paid.')
      await refresh()
    } catch (e: any) {
      say('Failed (position may still be healthy): ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  const crashPrice = async () => {
    if (!program || !publicKey || !position) return
    setBusy(true)
    setKeeperOn(false)
    say('Simulating crash −25% (demo)…')
    try {
      const target = Math.round(position.entryPrice * 0.75)
      const s = await (program.methods as any)
        .setMarkPrice(new BN(target))
        .accountsPartial({ market: marketKey, authority: publicKey })
        .rpc()
      setSig(s)
      say('Price crashed −25% — anyone can liquidate this position now.')
      await refresh()
    } catch (e: any) {
      say('Failed: ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  const pumpPrice = async () => {
    if (!program || !publicKey || !position) return
    setBusy(true)
    setKeeperOn(false)
    say('Simulating pump +25% (demo)…')
    try {
      const target = Math.round(position.entryPrice * 1.25)
      const s = await (program.methods as any)
        .setMarkPrice(new BN(target))
        .accountsPartial({ market: marketKey, authority: publicKey })
        .rpc()
      setSig(s)
      say('Price pumped +25% — PnL green, close for payout.')
      await refresh()
    } catch (e: any) {
      say('Failed: ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  // A resolved prediction settles at the locked Pyth price (mirrors close_position on-chain).
  const mark = market?.resolved ? market.resolutionPrice : market?.markPrice ?? pyth ?? 0
  const pythReady = isPred && !!market && pythAcct && pythAcct !== 'missing' && pythAcct.publishTime >= market.expiryTs
  // Funding as the program would accrue it at this second — frozen at expiry once a prediction resolves
  // (close_position skips accrual on a resolved market).
  const frozen = !!market && isPred && market.resolved
  const fund = market
    ? frozen
      ? { fundingRate: market.eng.fundingRate, cumulativeFunding: market.eng.cumulativeFunding }
      : accrueFunding(market.eng, BigInt(Math.floor(chainNow)))
    : null
  const owedBig = position && fund ? fundingOwed(position.eng, fund.cumulativeFunding) : 0n
  const owed = Number(owedBig) // lamports: + = this position pays funding, - = it receives
  const owedTxt = (Math.abs(owed) / LAMPORTS).toFixed(6)
  const owedShown = Number(owedTxt) > 0 // first seconds accrue only a few lamports — no "−0.000000"
  let pnl = 0 // net of funding = what a close pays out beyond the collateral
  let equity = 0
  if (position && fund && mark > 0 && position.entryPrice > 0) {
    equity = Number(equityAfterFunding(position.eng, BigInt(mark), fund.cumulativeFunding))
    pnl = equity - position.collateral
  }
  const pnlPct = position && position.collateral > 0 ? (pnl / position.collateral) * 100 : 0
  const posLev = position ? Math.max(1, Math.round(position.size / position.collateral)) : 0
  const maint = position ? Number(maintenance(position.eng)) : 0
  const liqP6 = position ? liquidationPrice(position.eng, owedBig) : 0
  const healthFrac = position ? Math.max(0, Math.min(1, (equity - maint) / Math.max(1, position.collateral - maint))) : 0
  const chgPct = position && position.entryPrice > 0 ? ((mark - position.entryPrice) / position.entryPrice) * 100 : 0

  // Engine strip: skew (vAMM depth) + funding velocity, straight from on-chain state.
  const longOi = market?.totalLong ?? 0
  const shortOi = market?.totalShort ?? 0
  const oi = longOi + shortOi
  const premPct = market ? Number(skewPremium(market.eng)) / 1e4 : 0 // 6-dec fraction -> %
  const rateH = fund ? ratePctPerHour(fund.fundingRate) : 0
  // Order ticket: the fill the program would give this order right now — size moves the price.
  const colLam = Math.round((parseFloat(collateral) || 0) * LAMPORTS)
  const levN = parseInt(leverage) || 0
  const orderSize = colLam > 0 && levN >= 1 && levN <= MAX_LEVERAGE ? BigInt(colLam) * BigInt(levN) : 0n
  const estFill = market && orderSize > 0n && market.markPrice > 0 ? Number(fillPrice(market.eng, orderSize, isLong)) : 0
  const impactPct = market && estFill > 0 ? ((estFill - market.markPrice) / market.markPrice) * 100 : 0
  const feedShort = feed.pythId.slice(0, 4) + '…' + feed.pythId.slice(-4)

  const priceHero = () => {
    const p6 = market ? mark : pyth
    if (p6 == null) return <>—</>
    const { whole, cents } = priceParts(p6)
    return (
      <>
        <i>$</i>{whole}<small>.{cents}</small>
      </>
    )
  }

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">PERPETUA<span className="tick" /></div>
        <div className="tagline">THE MARKET NEVER CLOSES</div>
        <div className="top-right">
          <div className="net">
            <span className="dot" /> {isLocal ? 'localnet' : 'devnet'}
            {balance != null && <span className="bal">{balance.toFixed(2)} SOL</span>}
          </div>
          {publicKey && balance != null && balance < 1 && (
            <button className="btn ghost" disabled={busy} onClick={airdrop}>
              airdrop {isLocal ? '5' : '2'} SOL
            </button>
          )}
          {isLocal ? (
            <span
              style={{
                font: '500 10.5px/1 var(--mono)',
                color: 'var(--fg-dim)',
                border: '1px solid var(--line-strong)',
                padding: '9px 13px',
                borderRadius: 2,
              }}
            >
              {publicKey ? publicKey.toString().slice(0, 4) + '…' + publicKey.toString().slice(-4) : '…'} · burner
            </span>
          ) : (
            <WalletMultiButton />
          )}
        </div>
      </header>

      {!publicKey && (
        <div className="connect">
          <div className="tagline">THE MARKET NEVER CLOSES</div>
          <h1>Any feed.<br />Any direction.<br />Ten seconds.</h1>
          <div className="rule" />
          <WalletMultiButton />
          <div className="sub">SOL COLLATERAL · PYTH ORACLE · PERMISSIONLESS LIQUIDATION</div>
        </div>
      )}

      {publicKey && (
        <>
          <div className="panes">
            {/* LEFT — markets */}
            <aside className="rail-l">
              <div className="rail-label">MARKETS · {FEEDS.length}</div>
              <div className="mkt-list">
                {FEEDS.map((f) => {
                  const on = f.symbol === feedSym
                  const st = on
                    ? position
                      ? `${posLev}× ${position.isLong ? 'long' : 'short'} open`
                      : market
                      ? isPred
                        ? market.resolved
                          ? 'resolved'
                          : 'prediction live'
                        : 'market live'
                      : 'no market'
                    : 'tap to view'
                  const p6 = on && market ? mark : Math.round(f.fallback * PRICE_ONE)
                  return (
                    <button
                      key={f.symbol}
                      className={'mkt' + (on ? ' active' : '')}
                      onClick={() => setFeedSym(f.symbol)}
                    >
                      <div>
                        <div className="sym">{f.symbol}</div>
                        <div className="st">{st}</div>
                      </div>
                      <div className="r">
                        <div className="mpx">{fmtNum(p6)}</div>
                        <div className={'mchg' + (on && position && pnl >= 0 ? ' up' : '')}>
                          {on && position ? `${chgPct >= 0 ? '+' : ''}${chgPct.toFixed(1)}%` : '—'}
                        </div>
                      </div>
                    </button>
                  )
                })}
              </div>
              <div className="ignite-box">
                <div className="rail-label">ANY PYTH FEED</div>
                <div className="seg kind">
                  <button className={igniteKind === MARKET_KIND_PERP ? 'active' : ''} disabled={!!market} onClick={() => setIgniteKind(MARKET_KIND_PERP)}>PERP</button>
                  <button className={igniteKind === MARKET_KIND_PREDICTION ? 'active' : ''} disabled={!!market} onClick={() => setIgniteKind(MARKET_KIND_PREDICTION)}>PREDICTION</button>
                </div>
                {igniteKind === MARKET_KIND_PREDICTION && !market && (
                  <div className="seg kind three">
                    {DURATIONS.map((d) => (
                      <button key={d} className={duration === d ? 'active' : ''} onClick={() => setDuration(d)}>
                        {d >= 3600 ? `${d / 3600}H` : `${d / 60}M`}
                      </button>
                    ))}
                  </div>
                )}
                <p>
                  {igniteKind === MARKET_KIND_PREDICTION
                    ? 'Same engine, with an expiry. When it expires, anyone resolves it — the contract reads the Pyth price itself.'
                    : 'Permissionless. Pick a feed, ignite, trade in ten seconds.'}
                </p>
                <button className="btn ignite" disabled={busy || !!market} onClick={createMarket}>
                  {market ? 'MARKET LIVE' : igniteKind === MARKET_KIND_PREDICTION ? `IGNITE PREDICTION · ${fmtDur(duration)}` : 'IGNITE MARKET'}
                </button>
              </div>
            </aside>

            {/* CENTER — market */}
            <main className="main">
              <div className="main-head">
                <span className="lbl">{feed.symbol} {isPred ? 'prediction' : 'perpetual'}</span>
                <span className="sep" />
                <span className="meta">
                  {isPred && market ? `expires ${fmtClock(market.expiryTs)} · resolved from Pyth` : 'vAMM · on-chain settlement'}
                </span>
              </div>

              <div className="pxwrap">
                <span className={'pxbig num' + (keeperOn ? ' live' : '')}>{priceHero()}</span>
                {position && (
                  <div className="pxside">
                    <div className={'chg ' + (pnl >= 0 ? 'up' : 'down')}>
                      {chgPct >= 0 ? '+' : ''}
                      {chgPct.toFixed(2)}%
                    </div>
                    <div className="chsub">FROM {fmtUsd(position.entryPrice)}</div>
                  </div>
                )}
              </div>

              <div className="oracle">
                <span className="pulse" />
                <span className="t">
                  oracle · Pyth feed <span className="dim">{feedShort}</span>
                  {market &&
                    (market.lastOracleTs > 0 ? (
                      <> · price verified on-chain <span className="dim">published {fmtClock(market.lastOracleTs)}</span></>
                    ) : isPred ? (
                      <> · <span className="dim">starting price set at creation · after that only Pyth can move it</span></>
                    ) : (
                      <> · <span className="dim">Creator-set mark (demo) · retires at the first Pyth price</span></>
                    ))}
                </span>
              </div>

              {market && isPred && (
                <div className="resolve">
                  {!expired ? (
                    <>
                      <div className="k">RESOLVES IN</div>
                      <div className="v num">{fmtDur(market.expiryTs - chainNow)}</div>
                      <div className="sub">At expiry anyone can resolve it. The contract reads the Pyth price account itself — nobody types the result in.</div>
                    </>
                  ) : !market.resolved ? (
                    <>
                      <div className="k">EXPIRED · AWAITING PYTH</div>
                      <div className="sub">
                        {pythAcct === 'missing'
                          ? 'No Pyth price account for this feed on this cluster.'
                          : pythAcct
                          ? `Latest Pyth price ${fmtUsd(pythAcct.price6)} published ${fmtAgo(chainNow - pythAcct.publishTime)}${
                              pythReady ? ' — after expiry, ready to resolve.' : ' — before expiry; waiting for the next Pyth update (~hourly).'
                            }`
                          : 'Checking Pyth…'}
                      </div>
                      <button className="btn primary" disabled={busy} onClick={resolveMarket}>RESOLVE FROM PYTH</button>
                    </>
                  ) : (
                    <>
                      <div className="k">RESOLVED · PYTH VERIFIED</div>
                      <div className="v num">{fmtUsd(market.resolutionPrice)}</div>
                      <div className="sub">Published {fmtClock(market.lastOracleTs)} · locked on-chain · positions settle at this price</div>
                    </>
                  )}
                </div>
              )}

              {!market ? (
                <div className="no-market">
                  No market for {feed.symbol} yet. Ignite it from the left — permissionless, ~10 seconds, you seed
                  2 SOL of liquidity.
                </div>
              ) : (
                <>
                  <div className="statrow">
                    {position ? (
                      <>
                        <div><div className="k">Size</div><div className="v num">{fmtSol(position.size)}<small>SOL</small></div></div>
                        <div><div className="k">Entry</div><div className="v num">{fmtUsd(position.entryPrice)}</div></div>
                        <div><div className="k">Liq price</div><div className="v num down">{fmtUsd(liqP6)}</div></div>
                        <div><div className="k">Equity</div><div className="v num">{fmtSol(equity)}<small>SOL</small></div></div>
                      </>
                    ) : (
                      <>
                        <div><div className="k">Mark</div><div className="v num">{fmtUsd(market.markPrice)}</div></div>
                        <div><div className="k">Open interest</div><div className="v num">{fmtSol(oi)}<small>SOL</small></div></div>
                        <div><div className="k">Price source</div><div className="v">{market.lastOracleTs > 0 ? 'Pyth ✓' : 'Creator-set'}</div></div>
                        <div><div className="k">Counterparty</div><div className="v">vAMM</div></div>
                      </>
                    )}
                  </div>

                  {/* Engine: skew moves the fill, funding drifts with skew (SIP-279) — live from on-chain state. */}
                  <div className="engine">
                    <div className="skewbox">
                      <div className="head">
                        <span className="k">
                          SKEW · {oi === 0 ? 'NO OPEN INTEREST' : longOi > shortOi ? 'LONG-HEAVY' : longOi < shortOi ? 'SHORT-HEAVY' : 'BALANCED'}
                        </span>
                        <span className="s num">{fmtSol(longOi)} long / {fmtSol(shortOi)} short</span>
                      </div>
                      <div className="bar">
                        <div className="l" style={{ width: (oi > 0 ? (longOi / oi) * 100 : 0) + '%' }} />
                        <div className="r" style={{ width: (oi > 0 ? (shortOi / oi) * 100 : 0) + '%' }} />
                      </div>
                      <div className="legend">
                        <span className="num">fill premium {fmtPct(premPct)}</span>
                        <span className="num">
                          depth {(Number(market.eng.skewScale) / LAMPORTS).toLocaleString('en-US', { maximumFractionDigits: 2 })} SOL
                        </span>
                      </div>
                    </div>
                    <div className="fundbox">
                      <div className="k">FUNDING / HOUR</div>
                      <div className="v num">{fmtPct(rateH, Math.abs(rateH) >= 1 ? 2 : 4)}</div>
                      <div className="sub">
                        {frozen ? 'frozen at expiry' : rateH > 0 ? 'longs pay shorts' : rateH < 0 ? 'shorts pay longs' : 'flat'}
                      </div>
                    </div>
                  </div>

                  {position && (
                    <div className="posrow">
                      <div className="health">
                        <div className="head">
                          <span className="k">POSITION HEALTH</span>
                          <span className={'s' + (healthFrac < 0.25 ? ' risk' : '')}>
                            {Math.round(healthFrac * 100)}% · {healthFrac < 0.25 ? 'AT RISK' : 'SAFE'}
                          </span>
                        </div>
                        <div className="bar">
                          <div
                            className={'fill' + (healthFrac < 0.25 ? ' risk' : '')}
                            style={{ width: Math.max(2, healthFrac * 100) + '%' }}
                          />
                        </div>
                        <div className="legend">
                          <span className="liqv num">LIQ {fmtUsd(liqP6)}</span>
                          <span className="num">MARK {fmtUsd(mark)}</span>
                        </div>
                      </div>
                      <div className="pnl">
                        <div className="k">UNREALIZED PNL</div>
                        <div className={'big num ' + (pnl >= 0 ? 'up' : 'down')}>
                          {pnl >= 0 ? '+' : ''}
                          {fmtSol(pnl)}
                        </div>
                        <div className="sub num">
                          SOL · {pnlPct >= 0 ? '+' : ''}
                          {pnlPct.toFixed(1)}% on collateral
                        </div>
                      </div>
                    </div>
                  )}

                  {/* Pump/crash = creator override. On-chain rule: perps only, and only until the market's
                      first Pyth price; prediction markets never (only Pyth moves them). */}
                  {isAuthority && position && !isPred && (
                    <div className="demostrip">
                      <span className="lbl">AUTHORITY · DEMO</span>
                      {market.lastOracleTs === 0 ? (
                        <>
                          <button className="btn ghost" disabled={busy} onClick={pumpPrice}>pump +25%</button>
                          <button className="btn ghost" disabled={busy} onClick={crashPrice}>crash −25%</button>
                        </>
                      ) : (
                        <span className="inlinelog">Price comes from Pyth — the creator override is retired on-chain.</span>
                      )}
                    </div>
                  )}
                </>
              )}
            </main>

            {/* RIGHT — order / manage */}
            <aside className="rail-r">
              {!position ? (
                <>
                  <div className="rail-label">ORDER</div>
                  <div className="seg">
                    <button className={'long' + (isLong ? ' active' : '')} onClick={() => setIsLong(true)}>LONG</button>
                    <button className={'short' + (!isLong ? ' active' : '')} onClick={() => setIsLong(false)}>SHORT</button>
                  </div>

                  <div className="field-label">COLLATERAL · SOL</div>
                  <div className="field">
                    <input className="num" value={collateral} onChange={(e) => setCollateral(e.target.value)} />
                    <span className="unit">SOL</span>
                  </div>

                  <div className="lev-head">
                    <span className="k">LEVERAGE</span>
                    <span className="v num">{Number(leverage) || 0}×</span>
                  </div>
                  <div className="field">
                    <input className="num" value={leverage} onChange={(e) => setLeverage(e.target.value)} />
                    <span className="unit">×</span>
                  </div>

                  <div className="summary">
                    <div className="row"><span className="k">Notional</span><span className="v num">{((Number(collateral) || 0) * (Number(leverage) || 0)).toFixed(2)} SOL</span></div>
                    {estFill > 0 && !expired && (
                      <>
                        <div className="row"><span className="k">Est. fill</span><span className="v num">{fmtUsd(estFill)}</span></div>
                        <div className="row"><span className="k">Price impact</span><span className="v num">{fmtPct(impactPct)}</span></div>
                      </>
                    )}
                    <div className="row"><span className="k">Counterparty</span><span className="v">vAMM pool</span></div>
                  </div>

                  <div className="spacer" />
                  <button className="btn primary" disabled={busy || !market || expired} onClick={openPosition}>
                    {!market ? 'IGNITE MARKET FIRST' : expired ? 'MARKET EXPIRED' : `OPEN ${isLong ? 'LONG' : 'SHORT'}`}
                  </button>
                </>
              ) : (
                <>
                  <div className="rail-label">POSITION · {position.isLong ? 'LONG' : 'SHORT'} {posLev}×</div>
                  <div className="summary" style={{ boxShadow: 'none', marginTop: 16, paddingTop: 0 }}>
                    <div className="row"><span className="k">Size</span><span className="v num">{fmtSol(position.size)} SOL</span></div>
                    <div className="row"><span className="k">Entry</span><span className="v num">{fmtUsd(position.entryPrice)}</span></div>
                    <div className="row"><span className="k">Mark</span><span className="v num">{fmtUsd(mark)}</span></div>
                    <div className="row"><span className="k">Liq price</span><span className="v num down">{fmtUsd(liqP6)}</span></div>
                    <div className="row">
                      <span className="k">Funding</span>
                      <span className={'v num' + (owedShown && owed > 0 ? ' down' : owedShown && owed < 0 ? ' up' : '')}>
                        {owedShown ? (owed > 0 ? '−' : '+') : ''}
                        {owedTxt} SOL
                      </span>
                    </div>
                    <div className="row"><span className="k">Equity</span><span className="v num">{fmtSol(equity)} SOL</span></div>
                  </div>
                  <div className="spacer" />
                  <button className="btn primary" disabled={busy} onClick={closePosition}>CLOSE POSITION</button>
                  <button className="btn out danger" disabled={busy} onClick={liquidate}>LIQUIDATE (demo)</button>
                </>
              )}
            </aside>
          </div>

          <div className="statusbar">
            <span>{log || 'ready.'}</span>
            {sig &&
              (isLocal ? (
                <span className="tx">tx {sig.slice(0, 8)}…{sig.slice(-8)} · localnet</span>
              ) : (
                <a className="tx" href={explorer(sig)} target="_blank" rel="noreferrer">
                  View last tx on Solana Explorer
                </a>
              ))}
          </div>
        </>
      )}
    </div>
  )
}
