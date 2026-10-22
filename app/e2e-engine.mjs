// E2E ENGINE lawan validator lokal: angka yang DITAMPILIN UI harus SAMA PERSIS (ke lamport) sama angka on-chain.
// Rumusnya diambil dari src/engine.ts — file yang SAMA yang dipakai App.tsx (Est. fill, funding, equity, liq price).
//   Babak panggung (DEMO.md, angka App.tsx): XAU @ harga Pyth (clone) · seed 2 SOL · depth 25 SOL · LONG 0.5 SOL 5x ·
//     pump +25% dari entry · close · buka lagi · crash -25% · likuidasi sendiri.
//   Dua sisi: trader kedua SHORT ke pasar long-heavy · funding dua arah · batas harga likuidasi (±0.2%).
//   node e2e-engine.mjs
import { readFileSync } from 'fs'
import { homedir } from 'os'
import ts from 'typescript'
import anchor from '@coral-xyz/anchor'
const { AnchorProvider, Program, BN, Wallet } = anchor
import { Connection, PublicKey, Keypair, SystemProgram, LAMPORTS_PER_SOL } from '@solana/web3.js'

// src/engine.ts -> JS. Node dari apt nggak bisa baca .ts, jadi transpile pakai paket typescript app sendiri
// (yang sama dipake `tsc -b`). Satu sumber rumus buat UI + tes ini.
const engineSrc = readFileSync(new URL('./src/engine.ts', import.meta.url), 'utf8')
const engineJs = ts.transpileModule(engineSrc, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText
const E = await import('data:text/javascript;base64,' + Buffer.from(engineJs).toString('base64'))

const RPC = 'http://127.0.0.1:8899'
const SOL = LAMPORTS_PER_SOL
const XAU = '765d2ba906dbc32ca17cc11f5310a89e9ee1f6420508c63861f2f8ba4ee34bb2'
const US500 = 'e266cf0bf9a358bb19c4269f59b3d2e6258ecda5b0707e758f00c087f4baa3e9'
const SKEW_SCALE = new BN(25).mul(new BN(SOL)) // = anchor.ts SKEW_SCALE (depth demo)

const idl = JSON.parse(readFileSync(new URL('./src/idl/perpetua.json', import.meta.url)))
const kp = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(readFileSync(homedir() + '/.config/solana/id.json'))))
const conn = new Connection(RPC, 'confirmed')
const programFor = (signer) => new Program(idl, new AnchorProvider(conn, new Wallet(signer), { commitment: 'confirmed' }))
const program = programFor(kp)
const PID = new PublicKey(idl.address)
const pda = (seeds) => PublicKey.findProgramAddressSync(seeds, PID)[0]
// Perp slot = ["market", feed, expiry 0 as i64 LE].
const marketOf = (hex) => {
  const feed = new PublicKey(Buffer.from(hex, 'hex'))
  const market = pda([Buffer.from('market'), feed.toBuffer(), Buffer.alloc(8)])
  return { feed, market, vault: pda([Buffer.from('vault'), market.toBuffer()]) }
}
const positionOf = (market, owner) => pda([Buffer.from('position'), market.toBuffer(), owner.toBuffer()])

const big = (v) => BigInt(v.toString())
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const fail = (msg) => {
  console.log('E2E GAGAL:', msg)
  process.exit(1)
}
const same = (what, chain, engine) => {
  if (chain !== engine) fail(`${what}: on-chain ${chain} != engine ${engine}`)
  console.log(`    = ${what}: ${chain} (engine == on-chain)`)
}
const readMarket = async (market) => {
  const m = await program.account.market.fetch(market)
  return {
    markPrice: big(m.markPrice), totalLong: big(m.totalLong), totalShort: big(m.totalShort), skewScale: big(m.skewScale),
    cumulativeFunding: big(m.cumulativeFunding), fundingRate: big(m.fundingRate), lastFundingTs: big(m.lastFundingTs),
  }
}
const readPosition = async (position) => {
  const p = await program.account.position.fetch(position)
  return { isLong: p.isLong, size: big(p.size), collateral: big(p.collateral), entryPrice: big(p.entryPrice), entryFunding: big(p.entryFunding) }
}
// Angka yang program sendiri log-in (close: payout · likuidasi: equity + bounty).
const programLog = async (sig, marker) => {
  const tx = await conn.getTransaction(sig, { commitment: 'confirmed', maxSupportedTransactionVersion: 0 })
  const line = tx?.meta?.logMessages?.find((l) => l.includes(marker))
  if (!line) fail(`log "${marker}" nggak ketemu: ${JSON.stringify(tx?.meta?.logMessages)}`)
  return line
}
const logNum = (line, key) => BigInt(line.match(new RegExp(key + '=(-?\\d+)'))[1])

