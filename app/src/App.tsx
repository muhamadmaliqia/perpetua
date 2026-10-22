import { useCallback, useEffect, useMemo, useState } from 'react'
import { useConnection, useAnchorWallet, useWallet } from '@solana/wallet-adapter-react'
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui'
import { PublicKey, SystemProgram } from '@solana/web3.js'
import {
  BN, FEEDS, LAMPORTS, PRICE_ONE, SKEW_SCALE, MARKET_KIND_PERP, MARKET_KIND_PREDICTION,
  feedPubkey, getProgram, marketPda, vaultPda, positionPda, pythPriceAccount, decodePythPrice,
  pick, getBurnerKeypair, makeBurnerWallet,
} from './anchor'
// Engine math mirrored from the program (fill, funding, liquidation), the same file the CLI e2e checks against.
import {
  accrueFunding, equityAfterFunding, exitPrice, fillPrice, fundingOwed, liquidationPrice, maintenance, ratePctPerHour,
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
const PARAMS = new URLSearchParams(window.location.search)
const RPC = PARAMS.get('rpc') || 'https://api.devnet.solana.com'
// Deep links: ?feed=SOL/USD&view=prediction opens that feed's prediction view directly.
const START_FEED = FEEDS.find((f) => f.symbol === PARAMS.get('feed'))?.symbol ?? FEEDS[0].symbol
const START_KIND = PARAMS.get('view') === 'prediction' ? MARKET_KIND_PREDICTION : MARKET_KIND_PERP
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
// Sign and colour at the shown precision: a value that rounds to zero is neutral (no red "-0.000" right after
// opening, when funding has only taken a few lamports).
const tone = (x: number, dp: number) => {
  const r = Number(x.toFixed(dp))
  return r > 0 ? 'up' : r < 0 ? 'down' : ''
}
const signed = (x: number, dp: number) => {
  const r = Number(x.toFixed(dp))
  return (r > 0 ? '+' : r < 0 ? '-' : '') + Math.abs(r).toFixed(dp)
}
const DURATIONS = [60, 300, 3600]
const MAX_LEVERAGE = 20 // constants.rs
const big = (v: any): bigint => BigInt(v.toString())

// `key` / `market` = the account address the data was fetched for, so a view switch never shows stale data.
type MarketState = {
  key: string; authority: string; markPrice: number; totalLong: number; totalShort: number
  kind: number; expiryTs: number; resolved: boolean; resolutionPrice: number; lastOracleTs: number
  eng: EngineMarket
} | null
type PositionState = { market: string; isLong: boolean; size: number; collateral: number; entryPrice: number; eng: EnginePosition } | null
// akun harga Pyth sponsored buat feed ini di cluster sekarang: harga + publish_time, atau 'missing'
type PythAcct = { price6: number; publishTime: number } | 'missing' | null
// satu ronde prediksi (pasar ber-expiry) buat feed ini; slot = (feed, expiry)
type PredRound = { key: string; expiryTs: number; resolved: boolean }

// Error resolve -> kalimat jujur buat statusbar (keputusan tetep di kontrak).
const explainResolve = (e: any, pythAcct: PythAcct, now: number) => {
  const m = e?.message ?? String(e)
  if (m.includes('OraclePriceBeforeExpiry'))
    return pythAcct && pythAcct !== 'missing'
      ? `Pyth hasn't published a price after expiry yet (last one ${fmtAgo(now - pythAcct.publishTime)}). Sponsored feeds update about hourly.`
      : "Pyth hasn't published a price after expiry yet."
  if (m.includes('OracleInvalidAccount')) return 'No Pyth price account for this feed on this cluster (localnet has none: use devnet or a Hermes-posted price).'
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

  const [feedSym, setFeedSym] = useState(START_FEED)
  const feed = useMemo(() => FEEDS.find((f) => f.symbol === feedSym)!, [feedSym])

  const [pythPrices, setPythPrices] = useState<Record<string, PythAcct>>({})
  const [marketRaw, setMarket] = useState<MarketState>(null)
  const [positionRaw, setPosition] = useState<PositionState>(null)
  const [balance, setBalance] = useState<number | null>(null)
  const [sig, setSig] = useState<string | null>(null)
  const [log, setLog] = useState('')
  const [busy, setBusy] = useState(false)
  const [keeperOn, setKeeperOn] = useState(false)

  const [collateral, setCollateral] = useState('0.5')
  const [leverage, setLeverage] = useState('5')
  const [isLong, setIsLong] = useState(true)

  // View: PERP (the feed's one perp, slot expiry 0) or PREDICTION (the feed's rounds, one slot per expiry).
  const [igniteKind, setIgniteKind] = useState(START_KIND)
  const [duration, setDuration] = useState(DURATIONS[0])
  const [chainOffset, setChainOffset] = useState(0) // detik: jam on-chain - jam lokal
  const [tick, setTick] = useState(() => Date.now())
  // prediction rounds of the feed they were fetched for (newest expiry first), tagged so a feed switch never
  // shows the previous feed's rounds
  const [predList, setPredList] = useState<{ sym: string; rounds: PredRound[] }>({ sym: '', rounds: [] })
  const [myMarkets, setMyMarkets] = useState<Set<string>>(() => new Set()) // markets where this wallet has a position
  const [pendingPred, setPendingPred] = useState<{ sym: string; key: string } | null>(null) // round just ignited here

  const feedKey = useMemo(() => feedPubkey(feed.pythId), [feed])
  const program = useMemo(() => (wallet ? getProgram(connection, wallet) : null), [connection, wallet])
  // Which market the center shows: the perp slot, or the prediction round you just started / hold a position in /
  // the newest one.
  const activeKeyStr = useMemo(() => {
    if (igniteKind === MARKET_KIND_PERP) return marketPda(feedKey, 0).toBase58()
    if (pendingPred && pendingPred.sym === feed.symbol) return pendingPred.key
    const rounds = predList.sym === feed.symbol ? predList.rounds : []
    const mine = rounds.find((r) => myMarkets.has(r.key))
    return (mine ?? rounds[0])?.key ?? null
  }, [igniteKind, feedKey, feed, pendingPred, predList, myMarkets])
  const marketKey = useMemo(() => (activeKeyStr ? new PublicKey(activeKeyStr) : null), [activeKeyStr])
  const market = marketRaw && marketRaw.key === activeKeyStr ? marketRaw : null
  const position = positionRaw && positionRaw.market === activeKeyStr ? positionRaw : null
  const vaultKey = useMemo(() => (marketKey ? vaultPda(marketKey) : null), [marketKey])
  const positionKey = useMemo(() => (marketKey && publicKey ? positionPda(marketKey, publicKey) : null), [marketKey, publicKey])

  // Latest Pyth price per feed: starting price at ignite + display. Feeds with no account on this cluster use the
  // reference price. (Only display, every on-chain decision reads Pyth itself.)
  const pythAcct: PythAcct = pythPrices[feed.symbol] ?? null
  const refPrice = (f: (typeof FEEDS)[number]) => {
    const a = pythPrices[f.symbol]
    return a && a !== 'missing' ? a.price6 : Math.round(f.fallback * PRICE_ONE)
  }
  const pyth = refPrice(feed)

  const say = (m: string) => setLog(m)

  const refresh = useCallback(async () => {
    if (!program) return
    if (!marketKey) {
      // prediction view with no round yet for this feed
      setMarket(null)
      setPosition(null)
      return
    }
    try {
      const m: any = await (program.account as any).market.fetchNullable(marketKey)
      setMarket(
        m
          ? {
              key: marketKey.toBase58(),
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
                market: marketKey.toBase58(),
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

  // Pyth's sponsored price accounts on this cluster (devnet: XAU/SOL/BTC/ETH; localnet: cloned by demo-local.sh).
  // Hermes itself is API-key gated since the Aug 26 2026 Core upgrade, so no live keeper here: on a perp without a
  // Pyth price the mark moves with the demo pump/crash controls.
  const refreshPythAll = useCallback(async () => {
    try {
      const infos = await connection.getMultipleAccountsInfo(FEEDS.map((f) => pythPriceAccount(f.pythId)))
      const next: Record<string, PythAcct> = {}
      FEEDS.forEach((f, i) => {
        const a = infos[i]
        next[f.symbol] = a ? decodePythPrice(a.data) ?? 'missing' : 'missing'
      })
      setPythPrices(next)
    } catch {
      /* keep the last prices */
    }
  }, [connection])

  // This feed's prediction rounds (newest expiry first) + the markets where this wallet holds a position.
  const refreshRounds = useCallback(async () => {
    if (!program) return
    try {
      // Market layout: 8 discriminator + 32 authority, then oracle_feed at offset 40.
      const all: any[] = await (program.account as any).market.all([{ memcmp: { offset: 40, bytes: feedKey.toBase58() } }])
      setPredList({
        sym: feedSym,
        rounds: all
          .filter((r) => Number(pick(r.account, 'marketKind', 'market_kind')) === MARKET_KIND_PREDICTION)
          .map((r) => ({
            key: r.publicKey.toBase58(),
            expiryTs: Number(pick(r.account, 'expiryTs', 'expiry_ts').toString()),
            resolved: !!pick(r.account, 'resolved', 'resolved'),
          }))
          .sort((a, b) => b.expiryTs - a.expiryTs),
      })
      if (publicKey) {
        // Position layout: 8 discriminator, then owner at offset 8.
        const mine: any[] = await (program.account as any).position.all([{ memcmp: { offset: 8, bytes: publicKey.toBase58() } }])
        setMyMarkets(new Set(mine.map((p) => pick(p.account, 'market', 'market').toBase58())))
      }
    } catch {
      /* next tick retries */
    }
  }, [program, feedKey, feedSym, publicKey])

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
    refreshBalance()
    const t = setInterval(() => {
      refresh()
      refreshBalance()
    }, 1500)
    return () => clearInterval(t)
  }, [refresh, refreshBalance])

  // Jam on-chain (bukan jam laptop): countdown & expiry ngikut Clock validator.
  const refreshChain = useCallback(async () => {
    try {
      const t = await connection.getBlockTime(await connection.getSlot('confirmed'))
      if (t != null) setChainOffset(t - Date.now() / 1000)
    } catch {
      /* ignore */
    }
  }, [connection])

  // Slower loop: chain clock, Pyth prices, and this feed's prediction rounds (a round you start shows right away
  // through pendingPred; rounds started by others appear within ~10 s).
  useEffect(() => {
    refreshChain()
    refreshPythAll()
    refreshRounds()
    const t = setInterval(() => {
      refreshChain()
      refreshPythAll()
      refreshRounds()
    }, 10000)
    return () => clearInterval(t)
  }, [refreshChain, refreshPythAll, refreshRounds])

  const isPred = market?.kind === MARKET_KIND_PREDICTION
  // On-chain clock ticks every 500 ms: the prediction countdown, funding (accrues every second on-chain) and
  // "Pyth published Xs ago" all follow it.
  useEffect(() => {
    const t = setInterval(() => setTick(Date.now()), 500)
    return () => clearInterval(t)
  }, [])
  const chainNow = tick / 1000 + chainOffset
  const expired = isPred && !!market && chainNow >= market.expiryTs
  const isPredView = igniteKind === MARKET_KIND_PREDICTION
  // Perp: one market per feed. Prediction: a new round can start once the shown one has expired (or none yet).
  const canIgnite = isPredView ? !market || market.resolved || expired : !market

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
      say('Airdrop failed (devnet rate limit, use faucet.solana.com): ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  const createMarket = async () => {
    if (!program || !publicKey) return
    const pred = igniteKind === MARKET_KIND_PREDICTION
    setBusy(true)
    say(pred ? 'Igniting prediction market…' : 'Igniting market…')
    try {
      // Starting price = the latest Pyth price for this feed when the cluster has its account (devnet; localnet
      // via demo-local.sh clones), else the reference price, for perps and predictions alike.
      const p6 = pyth
      let expiry = 0
      if (pred) {
        const t = await connection.getBlockTime(await connection.getSlot('confirmed'))
        expiry = (t ?? Math.floor(chainNow)) + duration
      }
      // Slot = (feed, expiry): perp = expiry 0; each prediction round gets its own market.
      const mKey = marketPda(feedKey, expiry)
      // Pool must cover a winning 5x close (+125% on collateral). 0.5 SOL was too thin → close-after-pump
      // hit InsufficientLiquidity (audit 4 Oct); 2 SOL covers the scripted demo with margin.
      const s = await (program.methods as any)
        .createMarket(feedKey, new BN(p6), new BN(2 * LAMPORTS), new BN(SKEW_SCALE), pred ? MARKET_KIND_PREDICTION : MARKET_KIND_PERP, new BN(expiry))
        .accountsPartial({
          authority: publicKey,
          market: mKey,
          vault: vaultPda(mKey),
          systemProgram: SystemProgram.programId,
        })
        .rpc()
      if (pred) setPendingPred({ sym: feed.symbol, key: mKey.toBase58() })
      setSig(s)
      say(
        pred
          ? `Prediction market live. Expires in ${fmtDur(duration)}, then anyone can resolve it from Pyth.`
          : 'Market live. On-chain mark ready, move the price with pump / crash (demo).',
      )
      setKeeperOn(!pred)
      await refresh()
    } catch (e: any) {
      say('Failed: ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  // F3+F2: resolve, anyone, after expiry. The contract reads the Pyth price account itself (owner, Full
  // verification, feed id, published inside [expiry, expiry+2h]) and locks it. Nobody types the price.
  const resolveMarket = async () => {
    if (!program || !publicKey || !marketKey) return
    setBusy(true)
    say('Resolving from Pyth…')
    try {
      const s = await (program.methods as any)
        .resolveMarket()
        .accountsPartial({ resolver: publicKey, market: marketKey, priceUpdate: pythPriceAccount(feed.pythId) })
        .rpc()
      setSig(s)
      say('Resolved: the contract read Pyth and locked the price. Close to settle.')
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
      say('Position closed. Payout sent to your wallet.')
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
      say('Position liquidated, bounty paid.')
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
    say('Simulating crash -25% (demo)…')
    try {
      const target = Math.round(position.entryPrice * 0.75)
      const s = await (program.methods as any)
        .setMarkPrice(new BN(target))
        .accountsPartial({ market: marketKey, authority: publicKey })
        .rpc()
      setSig(s)
      say('Price crashed -25%. Anyone can liquidate this position now.')
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
      say('Price pumped +25%. PnL green, close for payout.')
      await refresh()
    } catch (e: any) {
      say('Failed: ' + (e?.message ?? String(e)))
    }
    setBusy(false)
  }

  // A resolved prediction settles at the locked Pyth price (mirrors close_position on-chain).
  const mark = market?.resolved ? market.resolutionPrice : market?.markPrice ?? pyth ?? 0
  const pythReady = isPred && !!market && pythAcct && pythAcct !== 'missing' && pythAcct.publishTime >= market.expiryTs
  // Funding as the program would accrue it at this second, frozen at expiry once a prediction resolves
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
  const owedShown = Number(owedTxt) > 0 // first seconds accrue only a few lamports, no "-0.000000"
  // Exit price = what a close settles at: a resolved prediction at its locked price, otherwise the mark adjusted by
  // the closing order's skew premium (same rule as the fill on open), so an instant round trip is flat.
  const exit6 = position && market && mark > 0 ? (market.resolved ? BigInt(mark) : exitPrice(market.eng, BigInt(mark), position.eng)) : 0n
  const exitP6 = Number(exit6)
  let pnl = 0 // net of funding = what a close pays out beyond the collateral
  let equity = 0
  if (position && fund && exit6 > 0n && position.entryPrice > 0) {
    equity = Number(equityAfterFunding(position.eng, exit6, fund.cumulativeFunding))
    pnl = equity - position.collateral
  }
  const pnlPct = position && position.collateral > 0 ? (pnl / position.collateral) * 100 : 0
  const posLev = position ? Math.max(1, Math.round(position.size / position.collateral)) : 0
  const maint = position ? Number(maintenance(position.eng)) : 0
  const liqP6 = position && market ? liquidationPrice(market.eng, position.eng, owedBig) : 0
  const healthFrac = position ? Math.max(0, Math.min(1, (equity - maint) / Math.max(1, position.collateral - maint))) : 0
  // move of your exit price vs your entry (0% right after opening, since the premium comes back on close)
  const chgPct = position && position.entryPrice > 0 && exitP6 > 0 ? ((exitP6 - position.entryPrice) / position.entryPrice) * 100 : 0

  // Engine strip: skew (vAMM depth) + funding velocity, straight from on-chain state.
  const longOi = market?.totalLong ?? 0
  const shortOi = market?.totalShort ?? 0
  const oi = longOi + shortOi
  const premPct = market ? Number(skewPremium(market.eng)) / 1e4 : 0 // 6-dec fraction -> %
  const rateH = fund ? ratePctPerHour(fund.fundingRate) : 0
  // Order ticket: the fill the program would give this order right now, size moves the price.
  const colLam = Math.round((parseFloat(collateral) || 0) * LAMPORTS)
  const levN = parseInt(leverage) || 0
  const orderSize = colLam > 0 && levN >= 1 && levN <= MAX_LEVERAGE ? BigInt(colLam) * BigInt(levN) : 0n
  const estFill = market && orderSize > 0n && market.markPrice > 0 ? Number(fillPrice(market.eng, orderSize, isLong)) : 0
  const impactPct = market && estFill > 0 ? ((estFill - market.markPrice) / market.markPrice) * 100 : 0
  const feedShort = feed.pythId.slice(0, 4) + '…' + feed.pythId.slice(-4)

  const priceHero = () => {
    const p6 = market ? mark : pyth
    if (p6 == null) return <>-</>
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
            {/* LEFT: markets */}
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
                  const p6 = on && market ? mark : refPrice(f)
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
                        <div className={'mchg' + (on && position && tone(chgPct, 1) === 'up' ? ' up' : '')}>
                          {on && position ? `${signed(chgPct, 1)}%` : '-'}
                        </div>
                      </div>
                    </button>
                  )
                })}
              </div>
              <div className="ignite-box">
                <div className="rail-label">ANY PYTH FEED</div>
                <div className="seg kind">
                  <button className={igniteKind === MARKET_KIND_PERP ? 'active' : ''} onClick={() => setIgniteKind(MARKET_KIND_PERP)}>PERP</button>
                  <button className={igniteKind === MARKET_KIND_PREDICTION ? 'active' : ''} onClick={() => setIgniteKind(MARKET_KIND_PREDICTION)}>PREDICTION</button>
                </div>
                {isPredView && canIgnite && (
                  <div className="seg kind three">
                    {DURATIONS.map((d) => (
                      <button key={d} className={duration === d ? 'active' : ''} onClick={() => setDuration(d)}>
                        {d >= 3600 ? `${d / 3600}H` : `${d / 60}M`}
                      </button>
                    ))}
                  </div>
                )}
                <p>
                  {isPredView
                    ? 'Same engine, with an expiry. Anyone can start a round; when it expires, anyone resolves it; the contract reads the Pyth price itself.'
                    : 'Permissionless. Pick a feed, ignite, trade in ten seconds.'}
                </p>
                <button className="btn ignite" disabled={busy || !canIgnite} onClick={createMarket}>
                  {isPredView
                    ? canIgnite
                      ? `${market ? 'NEW ROUND' : 'IGNITE PREDICTION'} · ${fmtDur(duration)}`
                      : 'ROUND LIVE'
                    : market
                    ? 'MARKET LIVE'
                    : 'IGNITE MARKET'}
                </button>
              </div>
            </aside>

            {/* CENTER: market */}
            <main className="main">
              <div className="main-head">
                <span className="lbl">{feed.symbol} {isPredView ? 'prediction' : 'perpetual'}</span>
                <span className="sep" />
                <span className="meta">
                  {isPred && market ? `expires ${fmtClock(market.expiryTs)} · resolved from Pyth` : 'vAMM · on-chain settlement'}
                </span>
              </div>

              <div className="pxwrap">
                <span className={'pxbig num' + (keeperOn ? ' live' : '')}>{priceHero()}</span>
                {position && (
                  <div className="pxside">
                    <div className={'chg ' + tone(chgPct, 2)}>
                      {signed(chgPct, 2)}%
                    </div>
                    <div className="chsub">EXIT {fmtUsd(exitP6)} · FROM {fmtUsd(position.entryPrice)}</div>
                  </div>
                )}
              </div>

              <div className="oracle">
                <span className="pulse" />
                <span className="t">
                  oracle · Pyth feed <span className="dim">{feedShort}</span>
                  {!market &&
                    (pythAcct && pythAcct !== 'missing' ? (
                      <>
                        {' '}· latest Pyth price{' '}
                        <span className="dim">
                          published {fmtAgo(chainNow - pythAcct.publishTime)} · starting price at ignite
                        </span>
                      </>
                    ) : pythAcct === 'missing' ? (
                      <> · <span className="dim">no Pyth price account on this cluster · reference price</span></>
                    ) : null)}
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
                      <div className="sub">At expiry anyone can resolve it. The contract reads the Pyth price account itself. Nobody types the result in.</div>
                    </>
                  ) : !market.resolved ? (
                    <>
                      <div className="k">EXPIRED · AWAITING PYTH</div>
                      <div className="sub">
                        {pythAcct === 'missing'
                          ? 'No Pyth price account for this feed on this cluster.'
                          : pythAcct
                          ? `Latest Pyth price ${fmtUsd(pythAcct.price6)} published ${fmtAgo(chainNow - pythAcct.publishTime)}${
                              pythReady ? ' (after expiry), ready to resolve.' : ' (before expiry). Waiting for the next Pyth update (~hourly).'
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
                  {isPredView ? `No prediction round for ${feed.symbol} yet. Start one` : `No market for ${feed.symbol} yet. Ignite it`}{' '}
                  from the left: permissionless, ~10 seconds, you seed 2 SOL of liquidity.
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

                  {/* Engine: skew moves the fill, funding drifts with skew (SIP-279), live from on-chain state. */}
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
                        <div className={'big num ' + tone(pnl / LAMPORTS, 3)}>
                          {signed(pnl / LAMPORTS, 3)}
                        </div>
                        <div className="sub num">
                          SOL · {signed(pnlPct, 1)}% on collateral
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
                          <button className="btn ghost" disabled={busy} onClick={crashPrice}>crash -25%</button>
                        </>
                      ) : (
                        <span className="inlinelog">Price comes from Pyth. The creator override is retired on-chain.</span>
                      )}
                    </div>
                  )}
                </>
              )}
            </main>

            {/* RIGHT: order / manage */}
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
                    <div className="row"><span className="k">Exit price</span><span className="v num">{fmtUsd(exitP6)}</span></div>
                    <div className="row"><span className="k">Liq price</span><span className="v num down">{fmtUsd(liqP6)}</span></div>
                    <div className="row">
                      <span className="k">Funding</span>
                      <span className={'v num' + (owedShown && owed > 0 ? ' down' : owedShown && owed < 0 ? ' up' : '')}>
                        {owedShown ? (owed > 0 ? '-' : '+') : ''}
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
