import { useCallback, useEffect, useMemo, useState } from 'react'
import { useConnection, useAnchorWallet, useWallet } from '@solana/wallet-adapter-react'
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui'
import { SystemProgram } from '@solana/web3.js'
import {
  BN, FEEDS, LAMPORTS, PRICE_ONE,
  feedPubkey, getProgram, marketPda, vaultPda, positionPda,
  pick, getBurnerKeypair, makeBurnerWallet,
} from './anchor'

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

type MarketState = { authority: string; markPrice: number; totalLong: number; totalShort: number } | null
type PositionState = { isLong: boolean; size: number; collateral: number; entryPrice: number } | null

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
    setBusy(true)
    say('Igniting market…')
    try {
      const p6 = pyth ?? Math.round(feed.fallback * PRICE_ONE)
      // Pool must cover a winning 5x close (+125% on collateral). 0.5 SOL was too thin → close-after-pump
      // hit InsufficientLiquidity (audit 4 Oct); 2 SOL covers the scripted demo with margin.
      const s = await (program.methods as any)
        .createMarket(feedKey, new BN(p6), new BN(2 * LAMPORTS))
        .accountsPartial({
          authority: publicKey,
          market: marketKey,
          vault: vaultKey,
          systemProgram: SystemProgram.programId,
        })
        .rpc()
      setSig(s)
      say('Market live — on-chain mark ready. Move the price with pump / crash (demo).')
      setKeeperOn(true)
      await refresh()
    } catch (e: any) {
      say('Failed: ' + (e?.message ?? String(e)))
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

  const mark = market?.markPrice ?? pyth ?? 0
  let pnl = 0
  let equity = 0
  if (position && mark > 0 && position.entryPrice > 0) {
    const raw = (position.size * (mark - position.entryPrice)) / position.entryPrice
    pnl = position.isLong ? raw : -raw
    equity = position.collateral + pnl
  }
  const pnlPct = position && position.collateral > 0 ? (pnl / position.collateral) * 100 : 0
  const posLev = position ? Math.max(1, Math.round(position.size / position.collateral)) : 0
  const maint = position ? 0.05 * position.size : 0
  const liqP6 = position
    ? Math.round(position.isLong ? position.entryPrice * (1.05 - 1 / posLev) : position.entryPrice * (0.95 + 1 / posLev))
    : 0
  const healthFrac = position ? Math.max(0, Math.min(1, (equity - maint) / Math.max(1, position.collateral - maint))) : 0
  const chgPct = position && position.entryPrice > 0 ? ((mark - position.entryPrice) / position.entryPrice) * 100 : 0
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
                      ? 'market live'
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
                <p>Permissionless. Pick a feed, ignite, trade in ten seconds.</p>
                <button className="btn ignite" disabled={busy || !!market} onClick={createMarket}>
                  {market ? 'MARKET LIVE' : 'IGNITE MARKET'}
                </button>
              </div>
            </aside>

            {/* CENTER — market */}
            <main className="main">
              <div className="main-head">
                <span className="lbl">{feed.symbol} perpetual</span>
                <span className="sep" />
                <span className="meta">vAMM · on-chain settlement</span>
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
                </span>
              </div>

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
                        <div><div className="k">Open interest</div><div className="v num">{fmtSol(market.totalLong)}<small>SOL</small></div></div>
                        <div><div className="k">Oracle</div><div className="v">Pyth</div></div>
                        <div><div className="k">Counterparty</div><div className="v">vAMM</div></div>
                      </>
                    )}
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

                  {isAuthority && position && (
                    <div className="demostrip">
                      <span className="lbl">AUTHORITY · DEMO</span>
                      <button className="btn ghost" disabled={busy} onClick={pumpPrice}>pump +25%</button>
                      <button className="btn ghost" disabled={busy} onClick={crashPrice}>crash −25%</button>
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
                    <div className="row"><span className="k">Counterparty</span><span className="v">vAMM pool</span></div>
                  </div>

                  <div className="spacer" />
                  <button className="btn primary" disabled={busy || !market} onClick={openPosition}>
                    {market ? `OPEN ${isLong ? 'LONG' : 'SHORT'}` : 'IGNITE MARKET FIRST'}
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