const create = (m, price6, signer = kp) =>
  programFor(signer).methods
    .createMarket(m.feed, new BN(price6.toString()), new BN(2 * SOL), SKEW_SCALE, 0, new BN(0))
    .accountsPartial({ authority: signer.publicKey, market: m.market, vault: m.vault, systemProgram: SystemProgram.programId })
    .rpc()
const open = (m, signer, collateral, leverage, isLong) =>
  programFor(signer).methods
    .openPosition(new BN(collateral.toString()), new BN(leverage), isLong)
    .accountsPartial({ trader: signer.publicKey, market: m.market, vault: m.vault, position: positionOf(m.market, signer.publicKey), systemProgram: SystemProgram.programId })
    .rpc()
const setMark = (m, price6) => program.methods.setMarkPrice(new BN(price6.toString())).accountsPartial({ market: m.market, authority: kp.publicKey }).rpc()
const close = (m, signer) =>
  programFor(signer).methods
    .closePosition()
    .accountsPartial({ trader: signer.publicKey, market: m.market, vault: m.vault, position: positionOf(m.market, signer.publicKey) })
    .rpc()
const liquidate = (m, owner) =>
  program.methods
    .liquidatePosition()
    .accountsPartial({ liquidator: kp.publicKey, market: m.market, vault: m.vault, owner, position: positionOf(m.market, owner) })
    .rpc()

const HALF = BigInt(SOL / 2)

// ━━ A. BABAK PANGGUNG (angka persis App.tsx + DEMO.md) ━━
const xau = marketOf(XAU)
// Starting price exactly like App.tsx: the latest Pyth price in the sponsored account on this cluster
// (e2e-local.sh clones the real devnet XAU account), else the reference price.
const xauAcct = await conn.getAccountInfo(new PublicKey('2uPQGpm8X4ZkxMHxrAW1QuhXcse1AHEgPih6Xp9NuEWW'))
const xauStart = xauAcct
  ? BigInt(Math.round(Number(xauAcct.data.readBigInt64LE(41 + 32)) * 10 ** (xauAcct.data.readInt32LE(41 + 48) + 6)))
  : 4_122_820_000n
console.log(`[A1] create_market XAU/USD perp @ $${Number(xauStart) / 1e6} (${xauAcct ? 'Pyth price, cloned from devnet' : 'reference price'}) · seed 2 SOL · depth 25 SOL`)
await create(xau, xauStart)

console.log('[A2] LONG 0.5 SOL 5x — "Est. fill" di order ticket vs entry on-chain')
const est1 = E.fillPrice(await readMarket(xau.market), HALF * 5n, true)
await open(xau, kp, HALF, 5, true)
const posA = positionOf(xau.market, kp.publicKey)
const a1 = await readPosition(posA)
same('entry LONG 2.5 SOL (fill premium +5%)', a1.entryPrice, est1)

await sleep(4000) // biar funding sempet jalan (pasar long-heavy -> long bayar)
const pump = (a1.entryPrice * 125n) / 100n // = tombol "pump +25%" (App.tsx: entry * 1.25)
console.log(`[A3] pump +25% -> $${Number(pump) / 1e6}, lalu CLOSE — payout on-chain vs equity UI (PnL - funding)`)
await setMark(xau, pump)
const xPre = await readMarket(xau.market)
const sigClose = await close(xau, kp)
const xPost = await readMarket(xau.market)
const acc = E.accrueFunding(xPre, xPost.lastFundingTs)
same('funding index pas close', xPost.cumulativeFunding, acc.cumulativeFunding)
same('funding rate pas close', xPost.fundingRate, acc.fundingRate)
const owed1 = E.fundingOwed(a1, xPost.cumulativeFunding)
if (owed1 <= 0n) fail(`LONG di pasar long-heavy mestinya BAYAR funding, owed=${owed1}`)
const payout1 = logNum(await programLog(sigClose, 'Position closed:'), 'payout')
same('payout close', payout1, E.closePayout(a1, pump, xPost.cumulativeFunding))
console.log(`    payout ${(Number(payout1) / 1e9).toFixed(6)} SOL · funding dibayar ${owed1} lamports`)

console.log('[A4] buka lagi LONG 0.5 SOL 5x, crash -25% dari entry, LIKUIDASI — liq price UI vs program')
const est2 = E.fillPrice(await readMarket(xau.market), HALF * 5n, true)
await open(xau, kp, HALF, 5, true)
const a2 = await readPosition(posA)
same('entry LONG ke-2', a2.entryPrice, est2)
const crash = (a2.entryPrice * 75n) / 100n // = tombol "crash -25%"
const liqUi = E.liquidationPrice(a2, 0n)
console.log(`    liq price UI $${liqUi / 1e6} · crash ke $${Number(crash) / 1e6} (di bawah liq -> mesti kelikuidasi)`)
if (Number(crash) >= liqUi) fail('crash -25% mestinya di bawah liq price 5x')
await setMark(xau, crash)
const sigLiq = await liquidate(xau, kp.publicKey)
const xLiq = await readMarket(xau.market)
const liqLine = await programLog(sigLiq, 'LIQUIDATION:')
same('equity pas likuidasi', logNum(liqLine, 'equity'), E.equityAfterFunding(a2, crash, xLiq.cumulativeFunding))
same('bounty likuidator (1% notional)', logNum(liqLine, 'bounty'), (a2.size * 100n) / 10_000n)

// ━━ B. DUA SISI: SHORT ke pasar long-heavy · funding dua arah · batas harga likuidasi ━━
const us = marketOf(US500)
const traderB = Keypair.generate()
await conn.confirmTransaction(await conn.requestAirdrop(traderB.publicKey, 5 * SOL), 'confirmed')
console.log('[B1] create_market US500 perp @ $6000 · A LONG 0.5 SOL 5x · B SHORT 0.5 SOL 3x')
await create(us, 6_000_000_000n)
const estL = E.fillPrice(await readMarket(us.market), HALF * 5n, true)
await open(us, kp, HALF, 5, true)
same('entry LONG A', (await readPosition(positionOf(us.market, kp.publicKey))).entryPrice, estL)
const estS = E.fillPrice(await readMarket(us.market), HALF * 3n, false)
await open(us, traderB, HALF, 3, false)
const b = await readPosition(positionOf(us.market, traderB.publicKey))
same('entry SHORT B (isi DI ATAS mark: dia ngurangin skew)', b.entryPrice, estS)

await sleep(3000)
console.log('[B2] batas likuidasi SHORT B: mark 0.2% di bawah liq price UI -> sehat, 0.2% di atas -> kelikuidasi')
// Persis cara UI: akru funding sampai jam on-chain SEKARANG, baru hitung liq price.
const chainNow = BigInt(await conn.getBlockTime(await conn.getSlot('confirmed')))
const owedB = E.fundingOwed(b, E.accrueFunding(await readMarket(us.market), chainNow).cumulativeFunding)
if (owedB >= 0n) fail(`SHORT di pasar long-heavy mestinya NERIMA funding, owed=${owedB}`)
const liqB = E.liquidationPrice(b, owedB)
console.log(`    liq price UI SHORT B $${liqB / 1e6} (funding diterima ${-owedB} lamports)`)
await setMark(us, BigInt(Math.round(liqB * 0.998)))
let healthyErr = null
try {
  await liquidate(us, traderB.publicKey)
} catch (e) {
  healthyErr = e?.message ?? String(e)
}
if (!healthyErr?.includes('PositionHealthy')) fail('0.2% sebelum liq price mestinya ditolak PositionHealthy, dapet: ' + (healthyErr ?? 'SUKSES'))
console.log('    = 0.2% sebelum liq price: DITOLAK PositionHealthy')
const markLiq = BigInt(Math.round(liqB * 1.002))
await setMark(us, markLiq)
const sigLiqB = await liquidate(us, traderB.publicKey)
const uLiq = await readMarket(us.market)
same('equity SHORT B pas likuidasi', logNum(await programLog(sigLiqB, 'LIQUIDATION:'), 'equity'), E.equityAfterFunding(b, markLiq, uLiq.cumulativeFunding))

console.log('[B3] CLOSE A (LONG) — payout on-chain vs engine, funding ngikut skew yang berubah')
const aUs = await readPosition(positionOf(us.market, kp.publicKey))
const uPre = await readMarket(us.market)
const sigCloseA = await close(us, kp)
const uPost = await readMarket(us.market)
same('funding index pas close A', uPost.cumulativeFunding, E.accrueFunding(uPre, uPost.lastFundingTs).cumulativeFunding)
same('payout close A', logNum(await programLog(sigCloseA, 'Position closed:'), 'payout'), E.closePayout(aUs, markLiq, uPost.cumulativeFunding))

console.log('\nE2E OK (engine) — fill, funding, payout, likuidasi: angka UI == angka on-chain.')
